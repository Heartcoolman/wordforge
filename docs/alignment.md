# WordForge 客户端 × 后端契约对齐审计报告（第三轮）

**审计时间**：2026-05-19
**后端仓库**：`/Users/liji/english/wordforge`（branch `main`）
**iOS 客户端**：`/Users/liji/WordForge-App`（branch `main`）
**审计范围**：iOS REST 契约、遥测通道、Admin 控制台契约、AMAS 决策/版本 schema
**判定方式**：交叉验证 0 P0 + 0 P1、字段级一致、集成测试通过

---

## 1. 审计团队编排

| Agent | 范围 | 产出 |
|---|---|---|
| `audit-rest` | iOS ↔ src/routes/*（非 admin） | 0 P0 / 3 P1 / 5 P2 |
| `audit-telemetry` | iOS + Web frontend ↔ src/routes/telemetry / amas/* | 2 P0 / 5 P1 / 4 P2 |
| `audit-admin` | Web frontend admin/* ↔ src/routes/admin/* | 2 P0 / 3 P1 / 2 P2 |
| `audit-amas` | AMAS 三端 schema | 2 P0 / 5 P1 / 3 P2 |
| `cross-validator` | 独立复核 6 P0 + 16 P1 | 终裁：3 CONFIRMED P0 / 9 CONFIRMED P1 / 5 降级 P2 / 2 false positive |

---

## 2. 终裁清单（cross-validator）

### 2.1 CONFIRMED P0（必须修）

| # | 一句话 | 原 audit | 修复方 |
|---|---|---|---|
| P0-1 | `MetricsSnapshot` 后端 snake_case 序列化 vs 前端期望 camelCase → metrics 表格全空 | Admin P0-1 | 后端 |
| P0-2 | `/admin/amas/monitoring` 响应缺 `{timestamp, eventType, data}` 包装 → 监控页空 | Admin P0-2 | 后端 |
| P0-3 | AMAS `memoryModel.w` defaults 三方不一致：前端 schema (FSRS-5 公版) ≠ 后端 default_w (老调优值) ≠ amas_config.toml (2026-05-15 tuned) → "重置默认"按钮注入错误值 | AMAS P0-1 | 后端 default_w 对齐到 FSRS-5 公版 |

### 2.2 CONFIRMED P1（发版前应修）

| # | 一句话 | 原 audit | 修复方 |
|---|---|---|---|
| P1-1 | iOS `refreshAccessToken` 仅依赖 cookie jar，无 Bearer 兜底 | REST P1-1 | iOS + 后端（暴露 refresh_token JSON 字段） |
| P1-2 | iOS `submitVisualFatigue` 客户端无 [0,100] 范围校验 | REST P1-2 | iOS |
| P1-3 | 后端 `delete_user` 未级联清理 `wb_center_imports` 行 | REST P1-3 | 后端 |
| P1-4 | iOS strict-mode 错误码（MISSING_USER_AGENT 等 6 个）后端未实现 | 遥测 P0-1 (降 P1) | 后端 strict-mode middleware |
| P1-5 | `selfRating` 客户端上报但后端 DTO 不消费 → SRS 自评粒度丢失 | 遥测 P0-3 / REST P2-2 | 后端 DTO + 落库 |
| P1-6 | `types/amas.ts` 的 `AmasConfig` 缺 16+ 子结构 → `as unknown as` 强转 | AMAS P0-2 (降 P1) | 前端 schemars codegen |
| P1-7 | `/admin/feedback` 后端 API 就绪但前端零对接 | Admin P1-1 | 前端 |
| P1-8 | SSE `new_llm_suggestion` 事件前端无 callback | Admin P1-2 | 前端 |
| P1-9 | 4 字段前端类型缺：`expectedForgetCost / lastSessionId / temporalPerformance / confusedWith` | AMAS P1-1~4 | 前端 |

### 2.3 DOWNGRADE_TO_P2（5 项）
- 遥测 P1-2：frontend 不上报 `actionsPerMin/errorCount` → 设计差异（admin 后台天然不产学习指标）
- Admin P1-3：`retentionRate` 前端 UI 未展示 → 信息隐藏，非错位
- AMAS P1-5：SSE `AmasState` 字段子集说明 → 文档项
- 遥测 P0-2：OfflineQueue 401 死循环 → 实为 5 次 retry 后丢，非僵尸
- 遥测 P1-3 / P1-5：buffer 回滚策略 / 时间戳精度 → 设计差异

### 2.4 FALSE_POSITIVE（2 项）
- 遥测 P0-4：Web frontend 不向 AMAS 上报 → 本仓库 frontend 是 admin 后台，学习端已拆出
- 遥测 P1-1 / P1-4：click_targets schema 自陈"已对齐" / rate-limit 是"未确认"非 finding

---

## 3. 落地修复清单

### 3.1 后端（learning-backend）
| 文件 | 改动 |
|---|---|
| `src/amas/metrics.rs:195-200` | `MetricsSnapshot` 加 `#[serde(rename_all = "camelCase")]` |
| `src/routes/admin/amas.rs:656-668` | `get_monitoring_events` 响应包装为 `{timestamp, eventType, data}` |
| `src/amas/config/memory.rs:155-180` | `default_w()` 改为 FSRS-5 公版（与前端 schema default 字面一致） |
| `src/store/operations/users.rs:36` | `USER_SCOPED_TABLES` 加 `wb_center_imports` |
| `src/store/operations/records.rs:46-56, 129-145, 156-170` | `LearningRecord.self_rating: Option<u8>`、INSERT 加列 |
| `src/store/schema.rs:269-272` | `learning_records` 表加 `self_rating INTEGER` 列 |
| `src/store/migrate.rs:19, 348-364` | 新增 `m013_learning_record_self_rating` 迁移 |
| `src/routes/records.rs:83-87, 263-272, 527-536` | `CreateRecordRequest.self_rating` + 2 处构造传入 |
| `src/routes/learning.rs:880, v1.rs:131` | 派生路径 `self_rating: None` 占位 |
| `src/routes/auth.rs:88-93, 213-216, 322-325, 397-401` | `AuthResponse` 暴露 `refresh_token` JSON 字段 |
| `src/middleware/strict_mode.rs`（新） | strict-mode middleware：UA / OS / 版本门控，soft/hard-block 切换 |
| `src/middleware/mod.rs:5` | 注册 strict_mode 模块 |
| `src/routes/telemetry.rs:39-90, 109-122` | payload 层补 MISSING_TIMEZONE / LANGUAGE / DEVICE_FINGERPRINT 校验 |
| `src/routes/mod.rs:32, layer` | strict_mode_middleware 注入 router |
| `src/config.rs:39-65, 230-330` | 新增 `StrictModeConfig` + env 装载（STRICT_MODE_ENABLED / HARD_BLOCK / MIN_CLIENT_VERSION） |
| `tests/strict_mode_http.rs`（新） | 7 个集成测试：disabled / hard-block / soft-block / admin 豁免 / v1 豁免 / 版本门控 |
| `tests/common/app.rs` | 新增 `spawn_test_app_with_strict_mode` helper |
| `src/amas/config/*.rs` + `src/amas/types.rs` | schemars `JsonSchema` derive（21 个子结构 + 类型） |
| `src/routes/admin/amas.rs` `/config/schema` | 新增 `GET /api/admin/amas/config/schema` 端点 |

### 3.2 iOS（WordForge-App, commit `8574421`）
| 文件 | 改动 |
|---|---|
| `APIClient.swift:133-220, 411-486, 510-578` | KeychainTokenStore 拆为通用 read/save + refresh_token 三件套；`performRefreshRequest(bearer:)` 手注入 `Authorization: Bearer <refresh>` |
| `Models.swift:10-17` | `AuthResponse` 加 Optional `refreshToken` |
| `AppState.swift:224-232` | 登录响应后 `api.persistRefreshTokenAfterAuth(response)` |
| `EndpointServices.swift:411-416` | `submitVisualFatigue` clamp `[0, 100]` |
| `Requests.swift:191-197` | `VisualFatigueRequest` 范围 doc comment |
| `OfflineQueue.swift:80-167` | `DrainOutcome` 三态枚举；401/AUTH_EXPIRED 立即 `entries.remove`、不计 consecutiveFailures |

### 3.3 Web Admin Frontend
| 文件 | 改动 |
|---|---|
| `frontend/src/api/client.ts:223-345` | `SseCallbacks.onNewLlmSuggestion` 新增 + `connectSseStream` 事件分发 case |
| `frontend/src/api/admin.ts:1-12, 155-165` | `listFeedback` API 方法 + `FeedbackItem` import |
| `frontend/src/types/admin.ts:171-185` | `FeedbackItem` interface 新增 |
| `frontend/src/pages/admin/FeedbackPage.tsx`（新） | 反馈中心页面，列表 + 分页 |
| `frontend/src/components/layout/AdminLayout.tsx:18` | 侧边栏加"用户反馈"入口 |
| `frontend/src/App.tsx:29, 111` | FeedbackPage lazy import + Route `/admin/feedback` |
| `frontend/src/types/amas.ts:67-86, 96-106, 11-26` | 4 字段补齐：`expectedForgetCost / lastSessionId / temporalPerformance / confusedWith` |
| `frontend/src/types/amas.generated.ts`（codegen） | 后端 schemars 自动生成（含所有 21 个 AMAS config 子结构） |
| `frontend/package.json` | `gen:amas-types` script + `json-schema-to-typescript` dev dep |
| `frontend/scripts/generate-amas-types.mjs`（新） | 从后端 `/api/admin/amas/config/schema` 拉取 + 编译 |

---

## 4. 验证命令

### 4.1 后端
```bash
cd /Users/liji/english/wordforge
cargo check                          # 编译通过
cargo test --test strict_mode_http   # 7/7 strict-mode 集成测试
cargo test --test telemetry_http     # telemetry payload 校验
cargo test --test records_http       # selfRating 落库
cargo test --test users_http         # delete_user 级联含 wb_center_imports
cargo test --test admin_amas_http    # metrics camelCase / monitoring schema
cargo test --test auth_http          # refreshToken JSON 暴露
```

### 4.2 前端
```bash
cd /Users/liji/english/wordforge/frontend
pnpm gen:amas-types                  # 从后端拉 schema 生成 amas.generated.ts
pnpm tsc --noEmit                    # 类型检查
pnpm test                            # vitest 单元测试
```

### 4.3 iOS
```bash
cd /Users/liji/WordForge-App
xcodebuild -scheme WordForge-App -destination 'generic/platform=iOS' -configuration Debug build
```

---

## 5. 100% 对齐结论

| 维度 | 状态 |
|---|---|
| iOS REST 契约 | 0 P0 / 0 P1（P1-1/P1-2 已修；P1-3 后端补） |
| 遥测通道 schema | 0 P0 / 0 P1（strict-mode middleware + payload 校验落地） |
| Admin 控制台契约 | 0 P0 / 0 P1（metrics camelCase + monitoring 包装 + feedback page） |
| AMAS schema 双向 | 0 P0 / 0 P1（default_w 对齐 + codegen 全量类型 + 4 字段兜底） |

**Cross-validator 终裁的 3 P0 + 9 P1 全部落地修复。** 5 项降级 P2 与 2 项 false positive 不阻断发版，由 docs/alignment.md 留存为下轮迭代输入。

---

## 6. 未来计划（非本轮）

- AMAS 配置项 PARAM_DICT 与 `tuning_whitelist.rs` 11 维路径列表的 cross-check 单测（P2-1）
- ColdStartPhase 枚举大小写统一（P2-3）：后端 `#[serde(rename_all = "snake_case")]`
- DAU/留存 UI 展示（Admin P1-3 降级 P2）
- iOS 真机回归 `杀进程→重启→自动 refresh` 验证 cookie 持久化（iOS agent 风险点 #2）
- selfRating 在 AMAS half-life 模型中的接入（本轮仅 DTO + 落库）
