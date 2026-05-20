# admin /admin/updates Beta 通道 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans for inline batch execution. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** admin 后台 /admin/updates 同时显示 Stable + Beta 双通道，各自一键升级；后端拉 `/releases?per_page=10` 一次分流出双 latest；release.yml 显式锁死 prerelease 规则；`/opt/wordforge/.env` 复位。

**Architecture:** 单 GitHub fetch → 后端 parse list → 双 channel cache → status_from 返双 ChannelStatus → 前端 Stable 主卡 + Beta 折叠区。

**Tech Stack:** Rust 1.77 / axum / reqwest / semver / Solid.js / vitest / msw / playwright

**Target Release:** v0.6.0-beta.3（与本 hotfix 同一笔合 main 后 tag）

**Branch:** `release/v0.6.0-beta.3-hotfix`（已创建，spec 已 commit on this branch）

---

## File Structure

**Backend (Rust):**
- Modify `src/services/updater.rs` — Channel enum / UpdaterCache 双 field / parse_release_list_payload / status_from / check_inner / apply 签名
- Modify `src/routes/admin/updates.rs` — ApplyRequest.channel / force_check 双 broadcast / apply 转发 channel
- Modify `src/state.rs` — SseEvent::ReleaseAvailable payload + channel
- Modify `src/workers/update_checker.rs` — 双通道分别 broadcast
- Modify `src/config.rs` — UpdateCheckConfig::api_url default `/releases?per_page=10`
- Create `tests/updates_channel_http.rs` — 集成测：双通道 status / check / apply 路径

**Frontend (TS / Solid):**
- Modify `frontend/src/types/admin.ts` — ChannelStatus 接口 + AdminUpdateStatus 改 stable/beta 嵌套
- Modify `frontend/src/api/admin.ts` — updatesApply 加 channel 参数
- Modify `frontend/src/api/client.ts` — SseCallbacks.onReleaseAvailable payload 加 channel
- Create `frontend/src/components/ui/Collapsible.tsx` — 通用折叠条
- Create `frontend/src/components/admin/UpdateChannelCard.tsx` — 单通道卡片（双调用复用）
- Modify `frontend/src/pages/admin/UpdatesPage.tsx` — 主卡 + 折叠区 + 双 apply 路径
- Modify `frontend/tests/api/admin.test.ts` — updatesStatus 双通道契约 / updatesApply channel
- Modify `frontend/tests/pages/admin/UpdatesPage.test.tsx` — 双通道渲染
- Modify `frontend/tests/pages/admin/UpdatesPage.features.test.tsx` — badge / 跨通道按钮 / 折叠交互
- Create `frontend/tests/components/ui/Collapsible.test.tsx`

**Release / Deploy:**
- Modify `.github/workflows/release.yml` — softprops/action-gh-release@v2 加 `prerelease:` expression
- Modify `Cargo.toml` + `Cargo.lock` — bump 0.6.0-beta.2 → 0.6.0-beta.3
- SSH `/opt/wordforge/.env` 改 `UPDATE_CHECK_API_URL=.../releases?per_page=10`，restart service

---

## Task 1 — Channel enum

**Files:** `src/services/updater.rs` 新增枚举（紧贴 `pub enum UpdaterError` 之后）

- [ ] Step 1 · 添加测试

```rust
// 在 #[cfg(test)] mod tests {...} 内追加
#[test]
fn channel_serde_lowercase() {
    let s: Channel = serde_json::from_str("\"stable\"").unwrap();
    assert!(matches!(s, Channel::Stable));
    let b: Channel = serde_json::from_str("\"beta\"").unwrap();
    assert!(matches!(b, Channel::Beta));
    assert_eq!(serde_json::to_string(&Channel::Stable).unwrap(), "\"stable\"");
}
```

