# Admin 一键升级 — Beta 通道设计

**日期**：2026-05-20  
**目标版本**：v0.6.0-beta.3 hotfix  
**触发**：v0.6.0-beta.2 部署后 admin /admin/updates 显示乱（latest=v0.6.0-beta.1），根因是 prerelease 标志错 + `.env` URL hardcode。

## 1. 背景与问题

`/opt/wordforge/.env` 之前 `UPDATE_CHECK_API_URL=.../releases/tags/v0.6.0-beta.1` 是 v0.5.x 七连发期间为让 admin 一键升级在 beta 通道工作设的 workaround，事后没改回。`release.yml` 用 `softprops/action-gh-release@v2` 默认 prerelease 判定，v0.6.0-beta.1 标 `prerelease=true`、v0.6.0-beta.2 反而 `false` — 不稳。

**结构性矛盾**：beta tag 应当是 GitHub Pre-release（Latest badge 留稳定版）；但 `/releases/latest` 端点会跳过 prerelease，导致 admin 后台拉不到 beta latest，一键升级失效。

## 2. 目标

1. beta tag 在 GitHub 上正确标 prerelease=true；release.yml 显式锁死规则，不靠 action 默认
2. admin 后台 /admin/updates 同时展示 Stable 和 Beta 两个通道，各自可一键升级
3. 一次部署解决三件事：feature + workflow + .env URL 复位

## 3. 已决策

| # | 项 | 决定 |
|---|---|---|
| 1 | UI layout | 主卡片 + Beta 折叠区 |
| 2 | 主 / 折叠分配 | 主区域始终 Stable，Beta 始终折叠 |
| 3 | Beta 默认状态 | 默认折叠 + 有新版本时显示 badge |
| 4 | 后端拉法 | 单 URL `/releases?per_page=10`，后端 filter 双 latest |
| 5 | 跨通道 / 降级 | 严格 semver 向上才可点；跨通道升级允许；**前端**按钮 disabled 看 `hasUpdate`（即 strictly newer）；**后端**仍按 `allow_downgrade` flag 把关 apply（默认 false，敌进 422） |
| 6 | Channel 定义 | stable_latest = max semver where `prerelease=false`；beta_latest = max semver overall（含 prerelease，即"任何 release 里能拿到的最高"） |
| 7 | release.yml workflow | `prerelease: ${{ contains(github.ref_name, '-') }}` 显式锁死 |
| 8 | API 契约破坏性 | 直接替换 `UpdateStatus` 结构（admin 用户少，beta 期可控） |
| 9 | Release Notes 渲染 | 各通道卡片内嵌自己的 notes，不再有单独大块 |
| 10 | apply 路由 | `ApplyRequest` 加 `channel: "stable" \| "beta"`，后端验证 target = 该 channel latest |
| 11 | SSE 广播 | `release_available` 事件 payload 加 `channel`；前端按通道区分通知 |
| 12 | 发版承载 | v0.6.0-beta.3 hotfix（feature + workflow + .env 三件一笔） |

## 4. 架构

```
GitHub /releases?per_page=10 (JSON array)
        │  Updater::fetch_inner (ETag-aware)
        ▼
parse_release_list_payload → ParsedList { stable_latest, beta_latest }
        ▼
UpdaterCache { etag, last_checked_at, stable, beta }
        ▼
UpdateStatus { currentVersion, stable, beta, lastCheckedAt, ... }
        ▼
GET /api/admin/updates/status   →  双通道
POST /api/admin/updates/check   →  force refresh
POST /api/admin/updates/apply { channel, targetVersion, confirmCurrentVersion }
        ▼
SSE ReleaseAvailable { channel, latestTag }
        ▼
Frontend UpdatesPage:
  当前版本 StatCard
  Stable Card（主，含 notes + 升级按钮）
  Collapsible [▸ Beta 通道 · vX.Y.Z-beta.N ●]   ← badge = beta.hasUpdate 时
    └ Beta Card（与 Stable 同构）
```

