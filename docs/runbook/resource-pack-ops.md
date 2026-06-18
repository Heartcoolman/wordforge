# Runbook：资源包发布与排障

> 适用：自托管 WordForge 生产环境
> 结构：发布/灰度按流程展开，故障部分按 **症状 → 诊断 → 处置** 三段式
> 前置：资源包的机制、容器约定、deeplink 契约与安全红线见 [资源包热更](/resource-packs)。本文只承接到**运维细节**，不重复其概念。

---

## 概述

资源包是「数据热更」机制：admin 上传一份 `payload.json` → 服务端自动签名落盘 → 切通道激活触发 SSE → 在线客户端拉取消费。本文覆盖发布 SOP、通道灰度、首包冒烟与常见故障。

涉及的运维资产：

| 资产 | 位置 | 说明 |
|---|---|---|
| 签名私钥 | `data/keys/ed25519_resource_pack.key` | Ed25519 原始私钥，权限 0600，首次启动自动生成 |
| 签名公钥 | `data/keys/ed25519_resource_pack.pub` | 原始公钥，权限 0644；客户端硬编码值才是验签信任锚 |
| payload 落盘根 | `static/packs/<pack>/<ver>/payload.json` | 版本号路径，永不变，`immutable` 长缓存 |
| 安装遥测 | `install_log`（DB） | `installed` / `verify_failed` / `rollback` 三态 |

> **路径说明**：keys 目录是 DB 文件（`DATABASE_URL`）所在目录下的 `keys/` 子目录。默认 `data/learning.db` → `data/keys/`。

---

## 发布流程（admin 操作 → 服务端自动化）

在内嵌管理后台「资源包」页操作；脚本化端点见 [API 接口](/api-endpoints) §22。

**第 1 步：上传新版**（`POST /api/admin/resource-packs/:pack_id/versions`）

填 `pack_id`（如 `content-slots` / `app-config`）、`version`（semver，单调递增）、`channel`、可选 `minAppVersion`，选 `payload.json` 上传。服务端自动完成：

1. 校验 query；payload 非空且 ≤ 4 MiB。
2. **去重**：`(pack_id, version)` 已存在则返回 `409 PACK_VERSION_EXISTS`，**绝不覆盖磁盘旧 payload**（重传同版本号会让已激活包的 sha256/签名与新字节不符 → 全量在线客户端验签必败「变砖」）。
3. 计算 SHA-256。
4. 用私钥做 Ed25519 签名（base64，88 字符含 padding）。
5. 落盘 `static/packs/<pack>/<ver>/payload.json`，并写 `resource_pack_versions` 表（含 sha256、签名、size、channel、minAppVersion）。
6. 写 admin 审计 `resource_pack.upload`。

> **上传 ≠ 生效**。此刻 manifest 尚不返回该版本，客户端拿不到。

**第 2 步：切通道激活**（`PUT /api/admin/resource-packs/:pack_id/channel/:channel/active`）

在通道选择器选版本激活。服务端：

1. 更新该通道激活版本（旧版本文件保留，仅 DB 指针变更）。
2. 写审计 `resource_pack.set_active`。
3. 广播 SSE `resource_pack_available`（payload：pack_id / version / channel），**同 pack × channel 5 分钟内去重**，去重命中只记日志不发。
4. 返回 `audienceClients` = 当前在线 SSE 连接数（切激活对话框「受众」预览）。

**第 3 步：客户端消费**

在线设备收到 SSE（或冷启动 / 手动 check）→ `GET /api/resource-packs/<packId>/manifest?appVersion=&locale=&channel=` → 下载 `downloadURL` → SHA-256 校验 → Ed25519 验签（硬编码生产公钥）→ `minAppVersion` 门控 → 注册的 consumer 消费 → `POST /api/telemetry/resource-pack-install` 上报 `installed` / `verify_failed` / `rollback`。

**第 4 步：观察**