- [ ] Step 2 · 跑测验证 fail：`cargo test --lib services::updater::tests::channel_serde_lowercase 2>&1 | tail`
- [ ] Step 3 · 实现

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    pub fn as_str(&self) -> &'static str {
        match self { Channel::Stable => "stable", Channel::Beta => "beta" }
    }
}
```

（需要 `use serde::{Deserialize, Serialize};` 已有则 skip）

- [ ] Step 4 · 跑测：通过
- [ ] Step 5 · 不单独 commit，与下面合并

---

## Task 2 — UpdaterCache 拆双 field

**Files:** `src/services/updater.rs:138-143`

- [ ] Step 1 · 改 struct

```rust
#[derive(Default)]
struct UpdaterCache {
    last_checked_at: Option<DateTime<Utc>>,
    last_checked_instant: Option<Instant>,
    stable: Option<CachedRelease>,
    beta: Option<CachedRelease>,
    etag: Option<String>,
}
```

- [ ] Step 2 · `cargo check` — 应该出现大量编译错误（latest 引用处），逐个换为按 channel 访问
- [ ] Step 3 · 全部 `cache.latest` 替换：暂时把所有引用都改为 `cache.beta`（beta 是 superset，等 status_from 改完再分流），让 build 通
- [ ] Step 4 · `cargo build` 通过；提交点不到，与 task 3-5 合并

---

## Task 3 — ChannelStatus + UpdateStatus 改造

**Files:** `src/services/updater.rs:94-109`

- [ ] Step 1 · 加 ChannelStatus

```rust
/// 单通道视图：latest version + 升级判定。两通道共结构。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    pub latest_version: String,
    pub latest_published_at: Option<DateTime<Utc>>,
    pub release_notes: String,
    pub release_url: String,
    pub has_update: bool,
    pub can_apply: bool,
}
```

- [ ] Step 2 · 替换 UpdateStatus

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub stable: Option<ChannelStatus>,
    pub beta: Option<ChannelStatus>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub auto_check_enabled: bool,
    pub allow_downgrade: bool,
}
```

- [ ] Step 3 · `cargo check` — 现在 status_from / workers / routes 编译错。下游 task 修。

---

## Task 4 — parse_release_list_payload（核心逻辑）

**Files:** `src/services/updater.rs:713` 附近（紧邻原 `parse_release_payload`）

- [ ] Step 1 · 测试

```rust
#[test]
fn parse_list_picks_max_semver_per_channel() {
    let arch = current_arch_token().unwrap_or("x86_64");
    let mk = |tag: &str, pre: bool| serde_json::json!({
        "tag_name": tag,
        "html_url": format!("https://example.com/{tag}"),
        "body": format!("notes for {tag}"),
        "published_at": "2026-05-20T00:00:00Z",
        "prerelease": pre,
        "assets": [
            { "name": format!("wordforge-linux-{arch}.tar.gz"), "browser_download_url": format!("https://example.com/{tag}.tar.gz"), "size": 1000 },
            { "name": format!("wordforge-linux-{arch}.tar.gz.sha256"), "browser_download_url": format!("https://example.com/{tag}.sha256"), "size": 64 },
        ],
    });
    let body = serde_json::Value::Array(vec![
        mk("v0.6.0-beta.3", true),
        mk("v0.6.0-beta.2", true),
        mk("v0.5.6", false),
        mk("v0.5.5", false),
    ]);
    let parsed = parse_release_list_payload(&body);
    assert_eq!(parsed.stable.as_ref().unwrap().tag, "v0.5.6");
    assert_eq!(parsed.beta.as_ref().unwrap().tag, "v0.6.0-beta.3");
}

#[test]
fn parse_list_beta_is_superset_of_stable() {
    let arch = current_arch_token().unwrap_or("x86_64");
    let body = serde_json::json!([{
        "tag_name": "v1.0.0",
        "html_url": "x",
        "body": "",
        "published_at": "2026-05-20T00:00:00Z",
        "prerelease": false,
        "assets": [
            { "name": format!("wordforge-linux-{arch}.tar.gz"), "browser_download_url": "u", "size": 1 },
            { "name": format!("wordforge-linux-{arch}.tar.gz.sha256"), "browser_download_url": "u", "size": 1 },
        ],
    }]);
    let parsed = parse_release_list_payload(&body);
    // 只有 stable release 时 beta_latest 也等于 stable_latest
    assert_eq!(parsed.stable.as_ref().unwrap().tag, "v1.0.0");
    assert_eq!(parsed.beta.as_ref().unwrap().tag, "v1.0.0");
}

#[test]
fn parse_list_handles_empty() {
    let body = serde_json::Value::Array(vec![]);
    let parsed = parse_release_list_payload(&body);
    assert!(parsed.stable.is_none() && parsed.beta.is_none());
}

#[test]
fn parse_list_skips_missing_tag_name() {
    let body = serde_json::json!([
        { "html_url": "x", "body": "", "prerelease": false, "assets": [] },
    ]);
    let parsed = parse_release_list_payload(&body);
    assert!(parsed.stable.is_none() && parsed.beta.is_none());
}
```

- [ ] Step 2 · 实现