**关键不变量**
1. `beta_latest_semver >= stable_latest_semver`（beta 是 stable 超集）
2. `has_update_for_channel = is_strictly_newer(channel.latest, current)` — 跟通道标签无关
3. apply 时后端只接 `target ∈ {stable_latest, beta_latest}`，不接第三 tag

## 5. Components

### 5.1 Backend — `src/services/updater.rs`

| 单元 | 改/新 | 接口 |
|---|---|---|
| `Channel` enum | 新 | `Stable` / `Beta`；wire lowercase；serde + FromStr |
| `CachedRelease` | 复用 | 单 release parsed 视图 |
| `UpdaterCache` | 改 | `latest: Option<CachedRelease>` → `stable: Option<CachedRelease>` + `beta: Option<CachedRelease>` |
| `parse_release_list_payload(body) -> ParsedList` | 新 | 入 JSON array；出 `{ stable_latest, beta_latest }`；逐项算 semver 分类取 max；复用现有单 release parser |
| `Updater::check_inner` | 改 | 解 list 不解 single release，写双 cache |
| `Updater::status_from(cache)` | 改 | 返 `UpdateStatus` 含 `stable / beta: Option<ChannelStatus>` |
| `Updater::apply(channel, target, confirm_current)` | 改签名 | 验证 target == cache.<channel>.tag |

### 5.2 Backend — routes & state

| 单元 | 改 | 改动 |
|---|---|---|
| `routes/admin/updates.rs::ApplyRequest` | 加 `channel: Channel` 必填 |
| `routes/admin/updates.rs::force_check` | broadcast 改：逐通道与 prev 对比 |
| `state.rs::SseEvent::ReleaseAvailable` | payload 加 `channel: Channel` |
| `workers/update_checker.rs::run` | 双通道分别 broadcast |
| `routes/admin/monitoring.rs::check_update` | **不动**：仪表盘那条仍返单 latest = stable_latest |
| `config.rs::UpdateCheckConfig::api_url` 默认值 | `/releases?per_page=10` |

### 5.3 Frontend — types / api

| 单元 | 改 | 接口 |
|---|---|---|
| `types/admin.ts::ChannelStatus` | 新 | `{ latestVersion, latestPublishedAt, releaseNotes, releaseUrl, hasUpdate, canApply }` |
| `types/admin.ts::UpdateStatus` | 替 | `{ currentVersion, stable: ChannelStatus \| null, beta: ChannelStatus \| null, lastCheckedAt, autoCheckEnabled, allowDowngrade }` |
| `api/admin.ts::updatesStatus/Check` | 类型同步 |
| `api/admin.ts::updatesApply(channel, targetVersion, confirmCurrentVersion)` | body 加 channel |
| `api/client.ts::SseCallbacks::onReleaseAvailable` payload | `{ latestTag, channel }` |

### 5.4 Frontend — UI

| 单元 | 改 |
|---|---|
| `pages/admin/UpdatesPage.tsx` | 大改：当前版本 StatCard + Stable Card + Collapsible Beta + Beta Card |
| `components/admin/UpdateChannelCard.tsx`（新组件） | 双通道复用：title / channelStatus / onApply props |
| `components/ui/Collapsible.tsx`（**新增**，项目当前无现成组件，grep `Collapsible\|Accordion` 0 命中） | 折叠条：`title / badge / defaultOpen / onToggle` props；ARIA `role="button" aria-expanded`；keyboard space/enter 触发 |

### 5.5 配置 + workflow

| 单元 | 改 |
|---|---|
| `.github/workflows/release.yml` | softprops/action-gh-release@v2 加 `prerelease: ${{ contains(github.ref_name, '-') }}` |
| `/opt/wordforge/.env` `UPDATE_CHECK_API_URL` | 改 `/releases?per_page=10` |

## 6. Data Flow