卡片底部「近 7 天安装结果」+ 统计弹窗（`GET /api/admin/resource-packs/:pack_id/stats`）看三态计数。`verify_failed` / `rollback` 非零须立即排查（见下方故障部分）。

---

## 通道灰度（internal → beta → stable）

三通道语义见 [资源包热更 §一](/resource-packs)。灰度顺序固定为**逐级放量**，每级观察安装遥测无异常再进下一级：

1. **`internal`**：仅内部设备。切激活后用内部真机/浏览器跑通**首包冒烟清单**（见下节）。
2. **`beta`**：早期访问用户。观察 `verify_failed` / `rollback` 维持为 0、`installed` 正常增长，至少覆盖一个客户端冷启动周期。
3. **`stable`**：生产全量。客户端冷启动默认拉 `stable`。

要点：

- 切某通道激活**只广播该通道**；不同通道激活互不干扰，可并存不同版本。
- 灰度靠 `channel` + `minAppVersion`，**不靠百分比**（客户端自决可被绕过）。需要按 App 版本门控时上传时填 `minAppVersion`。
- 回滚 = 把目标通道切回旧版本（旧文件保留，秒级生效于在线设备）。回滚仍是一次「切激活」，同样受 5 分钟 SSE 去重约束。

---

## 首包冒烟检查清单（首个生产包必做）

新 `pack_id` 首次进 `stable` 前，或新增消费点后首次发布，必须在 `internal` 通道用**真机/真浏览器**逐项确认，不可只看 admin 返回：

- [ ] **上传成功**：admin 返回 `sha256` / `signature` / `sizeBytes`，无 409/500。
- [ ] **manifest 可达**：`GET /api/resource-packs/<pack>/manifest?appVersion=<真实版本>&locale=<真实locale>&channel=internal` 返回 200，`downloadURL` 指向 `/packs/<pack>/<ver>/payload.json`，`sha256` 与上传一致。
- [ ] **下载**：真机/浏览器实际拉到 payload，HTTP 200，字节数等于 `sizeBytes`。
- [ ] **验签通过**：客户端 Ed25519 验签成功（确认客户端硬编码公钥与本环境签名密钥匹配——本地开发后端签的包验签会失败，见下文）。
- [ ] **消费渲染**：注册的 consumer 实际渲染出内容（content-slots 看到运营位 / app-config flag 生效），不是「下载验签成功但不应用」。
- [ ] **遥测回写**：admin 统计弹窗出现 `installed`，且无 `verify_failed`。
- [ ] **minAppVersion 门控**（若设了）：用低于门槛的 App 版本请求 manifest 应被拒（`app_version_too_low`）。

全部通过后再逐级 `beta` → `stable` 放量。

---

## 故障排查

### 场景一：上传返回 500（落盘失败）

#### 症状
- `POST .../versions` 返回 500，body 含 `create_dir_all 失败` 或 `写 payload 失败`。

#### 诊断

```bash
INSTALL_DIR=/opt/wordforge

# 1. static/packs 目录是否可写、是否只读挂载
touch "$INSTALL_DIR/static/packs/.write-probe" && rm "$INSTALL_DIR/static/packs/.write-probe" \
  && echo "writable" || echo "READ-ONLY or missing"

# 2. 磁盘是否写满
df -h "$INSTALL_DIR"

# 3. 进程对 static/ 的属主/权限
ls -ld "$INSTALL_DIR/static" "$INSTALL_DIR/static/packs"
journalctl -u wordforge -n 100 | grep -i "payload\|create_dir\|packs"
```

#### 处置

- **只读文件系统 / 目录缺失**：确认 `static/packs` 存在且对运行 wordforge 的用户可写：

```bash
mkdir -p /opt/wordforge/static/packs
chown -R wordforge:wordforge /opt/wordforge/static/packs
```