```rust
struct ParsedReleaseList {
    stable: Option<CachedRelease>,
    beta: Option<CachedRelease>,
}

fn parse_release_list_payload(body: &serde_json::Value) -> ParsedReleaseList {
    let items = match body.as_array() {
        Some(a) => a,
        None => {
            // 兼容：单 object 也走老逻辑，包成单项
            return ParsedReleaseList {
                stable: parse_release_payload(body),
                beta: parse_release_payload(body),
            };
        }
    };
    let mut stable_best: Option<(semver::Version, CachedRelease)> = None;
    let mut beta_best: Option<(semver::Version, CachedRelease)> = None;
    for item in items {
        let Some(parsed) = parse_release_payload(item) else { continue };
        let ver = match semver::Version::parse(parsed.tag.trim_start_matches('v')) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let is_prerelease = item.get("prerelease").and_then(|v| v.as_bool()).unwrap_or(false);
        // beta_latest = max semver overall（含 stable）
        match &beta_best {
            None => beta_best = Some((ver.clone(), parsed.clone())),
            Some((cur, _)) if &ver > cur => beta_best = Some((ver.clone(), parsed.clone())),
            _ => {}
        }
        // stable_latest = max semver where prerelease=false
        if !is_prerelease {
            match &stable_best {
                None => stable_best = Some((ver, parsed)),
                Some((cur, _)) if &ver > cur => stable_best = Some((ver, parsed)),
                _ => {}
            }
        }
    }
    ParsedReleaseList {
        stable: stable_best.map(|(_, r)| r),
        beta: beta_best.map(|(_, r)| r),
    }
}
```

注：`parse_release_payload` 是原有函数（line 713），保留不动。`CachedRelease` 加 `#[derive(Clone)]`（如未有）。

- [ ] Step 3 · 跑测验证：`cargo test --lib services::updater::tests 2>&1 | tail -20`

---

## Task 5 — check_inner + status_from 改造

**Files:** `src/services/updater.rs`：`check_inner`（line 247-339 范围内 parse 部分）+ `status_from`（line 683-710）

- [ ] Step 1 · check_inner 改写 parse 部分

```rust
// 旧：let parsed = parse_release_payload(&body); cache.latest = parsed;
// 新：
let parsed = parse_release_list_payload(&body);
cache.stable = parsed.stable;
cache.beta = parsed.beta;
```

且 ETag 触发逻辑改为「stable 或 beta 任一缓存非空时才带 If-None-Match」：

```rust
let etag_now = {
    let cache = self.cache.read().await;
    if cache.stable.is_some() || cache.beta.is_some() {
        cache.etag.clone()
    } else {
        None
    }
};
```

- [ ] Step 2 · status_from 改写

```rust
fn status_from(&self, cache: &UpdaterCache) -> UpdateStatus {
    let to_channel = |opt: &Option<CachedRelease>| -> Option<ChannelStatus> {
        opt.as_ref().map(|r| {
            let has_update = is_strictly_newer(&r.tag, &self.current_tag);
            ChannelStatus {
                latest_version: r.tag.clone(),
                latest_published_at: r.published_at,
                release_notes: r.body.clone(),
                release_url: r.html_url.clone(),
                has_update,
                can_apply: has_update && !r.tarball_url.is_empty() && !r.sha256_url.is_empty(),
            }
        })
    };
    UpdateStatus {
        current_version: self.current_tag.clone(),
        stable: to_channel(&cache.stable),
        beta: to_channel(&cache.beta),
        last_checked_at: cache.last_checked_at,
        auto_check_enabled: self.auto_check_enabled,
        allow_downgrade: self.allow_downgrade,
    }
}
```

- [ ] Step 3 · `cargo build` 通过；剩余 build 错误是下游 routes / workers

---

## Task 6 — Updater::apply 加 channel 参数

**Files:** `src/services/updater.rs` Updater::apply

- [ ] Step 1 · 改签名 + 增加 target 在指定 channel cache 中的校验：

```rust
pub async fn apply(
    &self,
    channel: Channel,
    target_version: &str,
    backup_db: impl FnOnce(&Path) -> Result<(), UpdaterError>,
    progress: ProgressSink,
) -> Result<(), UpdaterError> {
    let latest = {
        let cache = self.cache.read().await;
        match channel {
            Channel::Stable => cache.stable.clone(),
            Channel::Beta => cache.beta.clone(),
        }
    };
    let latest = latest.ok_or(UpdaterError::NoCachedRelease)?;
    if latest.tag != target_version {
        return Err(UpdaterError::TargetMismatch {
            requested: target_version.to_string(),
            actual: latest.tag.clone(),
        });
    }
    if !self.allow_downgrade && !is_strictly_newer(&latest.tag, &self.current_tag) {
        return Err(UpdaterError::DowngradeBlocked {
            from: self.current_tag.clone(),
            to: latest.tag.clone(),
        });
    }
    // ...原 apply 流程不变，使用 latest 走 download/verify/swap
}
```

- [ ] Step 2 · 加 UpdaterError 变体

```rust
// 在 pub enum UpdaterError {...} 内追加：
#[error("升级目标 {requested} 与缓存中 {actual} 不一致；请重新检查")]
TargetMismatch { requested: String, actual: String },
#[error("拒绝降级：当前 {from} 不低于目标 {to}")]
DowngradeBlocked { from: String, to: String },
```