**A. 启动 / 周期检查** —— Updater::new 读 .update_etag → worker cron 每小时 → check_latest → 解 list → 写双 cache → 各通道与 prev 对比 broadcast。  
**B. admin 强制检查** —— POST /updates/check → force_check_latest → 同 A，跳 TTL。  
**C. 一键升级** —— POST /updates/apply {channel, targetVersion, confirmCurrentVersion} → 加锁 → 校验 confirm/target/strictly-newer → 202 立返 → 后台 download/verify/swap → SSE update_progress → systemd restart。  
**D. 折叠展开** —— 前端 createSignal<boolean>(false)；badge 由 status.beta?.hasUpdate 决定显示 latestVersion。

## 7. Error Handling

| 故障 | 处置 |
|---|---|
| GitHub list 5xx / 超时 | cache 不变，UI 显示「上次检查失败」吐司 |
| list 空 | cache 双字段 None，UI 显示「暂无可用版本」 |
| list 全 prerelease | stable_latest=None（UI「暂无稳定版」），Beta 正常 |
| 单 release 缺 tag_name | skip 该项 + tracing::warn |
| apply target mismatch | 422 + 前端 refetch status |
| apply 并发 | 复用 in-progress 锁 → 409 |
| SSE broadcast 时 cache None | skip 不发空 event |
| 前端收到旧版 payload（无 channel 字段） | fallback 当作 stable |

## 8. Testing

### 8.1 Rust unit (`src/services/updater.rs`)
- `parse_release_list_payload_picks_max_semver_per_channel`
- `parse_release_list_payload_handles_empty_array`
- `parse_release_list_payload_skips_missing_tag_name`
- `parse_release_list_payload_beta_is_superset_of_stable`（关键不变量）
- `status_from_double_channel`
- `apply_rejects_target_mismatch`
- `apply_accepts_cross_channel_upgrade`
- `apply_rejects_downgrade_without_flag`

### 8.2 Rust integration (`tests/updates_channel_http.rs` 新)
- GET /api/admin/updates/status 双通道返回正确
- POST /api/admin/updates/check force 路径
- POST /api/admin/updates/apply {channel} 不同通道

### 8.3 Frontend vitest
- `tests/api/admin.test.ts`：updatesStatus 解包双通道；updatesApply 带 channel
- `tests/pages/admin/UpdatesPage.test.tsx`：Stable 卡 + 折叠条 + Beta 渲染；badge 显示规则；跨通道按钮可点；降级 disabled
- SSE onReleaseAvailable `channel: "beta"` → 折叠区 badge

### 8.4 E2E (`frontend/e2e/admin-updates-channel.spec.ts`)
- 进 /admin/updates → 看到 Stable 卡 + 折叠条
- 点折叠条 → Beta 卡片可见
- mock backend：beta 升级按钮可点

## 9. 部署

1. 实现 + 测试通过
2. PR 合 main
3. 打 v0.6.0-beta.3 tag → release workflow 自动产 tar，**这次会标 prerelease=true**
4. 生产 SSH 升级（手动 / admin 一键，自动方式取决于当前 .env URL 状态）
5. 验证：admin /admin/updates 显示 Stable + Beta 双区，beta 区显示 latestVersion=v0.6.0-beta.3 与 current=v0.6.0-beta.3 一致 → has_update=false

## 10. 风险

- **未签名 admin 用户旧浏览器 tab 继续跑 v0.6.0-beta.2 SPA**：旧 SPA 调用 `/api/admin/updates/status` 拿到新结构会崩。已记忆条目 [paginated 字段名前端别写错] 强调 release 前 manual smoke。本次发布后通知用户 hard refresh。
- **GitHub /releases?per_page=10 仅取最近 10 个**：若长期不发版可能漏掉历史 latest。10 个对当前发版节奏足够。
- **softprops/action-gh-release@v2 的 `prerelease` 表达式**：需要确认该 action v2 接受 expression。已在 GitHub 文档确认支持 boolean 表达式。
