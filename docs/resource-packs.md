# 资源包热更（Resource Packs）

资源包是 WordForge 的**不发版内容热更**机制：把一份 `payload.json`（≤ 4 MiB 的不透明 JSON）经签名分发给客户端，让运营无需发版即可更新「已预埋消费点」的内容与配置。

> **一句话定位**：资源包是**「数据热更」不是「界面热更」**。它只能给客户端**预先写好消费代码**的位置喂新数据，**不能**下发新页面、新布局、新交互逻辑（不是 CodePush，也不是 server-driven UI）。能改到什么程度，取决于客户端提前埋了多少消费点。

## 一、链路总览

```
admin 上传 payload → 服务端算 SHA-256 + Ed25519 签名 → 落盘 static/packs/<pack>/<ver>/payload.json
        ↓ admin 切某通道「激活」
服务端广播 SSE resource_pack_available（同 pack×通道 5 分钟内去重）
        ↓ 客户端收到（或冷启动/手动检查）
GET /api/resource-packs/<packId>/manifest?appVersion=&locale=&channel=
        ↓ 下载 downloadURL → SHA-256 校验 → Ed25519 验签（硬编码生产公钥）→ minAppVersion 门控
注册的 consumer 消费 payload → 渲染 UI → POST /api/telemetry/resource-pack-install 上报结果
```

- **三通道**：`stable`（生产用户）/ `beta`（早期访问）/ `internal`（仅内部）。客户端冷启动默认拉 `stable`；切某通道激活只广播该通道。
- **验签信任锚**：三端**硬编码生产 Ed25519 公钥** `fr+eALsS/N3gz4AZmpSm/wDbtDCh596WjapwVPtHn6s=` 作为唯一信任锚；`GET /api/resource-packs/public-key` 仅供运行时自检对比（不一致仅告警，仍以硬编码为准），杜绝端点被替换架空验签。
- **缓存**：manifest `Cache-Control: max-age=60` + `ETag`（304）；payload 文件 `immutable, max-age=31536000`（版本号路径，永不变）。

## 二、两个通用容器

不要为每个用途新建 `packId`——投递链路对任意 `packId` 已通用，成本全在客户端消费侧，且每个新 `packId` 三端都要各发一次版。因此内容/配置类用途收敛到**两个通用容器**：

### 容器 A：`content-slots`（结构化内容位）

一次预埋覆盖 公告条 / 首页内容卡 / 激励语 / 空状态 / 推荐入口 / 更新提示等多种运营位。

```jsonc
{
  "schema": 1,
  "slots": {
    "global-announcement": [ /* ContentItem[]，全局公告条 */ ],
    "home-top":            [ /* 首页横幅下方内容位 */ ],
    "home-empty":          [ /* 首页空状态文案位 */ ],
    "update-notice":       [ /* 更新提示软文案（非强升） */ ]
  }
}
```

**ContentItem 字段**（三端逐字节一致）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 唯一 ID（关闭态、去重按此） |
| `kind` | string | `banner` / `notice` / `tip` / `entry` / `empty-state`；决定呈现样式 |
| `title` | string? | 标题 |
| `body` | string? | 副文案 |
| `imageUrl` | string? | 远程图（不占 4 MiB；无则走渐变兜底） |
| `deeplink` | string? | `wordforge://...`，见 §四 |
| `gradientStart` / `gradientEnd` | string? | `#RRGGBB` 渐变兜底色 |
| `level` | string? | `info` / `warning`（notice 配色） |
| `dismissible` | bool? | 可关闭（关闭态记本地） |
| `priority` | number | 降序在前 |
| `startAt` / `endAt` | string? | RFC3339 展示时间窗 |

### 容器 B：`app-config`（远程配置 / 开关）

```jsonc
{
  "schema": 1,
  "flags": {
    "announcements_enabled": true,      // 公告位总开关（kill-switch）
    "telemetry_sampling_rate": 1.0,     // 可丢弃遥测的采样率 [0,1]
    "llm_advisor_enabled": true         // 功能 kill-switch 示例
  }
}
```

`flags` 为 key → 标量（bool / number / string）。**语义是 kill-switch / 数值配置，不做百分比灰度**（客户端自决可被绕过，灰度靠 channel + `minAppVersion`）。

## 三、三端消费现状

| 端 | 消费实现 | 状态 |
|---|---|---|
| web | `src/resourcePack/`、`src/stores/appConfig.ts`、`src/components/content/{ContentSlot,AnnouncementBar}` | 两容器齐全 |
| Android | `core/di/AppContainer.kt` 的 `packAppliers` 注册 + Compose 消费 UI | 两容器齐全 |
| iOS | `ResourcePackStore`（@Published 响应式刷新）+ `RemoteAsset` 接入 HomeScreen | 两容器齐全 |