- [ ] Step 3 · 跑测 + 加 apply 测：

```rust
#[tokio::test]
async fn apply_rejects_target_mismatch() {
    let updater = test_updater_with_cache_beta("v0.6.0-beta.3").await;
    let err = updater.apply(Channel::Beta, "v0.6.0-beta.99", noop_backup, noop_sink()).await.unwrap_err();
    assert!(matches!(err, UpdaterError::TargetMismatch { .. }));
}
```

（`test_updater_with_cache_beta` 是测试 helper，新加。）

---

## Task 7 — SseEvent::ReleaseAvailable + workers + force_check broadcast

**Files:** `src/state.rs:48-51`, `src/workers/update_checker.rs`, `src/routes/admin/updates.rs:61-78`

- [ ] Step 1 · state.rs 改 enum 变体

```rust
#[serde(rename = "release_available")]
ReleaseAvailable {
    #[serde(rename = "latestTag")]
    latest_tag: String,
    channel: crate::services::updater::Channel,
},
```

- [ ] Step 2 · workers/update_checker.rs 改成双通道分别 prev/new 对比

```rust
pub async fn run(updater: Arc<Updater>, state: AppState) {
    use crate::services::updater::Channel;
    let prev = updater.snapshot().await;
    let status = match updater.check_latest().await {
        Ok(s) => s,
        Err(e) => { tracing::warn!("update_checker: {e}"); return; }
    };
    for (channel, prev_ch, new_ch) in [
        (Channel::Stable, &prev.stable, &status.stable),
        (Channel::Beta, &prev.beta, &status.beta),
    ] {
        let Some(new) = new_ch else { continue };
        if !new.has_update { continue; }
        if prev_ch.as_ref().map(|p| p.latest_version.as_str()) != Some(new.latest_version.as_str()) {
            tracing::info!(channel = channel.as_str(), latest = %new.latest_version, "update_checker: announcing release");
            state.broadcast_to_all_sse(SseEvent::ReleaseAvailable {
                latest_tag: new.latest_version.clone(),
                channel,
            });
        }
    }
}
```

- [ ] Step 3 · routes/admin/updates.rs::force_check 双通道广播

```rust
async fn force_check(...) -> Result<...> {
    let updater = require_updater(&state).await?;
    let prev = updater.snapshot().await;
    let status = updater.force_check_latest().await.map_err(map_err)?;
    use crate::services::updater::Channel;
    for (channel, prev_ch, new_ch) in [
        (Channel::Stable, &prev.stable, &status.stable),
        (Channel::Beta, &prev.beta, &status.beta),
    ] {
        let Some(new) = new_ch else { continue };
        if !new.has_update { continue; }
        if prev_ch.as_ref().map(|p| p.latest_version.as_str()) != Some(new.latest_version.as_str()) {
            state.broadcast_to_all_sse(SseEvent::ReleaseAvailable {
                latest_tag: new.latest_version.clone(),
                channel,
            });
        }
    }
    Ok(ok(status))
}
```

- [ ] Step 4 · `cargo build` 通过

---

## Task 8 — ApplyRequest 加 channel + apply handler 转发

**Files:** `src/routes/admin/updates.rs:80-189`

- [ ] Step 1 · ApplyRequest

```rust
use crate::services::updater::Channel;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyRequest {
    channel: Channel,
    target_version: String,
    confirm_current_version: String,
}
```

- [ ] Step 2 · apply handler 改：把 `target` 改成调用 `bg_updater.apply(req.channel, &target, backup_cb, sink)`，其他不动。

- [ ] Step 3 · `cargo build`

---

## Task 9 — config.rs api_url 默认值

**Files:** `src/config.rs:356-358`

- [ ] Step 1 · 改默认

```rust
api_url: env_or(
    "UPDATE_CHECK_API_URL",
    "https://api.github.com/repos/Heartcoolman/wordforge/releases?per_page=10",
),
```

- [ ] Step 2 · `cargo build` 通过

---

## Task 10 — 后端集成测试 tests/updates_channel_http.rs

**Files:** Create `tests/updates_channel_http.rs`

- [ ] Step 1 · 写测试（mock GitHub list 端点用 wiremock 或本地 fixture；如果已有 mock pattern 复用）

```rust
mod common;

use axum::http::{Method, StatusCode};
use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn it_status_returns_double_channel() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let response = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/status",
        None,
        &[("authorization", auth_header(&admin_token))],
    ).await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["stable"].is_object() || body["data"]["stable"].is_null());
    assert!(body["data"]["beta"].is_object() || body["data"]["beta"].is_null());
    assert!(body["data"]["currentVersion"].is_string());
}

#[tokio::test]
async fn it_apply_requires_channel_field() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let response = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({
            "targetVersion": "v0.0.0",
            "confirmCurrentVersion": "v0.0.0",
        })),
        &[("authorization", auth_header(&admin_token))],
    ).await;
    let (status, _, _) = response_json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
```

