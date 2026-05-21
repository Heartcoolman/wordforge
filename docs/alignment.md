# WordForge 客户端 × 后端契约对齐审计报告（第四轮）

**审计时间**：2026-05-21
**后端仓库**：`/Users/liji/english/wordforge`（branch `feat/v1-m0`）
**iOS 客户端**：`/Users/liji/WordForge-App`（branch `main`）
**审计范围**：v0.6.0-beta.1–beta.4 期间变更（双通道、ErrorBoundary 修、release notes md 渲染、WordState wire lowercase、favorites paginated）
**判定方式**：交叉验证 0 P0 + 0 P1、字段级一致、集成测试通过

---

## 1. 第三轮遗留（继承自 2026-05-19 报告）

第三轮终裁的 3 P0 + 9 P1 全部落地修复，已在 main 分支。本轮基于此基线，审计 beta.1–beta.4 新增变更。

---

## 2. 本轮审计范围（v0.6.0-beta.1–beta.4 变更）

| 变更 | commit/PR | 范围 |
|---|---|---|
| Admin 一键升级双通道（stable + beta） | beta.3 (#57) | 后端 updater + 前端 UpdatesPage |
| FeedbackPage `items` → `data` ErrorBoundary 修 | beta.1 hotfix | 前端 api/admin.ts |
| Release notes markdown 渲染 | beta.3 (#57) | 前端 UpdatesPage |
| WordState wire 序列化 lowercase（P3#7） | `d0325f8` | 后端 WordState enum + 前端类型 |
| favorites `list_favorites` 改 paginated()（P3#5） | `fb93944` | 后端 word_favorites.rs + 文档 |

---

## 3. 第四轮终裁清单

### 3.1 发现 P1（1 项，已修）

| # | 一句话 | 范围 | 修复方 | 状态 |
|---|---|---|---|---|
| P1-W1 | `frontend/src/types/wordState.ts::WordStateType` 仍为大写 `'NEW'\|'LEARNING'\|...`，而后端 P3#7 wire 已改 lowercase `"new"\|"learning"\|...` → iOS BatchUpdateRequest 发 uppercase、前端比对 state 字段全 mismatch | wordState.ts | 前端 | **已修**（本轮） |

### 3.2 发现 P2（文档类，已修）

| # | 一句话 | 文件 | 状态 |
|---|---|---|---|
| P2-D1 | `docs/api-spec.md §10` 单词学习状态枚举说明仍为 uppercase，与 P3#7 后实际 wire 不符 | docs/api-spec.md | **已修** |
| P2-D2 | `docs/api-endpoints.md §7` `WordLearningState` 结构示例 `state` 值仍为 `"MASTERED"` | docs/api-endpoints.md | **已修** |
| P2-D3 | `docs/api-endpoints.md §7` `batch-update` 请求体示例 `state` 值仍为 `"REVIEWING"` | docs/api-endpoints.md | **已修** |
| P2-D4 | `docs/api-endpoints.md §3.2/§13` `masteryLevel` 说明称"与 `WordLearningState.state` 同一份枚举"，但实际是两个不同枚举（SCREAMING_SNAKE_CASE vs lowercase） | docs/api-endpoints.md | **已修** |
| P2-D5 | `docs/api-endpoints.md §19` `GET /api/word-favorites` 响应示例仍为扁平数组，P3#5 后实际返回 paginated 结构 | docs/api-endpoints.md | **已修** |

### 3.3 ALIGNED（已对齐，无需操作）

| 变更 | 后端 | 前端 | 判定 |
|---|---|---|---|
| 双通道 `UpdateStatus{stable,beta}` | `src/services/updater.rs::UpdateStatus` + `Channel` enum（wire lowercase） | `types/admin.ts::ChannelStatus` + `AdminUpdateStatus` | ALIGNED |
| `updatesApply` 必传 `channel` | `routes/admin/updates.rs::ApplyRequest.channel` | `api/admin.ts::updatesApply(channel, ...)` | ALIGNED |
| SSE `ReleaseAvailable` 加 `channel` | `state.rs::SseEvent::ReleaseAvailable{latest_tag, channel}` | `api/client.ts::SseCallbacks.onReleaseAvailable` payload | ALIGNED |
| `FeedbackPage` `items` → `data` | `response::paginated()` 返回 `data` 字段 | `api/admin.ts::listFeedback` 类型签名 `data: FeedbackItem[]` | ALIGNED |
| Release notes 内嵌各通道卡片 | `ChannelStatus.release_notes` 字段 | `UpdatesPage` 内嵌渲染 | ALIGNED |
| favorites `paginated()` 返回 | `routes/word_favorites.rs::list_favorites` 改 `paginated()` | 无前端自有 favorites API（iOS 直调后端） | ALIGNED |
| `WordState` wire lowercase | `#[serde(rename_all = "lowercase")]` | `wordState.ts::WordStateType`（本轮已修） | ALIGNED（修后） |
| `WordMastery.masteryLevel` SCREAMING_SNAKE_CASE | `MasteryLevel` `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` | `amas.ts::WordMastery.masteryLevel: 'NEW'\|...` | ALIGNED（不同枚举，不受 P3#7 影响） |

---

## 4. 落地修复清单（本轮）

### 4.1 前端

| 文件 | 改动 |
|---|---|
| `frontend/src/types/wordState.ts:1` | `WordStateType` 由 uppercase 改为 lowercase（`'new'\|'learning'\|'reviewing'\|'mastered'\|'forgotten'`） |

### 4.2 文档

| 文件 | 改动 |
|---|---|
| `docs/api-spec.md:274` | §10 单词学习状态枚举改为 lowercase；补注与 `masteryLevel` 的区分 |
| `docs/api-endpoints.md:1235` | §7 `WordLearningState` 结构示例 `state` 值改为 `"mastered"` |
| `docs/api-endpoints.md:1334` | §7 `batch-update` 请求体示例 `state` 值改为 `"reviewing"`；加枚举说明 |
| `docs/api-endpoints.md:832,2281` | §3.2/§13 `masteryLevel` 说明改为"不同枚举，不可混用" |
| `docs/api-endpoints.md:2662` | §19 `GET /api/word-favorites` 响应示例改为 paginated 结构 |

---

## 5. 验证命令

### 5.1 后端
```bash
cd /Users/liji/english/wordforge
cargo test --no-fail-fast
```

### 5.2 前端
```bash
cd /Users/liji/english/wordforge/frontend
pnpm tsc --noEmit   # 类型检查（含 WordStateType lowercase 影响面）
pnpm test           # vitest 单元测试
```

---

## 6. 100% 对齐结论（第四轮）

| 维度 | 状态 |
|---|---|
| Admin 双通道 | 0 P0 / 0 P1（UpdateStatus stable/beta + apply channel 全对齐） |
| ErrorBoundary / listFeedback | 0 P0 / 0 P1（items→data hotfix 已落地） |
| WordState wire lowercase | 0 P0 / 0 P1（P1-W1 本轮修复，前端类型已同步） |
| favorites paginated | 0 P0 / 0 P1（后端 paginated()，文档已更新） |
| 文档与 wire 一致性 | 0 P0 / 0 P1（5 处 P2 文档全修） |

**第四轮 cross-validator：0 P0 / 0 P1 / 0 P2 high-impact。** 可进 rc.1。

---

## 7. 未来计划（非本轮）

- AMAS 配置项 PARAM_DICT 与 `tuning_whitelist.rs` 11 维路径列表的 cross-check 单测（P2-1）
- ColdStartPhase 枚举大小写统一（P2-3）：后端 `#[serde(rename_all = "snake_case")]`
- DAU/留存 UI 展示（Admin P1-3 降级 P2）
- iOS 真机回归 `杀进程→重启→自动 refresh` 验证 cookie 持久化（iOS agent 风险点 #2）
- selfRating 在 AMAS half-life 模型中的接入（第三轮仅 DTO + 落库）
- M2-Q3 rc.3 前最终一次 cross-validator（重点：M0-C5 410 客户端处理、M1-G1 导出契约、M1-G3 feedback 扩展字段）