> **未注册的 packId 会被下载验签但「不应用」**（Android `ResourcePackSync` / iOS RemoteAsset 同理）。新增一个可热更的 UI 位 = 必须在客户端写消费代码 + **发版**。`homepage-banners`（首页横幅）是早期独立 pack，仍受支持。

## 四、统一 deeplink 契约

`ContentItem.deeplink` 用 `wordforge://` scheme，三端映射一致：

| path | web | Android | iOS |
|---|---|---|---|
| `wordforge://review` | `/flashcard` | 复习 | review tab |
| `wordforge://practice` | `/quick-practice` | 练习 | study tab |
| `wordforge://study` | `/learning` | 学习 | study tab |
| `wordforge://word/{id}` | `/vocabulary` | 词详情 | 忽略（无词详情路由） |
| `wordforge://tab/{section}` | 一级路由 | 一级导航 | tab/`<section>` |

`section` ∈ `home / review / practice / study / wordbooks / stats / profile`。无法映射的 deeplink 一律忽略（按钮禁用）。

## 五、admin 操作（在内嵌管理后台「资源包」页）

1. **上传新版**：填 `pack_id`（如 `content-slots`）、`version`（semver，单调递增）、通道、可选 `minAppVersion`；选 `payload.json` 上传。服务端自动算 SHA-256 + Ed25519 签名落盘。**上传 ≠ 生效**。
2. **激活**：在通道选择器选版本激活 —— 这一步才触发 SSE 广播并对该通道生效。
3. **观察**：卡片底部「近 7 天安装结果」+ 统计弹窗看 `installed / verify_failed / rollback` 三态。
4. **回滚 / 下线**：切回旧版本（旧文件保留）；或「停用」做软删除。

灰度建议：先 `internal` / `beta` 自测 → 再切 `stable` 放量。脚本化见 [API 接口](/api-endpoints) §22「资源包管理（admin）」。

### 「停用」的语义（重要）

停用 = **软删除**：只把该版本从 server manifest 摘除（之后 `GET manifest` 返回 404），磁盘 payload 文件保留。**不会远程擦除已安装客户端的内容**——客户端内容来自本地缓存，停用后再 check 拿到 404 即保持现状。要真正撤回客户端内容，须**发新版覆盖**（如 `banners: []` 的新版本）并激活，靠客户端 check 替换。注意停用**不发 SSE**（仅激活发）。

## 六、安全约束（实现与运维须共同遵守）

- **验签信任锚硬编码**：三端验签默认且仅用硬编码生产公钥；动态 `/public-key` 仅自检告警。**本地开发后端用的是开发密钥，签出的包生产客户端验签会失败**——勿用本地后端签的包测生产客户端。
- **bundle 默认值 = 安全态**：每个 flag 的客户端内置默认必须是安全态（iOS 崩溃环会 `deactivateAllActive` 回滚到 bundle，回滚后仍须安全）。flag 读取做**运行时类型归一**（远程误填 `"false"`/`0`/`""` 不得静默架空开关）。
- **`telemetry_sampling_rate` 绝不可门控设备存活心跳**：心跳须无条件 ≤ 10s 上报，否则服务端 watchdog 误报 `data_corrupted`。该 flag 只能门控**可丢弃的行为遥测**。
- **只装纯文本/纯数据**：禁止下发 HTML / 富文本 / 可执行片段（触界面热更红线 + App Store 2.3.1 / 3.1.1 审核）。
- **计费 / 权限类 flag**：不可仅信任客户端值，server 必须二次校验。
- **「秒级生效」仅对在线收到 SSE 的设备成立**（manifest `max-age=60`、SSE 5 分钟去重、离线靠冷启动），不可用作需强一致的安全熔断。

## 七、运维要点

- **签名密钥**：服务端 `data/keys/ed25519_resource_pack.{key,pub}`，上传时服务端用私钥签。轮换流程见 [密钥轮换](/runbook/key-rotation)，发布/排障流程见 [资源包运维](/runbook/resource-pack-ops)。
- **首个生产包须冒烟**：上传 + 激活首包后，用真机/浏览器确认 下载 → 验签通过 → 消费 渲染成功，再放量 `stable`。
- **新增消费点要发版**：admin 文案里的「TTS 语音、词书」等是规划用途，消费侧未实现的 packId 传了也只会「下载验签成功但不应用」。

## 相关文档

- [API 接口](/api-endpoints) §21 资源包热更 / §22 资源包管理（admin）
- [客户端上传数据规范](/client-upload-data) — 资源包安装遥测上报
- [资源包运维 Runbook](/runbook/resource-pack-ops)
- [密钥轮换](/runbook/key-rotation) — Ed25519 签名密钥