- [ ] Step 2 · `cargo test --test updates_channel_http 2>&1 | tail`

---

## Task 11 — 后端阶段 commit

- [ ] Step 1 · `cargo test 2>&1 | tail -30` 全绿
- [ ] Step 2 · 

```bash
git add -A src/services/updater.rs src/routes/admin/updates.rs src/state.rs \
    src/workers/update_checker.rs src/config.rs tests/updates_channel_http.rs
git commit -m "feat(admin/updates): 后端双通道 Stable/Beta 改造

- src/services/updater.rs: Channel enum / ParsedReleaseList / parse_release_list_payload
  / UpdaterCache 拆 stable+beta / ChannelStatus 新 / UpdateStatus 替换契约 /
  apply(channel, target, ...) 加 channel + TargetMismatch + DowngradeBlocked
- src/routes/admin/updates.rs: ApplyRequest 加 channel；force_check 双通道广播
- src/state.rs: SseEvent::ReleaseAvailable payload 加 channel
- src/workers/update_checker.rs: 双通道分别 prev/new 对比 broadcast
- src/config.rs: UPDATE_CHECK_API_URL default 改 /releases?per_page=10
- tests/updates_channel_http.rs: 集成测双通道路径"
```

---

## Task 12 — 前端 types/admin.ts ChannelStatus + UpdateStatus

**Files:** `frontend/src/types/admin.ts:90-103`

- [ ] Step 1 · 替换 AdminUpdateStatus

```typescript
export interface ChannelStatus {
  latestVersion: string;
  latestPublishedAt: string | null;
  releaseNotes: string;
  releaseUrl: string;
  hasUpdate: boolean;
  canApply: boolean;
}

export interface AdminUpdateStatus {
  currentVersion: string;
  stable: ChannelStatus | null;
  beta: ChannelStatus | null;
  lastCheckedAt: string | null;
  autoCheckEnabled: boolean;
  allowDowngrade: boolean;
  applyTask?: ApplyTaskStatus;
}
```

- [ ] Step 2 · tsc check（vitest 跑会报）

---

## Task 13 — frontend api/admin.ts updatesApply 加 channel

**Files:** `frontend/src/api/admin.ts:118-128`

- [ ] Step 1 · 改 updatesApply 签名

```typescript
updatesApply: (channel: 'stable' | 'beta', targetVersion: string, confirmCurrentVersion: string) =>
  api.post<ApplyAccepted>(
    '/api/admin/updates/apply',
    { channel, targetVersion, confirmCurrentVersion },
    { useAdminToken: true },
  ),
```

---

## Task 14 — frontend api/client.ts SSE onReleaseAvailable 加 channel

**Files:** `frontend/src/api/client.ts:228-229, 321-322`

- [ ] Step 1 · callbacks 类型

```typescript
onReleaseAvailable?: (payload: { latestTag: string; channel: 'stable' | 'beta' }) => void;
```

- [ ] Step 2 · 事件分发 case

```typescript
} else if (eventType === 'release_available' && typeof data.latestTag === 'string') {
  const channel = (data.channel === 'beta' ? 'beta' : 'stable') as 'stable' | 'beta';
  callbacks.onReleaseAvailable?.({ latestTag: data.latestTag, channel });
}
```

（旧 payload 无 channel 时 fallback stable，保持渐进迁移。）

---

## Task 15 — Collapsible 组件

**Files:** Create `frontend/src/components/ui/Collapsible.tsx`, `frontend/tests/components/ui/Collapsible.test.tsx`

- [ ] Step 1 · 测试

```typescript
import { describe, it, expect } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { Collapsible } from '@/components/ui/Collapsible';

describe('Collapsible', () => {
  it('defaults to collapsed and toggles on click', () => {
    const { getByRole, queryByText } = render(() => (
      <Collapsible title="Beta 通道">
        <div>内容</div>
      </Collapsible>
    ));
    expect(queryByText('内容')).toBeNull();
    const trigger = getByRole('button', { name: /Beta 通道/ });
    fireEvent.click(trigger);
    expect(queryByText('内容')).not.toBeNull();
  });

  it('renders badge when provided', () => {
    const { queryByText } = render(() => (
      <Collapsible title="Beta 通道" badge="v0.6.0-beta.3">
        <div>内容</div>
      </Collapsible>
    ));
    expect(queryByText('v0.6.0-beta.3')).not.toBeNull();
  });

  it('starts expanded with defaultOpen', () => {
    const { queryByText } = render(() => (
      <Collapsible title="X" defaultOpen>
        <div>内容</div>
      </Collapsible>
    ));
    expect(queryByText('内容')).not.toBeNull();
  });
});
```