- **磁盘写满**：按 [事故响应 §场景四](/runbook/incident-response) 清理后重传。
- **签名器未就绪**（返 `503 RESOURCE_PACK_SIGNER_UNAVAILABLE` 而非 500）：keys 目录初始化失败，见下方场景四。

> 落盘失败时 DB 不会写入版本记录（落盘在前，DB upsert 在后），不会产生「有 DB 记录无文件」的脏态；修复后用**同版本号重传**即可（因 409 去重只针对已成功入库的版本）。

---

### 场景二：切通道激活后客户端没收到

#### 症状
- admin 切激活返回成功，但目标通道设备未更新内容；统计无新增 `installed`。

#### 诊断（按链路逐段排除）

```bash
# 1. 通道是否匹配：客户端拉的 channel 与你激活的 channel 是否一致
#    冷启动客户端默认拉 stable；若只激活了 beta/internal，stable 用户拿不到
curl -sf "http://127.0.0.1:3000/api/resource-packs/<pack>/manifest?appVersion=9.9.9&locale=en-US&channel=stable" | jq '.'

# 2. manifest 是否 404：目标通道当前无激活版本 → 客户端拿不到
#    （RESOURCE_PACK_NOT_FOUND 即该 pack×channel 无激活）

# 3. SSE 是否连上：在线设备才能秒级收到；离线设备靠下次冷启动
curl -sf http://127.0.0.1:3000/metrics 2>/dev/null | grep sse_connections
journalctl -u wordforge -n 200 | grep -i "resource_pack_available\|已广播\|跳过 SSE"

# 4. 是否撞 5 分钟去重：日志出现「5 分钟内已广播过，本次激活跳过 SSE」
```

#### 处置

- **通道不匹配**：激活到客户端实际拉取的通道（生产用户用 `stable`）。
- **manifest 404**：目标通道无激活版本——确认该 `version` 已上传**且**已对该 `channel` 切激活（上传 ≠ 激活）。
- **SSE 未连**：秒级生效仅对在线收到 SSE 的设备成立。离线设备靠冷启动拉 `stable`（manifest `max-age=60`），属预期，无需处置；若大面积 SSE 断开见 [事故响应 §场景二](/runbook/incident-response)。
- **撞 5 分钟去重**：等去重窗口过后再激活，或确认设备会在冷启动/手动 check 时自行拉到（去重只影响 SSE 推送，不影响 manifest 内容）。

---

### 场景三：遥测出现 `verify_failed`

#### 症状
- 统计弹窗 `verify_failed` 计数 > 0；客户端日志报验签失败。

#### 诊断

最常见根因：**用本地/开发后端签的包，发给了硬编码生产公钥的客户端**。本地开发后端首次启动自动生成的是**独立开发密钥**，与生产签名密钥不同，生产客户端验签必败。

```bash
# 1. 本环境签名公钥（应等于客户端硬编码值 fr+eALsS/N3gz4AZmpSm/wDbtDCh596WjapwVPtHn6s=）
curl -sf http://127.0.0.1:3000/api/resource-packs/public-key | jq -r '.publicKey'

# 2. 比对客户端硬编码的生产公钥
#    不一致 = 包是用别的密钥签的，生产客户端验签必败
echo "fr+eALsS/N3gz4AZmpSm/wDbtDCh596WjapwVPtHn6s="

# 3. 也可能是 payload 字节被改动导致 sha256/签名不符（如重传覆盖——本服务已用 409 拦截）
```

#### 处置

- **公钥不匹配（开发密钥签了生产包）**：用**生产环境后端**重新上传该 payload（生产后端持生产私钥），再激活；切勿把本地后端签出的包发到生产通道。
- **公钥确实变更过**（做过密钥轮换但客户端未跟随发版）：见 [密钥轮换 §资源包签名密钥轮换](/runbook/key-rotation)——轮换必须随客户端发版同步硬编码新公钥，否则旧客户端验签全败。
- **疑似 payload 被篡改/覆盖**：本服务上传去重已阻止覆盖；若确实异常，上传**新版本号**重签重发。