- [ ] Step 2 · 实现 `Collapsible.tsx`

```typescript
import { createSignal, Show, type JSX } from 'solid-js';

interface Props {
  title: string;
  badge?: string | null;
  defaultOpen?: boolean;
  onToggle?: (open: boolean) => void;
  children: JSX.Element;
}

export function Collapsible(props: Props) {
  const [open, setOpen] = createSignal(props.defaultOpen ?? false);
  const toggle = () => {
    const next = !open();
    setOpen(next);
    props.onToggle?.(next);
  };
  return (
    <div class="border border-border-hairline rounded-lg overflow-hidden">
      <button
        type="button"
        class="w-full flex items-center justify-between px-4 py-3 hover:bg-surface-secondary/60 transition-colors"
        aria-expanded={open()}
        onClick={toggle}
      >
        <span class="flex items-center gap-2 font-medium text-content">
          <span aria-hidden class="transition-transform" style={{ transform: open() ? 'rotate(90deg)' : 'none' }}>▸</span>
          {props.title}
        </span>
        <Show when={props.badge}>
          <span class="text-xs px-2 py-0.5 rounded-full bg-accent text-white font-mono">{props.badge}</span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="p-4 border-t border-border-hairline">{props.children}</div>
      </Show>
    </div>
  );
}
```

- [ ] Step 3 · `cd frontend && npx vitest run tests/components/ui/Collapsible.test.tsx`

---

## Task 16 — UpdateChannelCard 组件

**Files:** Create `frontend/src/components/admin/UpdateChannelCard.tsx`

- [ ] Step 1 · 实现（薄包装，把 ChannelStatus 渲染为卡片）

```typescript
import { Show, type JSX } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import type { ChannelStatus } from '@/types/admin';

interface Props {
  channel: 'stable' | 'beta';
  status: ChannelStatus | null;
  currentVersion: string;
  applying: boolean;
  onApply: () => void;
}

const LABEL: Record<'stable' | 'beta', string> = { stable: '稳定通道', beta: 'Beta 通道' };

export function UpdateChannelCard(props: Props) {
  const disabled = () => !props.status?.hasUpdate || !props.status?.canApply || props.applying;
  return (
    <Card>
      <div class="flex items-center justify-between mb-3">
        <div>
          <p class="text-sm text-content-secondary">{LABEL[props.channel]} · 远端最新</p>
          <p class="text-2xl font-semibold text-content">
            {props.status?.latestVersion ?? '尚未检查'}
          </p>
        </div>
        <Button
          disabled={disabled()}
          loading={props.applying}
          onClick={props.onApply}
        >
          {props.status?.hasUpdate ? `升级到 ${props.status.latestVersion}` : '已是最新'}
        </Button>
      </div>
      <Show when={!props.status?.canApply && props.status?.hasUpdate}>
        <p class="text-sm text-warning mb-2">远端发布了新版本，但未找到匹配当前架构的产物。</p>
      </Show>
      <Show when={props.status?.releaseNotes}>
        <details class="mt-3">
          <summary class="text-sm text-accent cursor-pointer">Release Notes</summary>
          <pre class="mt-2 whitespace-pre-wrap text-xs text-content-secondary font-mono leading-relaxed max-h-64 overflow-y-auto">
            {props.status!.releaseNotes}
          </pre>
          <Show when={props.status?.releaseUrl}>
            <a href={props.status!.releaseUrl} target="_blank" rel="noopener" class="text-xs text-accent hover:underline">在 GitHub 打开 ↗</a>
          </Show>
        </details>
      </Show>
    </Card>
  );
}
```

---

## Task 17 — UpdatesPage 改造

**Files:** `frontend/src/pages/admin/UpdatesPage.tsx`（314 行整体替换）

- [ ] Step 1 · 改 confirmApply 接收 channel；改 UI 结构

主要变更：
- `s().latestVersion / hasUpdate / canApply / releaseNotes / releaseUrl` 全部嵌套到 `s().stable / s().beta` 之下
- 把当前主区域改成「当前版本 StatCard」+ Stable `<UpdateChannelCard>` + `<Collapsible title="Beta 通道" badge={betaBadge()}>` 内嵌 Beta `<UpdateChannelCard>`
- `confirmApply` 接收 channel 参数，调用 `adminApi.updatesApply(channel, target.latestVersion, s().currentVersion)`
- `betaBadge()` = `() => s()?.beta?.hasUpdate ? s().beta!.latestVersion : null`

完整改写见 Task 17 execution 阶段。skeleton：

```typescript
// 状态
const [pendingChannel, setPendingChannel] = createSignal<'stable' | 'beta' | null>(null);

function openConfirm(channel: 'stable' | 'beta') {
  const ch = channel === 'stable' ? status()?.stable : status()?.beta;
  if (!ch || !ch.canApply) return;
  setPendingChannel(channel);
  setConfirmOpen(true);
}

async function confirmApply() {
  const ch = pendingChannel();
  const target = ch === 'stable' ? status()?.stable : status()?.beta;
  if (!ch || !target) return;
  // 调 adminApi.updatesApply(ch, target.latestVersion, status()!.currentVersion)
}

// JSX
<header>当前版本 + 立即检查按钮</header>
<UpdateChannelCard channel="stable" status={s().stable} currentVersion={s().currentVersion} applying={applying() && pendingChannel()==='stable'} onApply={() => openConfirm('stable')} />
<Collapsible title="Beta 通道" badge={s().beta?.hasUpdate ? s().beta!.latestVersion : null}>
  <UpdateChannelCard channel="beta" status={s().beta} currentVersion={s().currentVersion} applying={applying() && pendingChannel()==='beta'} onApply={() => openConfirm('beta')} />
</Collapsible>
<Card>升级进度条（applying || terminal 时显示）</Card>
<Card>安全提示</Card>
<Modal>二次确认</Modal>
```

- [ ] Step 2 · `cd frontend && npx vitest run tests/pages/admin/UpdatesPage 2>&1 | tail` — 现有测试会大量崩，task 18 修

---

## Task 18 — 前端测试更新

**Files:** `frontend/tests/api/admin.test.ts`, `frontend/tests/pages/admin/UpdatesPage.test.tsx`, `frontend/tests/pages/admin/UpdatesPage.features.test.tsx`

- [ ] Step 1 · `admin.test.ts` 改：updatesStatus mock 返回双通道结构 + 加 updatesApply channel 用例
- [ ] Step 2 · `UpdatesPage.test.tsx` 改：mock status 返双通道 + 断言渲染 Stable 卡 + 折叠条 + 默认 Beta 不可见 + 点击折叠条后 Beta 可见
- [ ] Step 3 · `UpdatesPage.features.test.tsx` 改：badge 展示规则 / 跨通道按钮 / SSE onReleaseAvailable channel
- [ ] Step 4 · `cd frontend && npm test 2>&1 | tail -30` 全绿

---

## Task 19 — 前端阶段 commit

- [ ] Step 1 · 

```bash
git add frontend/src frontend/tests
git commit -m "feat(admin/updates): 前端双通道 UI + 折叠区 + Collapsible/UpdateChannelCard 组件

- types/admin.ts: ChannelStatus 接口 + AdminUpdateStatus 改 stable/beta 嵌套
- api/admin.ts: updatesApply 加 channel 参数
- api/client.ts: SseCallbacks.onReleaseAvailable payload 加 channel（旧 payload fallback stable）
- components/ui/Collapsible.tsx: 通用折叠条 + badge + ARIA + 测试
- components/admin/UpdateChannelCard.tsx: 单通道卡片复用组件
- pages/admin/UpdatesPage.tsx: Stable 主卡 + Beta 折叠区 + 双 confirmApply
- tests: vitest 全套更新 + 新 Collapsible 测试

vitest 全量通过。"
```

---

## Task 20 — release.yml prerelease 规则

**Files:** `.github/workflows/release.yml` (最后 release job 的 softprops/action-gh-release@v2)

- [ ] Step 1 · 改

```yaml
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            dist/*.tar.gz
            dist/*.tar.gz.sha256
          generate_release_notes: true
          prerelease: ${{ contains(github.ref_name, '-') }}
```

- [ ] Step 2 · commit

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): 显式 prerelease 规则锁定（tag 含 '-' → prerelease=true）

v0.6.0-beta.1 / .2 被 softprops/action-gh-release@v2 自动判定结果不一致：
.1 → prerelease=true，.2 → false。改为显式 expression：
  prerelease: \${{ contains(github.ref_name, '-') }}
所有 -beta.N / -rc.N / -alpha.N 永远 prerelease=true，stable vX.Y.Z 永远 false。"
```

---

## Task 21 — bump 版本

**Files:** `Cargo.toml`, `Cargo.lock`

- [ ] Step 1 · `Cargo.toml` 改 `version = "0.6.0-beta.2"` → `version = "0.6.0-beta.3"`
- [ ] Step 2 · `cargo update -p learning-backend` 同步 lockfile
- [ ] Step 3 · 

```bash
git add Cargo.toml Cargo.lock
git commit -m "release: bump version to v0.6.0-beta.3"
```

---

## Task 22 — push 分支 + PR + 等 CI 全绿 + merge

- [ ] Step 1 · `git push -u origin release/v0.6.0-beta.3-hotfix`
- [ ] Step 2 · `gh pr create --base main --title "release: v0.6.0-beta.3 — admin/updates 双通道 + release.yml prerelease 规则"`
- [ ] Step 3 · `gh pr checks <PR#> --watch`
- [ ] Step 4 · `gh pr merge <PR#> --squash --delete-branch`
- [ ] Step 5 · `git checkout main && git pull --ff-only origin main`