---

### 场景四：上传返回 `503 RESOURCE_PACK_SIGNER_UNAVAILABLE`

#### 症状
- `POST .../versions` 返回 `503 RESOURCE_PACK_SIGNER_UNAVAILABLE`，「资源包签名器未初始化」。

#### 诊断

```bash
INSTALL_DIR=/opt/wordforge

# 启动日志确认签名器是否就绪
journalctl -u wordforge -n 500 | grep -i "资源包签名器"
#  就绪：「资源包签名器就绪」+ pubkey
#  失败：「资源包签名器初始化失败，相关端点将返 503」

# keys 目录权限/属主
ls -ld "$INSTALL_DIR/data/keys"
ls -l  "$INSTALL_DIR/data/keys/ed25519_resource_pack."*
```

#### 处置

- keys 目录无法创建/读写（属主或权限错）：修复后重启，服务会自动加载或生成密钥对。

```bash
chown -R wordforge:wordforge /opt/wordforge/data/keys
chmod 700 /opt/wordforge/data/keys
chmod 600 /opt/wordforge/data/keys/ed25519_resource_pack.key
systemctl restart wordforge
```

> **首次生成会产生新公钥**。生产环境严禁让其重新生成（会与客户端硬编码公钥不符）——keys 目录须随 DB 一同备份、随实例迁移，丢失等同于密钥轮换（须客户端重发版）。

---

## 停用 = 软删除语义（重要）

「停用」（`DELETE /api/admin/resource-packs/:pack_id/versions/:version`）是**软删除**，不是远程擦除：

- 仅把该版本从 server manifest 摘除——之后 `GET manifest` 对该通道返回 404；磁盘 payload 文件保留（约 30 天供回滚，GC 暂未实现）。
- **不会远程擦除已安装客户端的内容**：客户端内容来自本地缓存，停用后再 check 拿到 404 即**保持现状不变**。
- **停用不发 SSE**（仅切激活发），故已安装设备不会因停用而即时变化。

要真正撤回客户端已显示的内容，**必须发空内容新版覆盖**：

1. 上传新版本（如 content-slots 的 `{"schema":1,"slots":{}}`，或 app-config 把相关 flag 置安全态）。
2. 对目标通道**切激活**该新版（这一步才发 SSE / 改 manifest）。
3. 客户端 check 时拉到新版，渲染为空 → 完成撤回。

---

## 签名密钥保管规范

- **私钥 `ed25519_resource_pack.key` 权限必须 0600**，仅运行 wordforge 的用户可读（首次启动自动设 0600，迁移/恢复后须复核）。
- **私钥不入 git、不进仓库、不打印日志**：与 `data/` 同级，确保 `.gitignore` 覆盖 `data/keys/`。
- **keys 目录纳入备份**：与 DB 同备份策略（见 [备份恢复](/runbook/backup-restore)）。私钥丢失 = 无法再签新包，且重新生成会与客户端硬编码公钥不符（等同被迫轮换，须客户端重发版）。
- **公钥可公开**：`GET /api/resource-packs/public-key` 仅供客户端自检对比，**不是信任锚**——真正的信任锚是客户端硬编码的生产公钥 `fr+eALsS/N3gz4AZmpSm/wDbtDCh596WjapwVPtHn6s=`。
- 轮换流程见 [密钥轮换 §资源包签名密钥轮换](/runbook/key-rotation)。

---

## 相关文档

- [资源包热更](/resource-packs) — 机制 / 容器 / deeplink / 安全红线（权威说明）
- [密钥轮换](/runbook/key-rotation) — Ed25519 签名密钥轮换
- [事故响应](/runbook/incident-response) — SSE 打满 / 磁盘告警 / 5xx
- [API 接口](/api-endpoints) §21 资源包热更 / §22 资源包管理（admin）
- [客户端上传数据规范](/client-upload-data) — 安装遥测上报