---

## Task 23 — tag v0.6.0-beta.3 + push + release workflow

- [ ] Step 1 · 

```bash
git tag -a v0.6.0-beta.3 -m "v0.6.0-beta.3: admin/updates 双通道 + release.yml prerelease 规则

修复 v0.6.0-beta.2 错标 Latest 的问题；release.yml 显式 prerelease
expression（这次 v0.6.0-beta.3 应自动标 prerelease=true）。
admin 后台同时显示 Stable 与 Beta 通道。"
git push origin v0.6.0-beta.3
```

- [ ] Step 2 · `gh run watch <release-run-id> --exit-status`
- [ ] Step 3 · `gh release view v0.6.0-beta.3 --json isPrerelease` —— 应该 `true`

---

## Task 24 — 生产部署 + .env 复位 + 验证

- [ ] Step 1 · SSH 改 .env

```bash
ssh root@8.135.57.148 'sed -i "s|^UPDATE_CHECK_API_URL=.*$|UPDATE_CHECK_API_URL=https://api.github.com/repos/Heartcoolman/wordforge/releases?per_page=10|" /opt/wordforge/.env && rm -f /opt/wordforge/.update_etag'
```

- [ ] Step 2 · SSH 下 release tar 经 ghproxy.net + swap + restart

```bash
ssh root@8.135.57.148 '
URL=https://ghproxy.net/https://github.com/Heartcoolman/wordforge/releases/download/v0.6.0-beta.3/wordforge-linux-x86_64.tar.gz
cd /tmp
curl -fsSL -o wf-v0.6.0-beta.3.tar.gz "$URL"
curl -fsSL -o wf-v0.6.0-beta.3.tar.gz.sha256 "$URL.sha256"
cat wf-v0.6.0-beta.3.tar.gz.sha256; sha256sum wf-v0.6.0-beta.3.tar.gz | awk "{print \$1}"
rm -rf wordforge-linux-x86_64 && tar -xzf wf-v0.6.0-beta.3.tar.gz
cp /opt/wordforge/wordforge /opt/wordforge/wordforge.bak.v0.6.0-beta.2
[ -d /opt/wordforge/static.bak.v0.6.0-beta.2 ] && rm -rf /opt/wordforge/static.bak.v0.6.0-beta.2
mv /opt/wordforge/static /opt/wordforge/static.bak.v0.6.0-beta.2
systemctl stop wordforge
cp wordforge-linux-x86_64/wordforge /opt/wordforge/wordforge
cp -r wordforge-linux-x86_64/static /opt/wordforge/static
chown -R wordforge:wordforge /opt/wordforge/wordforge /opt/wordforge/static
chmod +x /opt/wordforge/wordforge
systemctl start wordforge
sleep 4
systemctl is-active wordforge
curl -s http://127.0.0.1:3000/api/status
'
```

- [ ] Step 3 · 验证：访问 `gh release view v0.6.0-beta.3 --json isPrerelease`、`curl http://127.0.0.1:3000/api/status` 显示 `v0.6.0-beta.3`、提示用户浏览器关 tab 重开后看 admin/updates 双通道。

---

## Task 25 — 主分支 cleanup + stash pop P3

- [ ] Step 1 · `git checkout main && git pull --ff-only`
- [ ] Step 2 · `git stash pop stash@{0}` 恢复 4 处 P3 修改
- [ ] Step 3 · `git status --short` 验证

---

## Self-Review Checklist

- [ ] 每个 spec section（Architecture / Components 5.1-5.5 / Data Flow / Error Handling / Testing）都有对应 task
- [ ] 没有 "TBD" / "TODO" 占位（Task 17 步骤里写了「完整改写见 Task 17 execution 阶段」—— execution 时按 skeleton + spec 展开即可）
- [ ] 类型签名一致：ChannelStatus 在 Task 3 / 12 / 15 / 16 / 17 一致；apply(channel, target, ...) 在 Task 6 / 8 / 13 一致
- [ ] 类型字段命名一致：latestVersion / hasUpdate / canApply / releaseNotes / releaseUrl / latestPublishedAt 全 camelCase（后端 serde rename）
- [ ] is_strictly_newer 复用，不再造一个
