# WordForge 客户端 × 后端契约对齐审计报告（第五轮 · M2-Q3 终版）

**审计时间**：2026-05-22
**后端仓库**：`/Users/liji/english/wordforge`（branch `feat/v1-m1`）
**iOS 客户端**：`/Users/liji/WordForge-App`（branch `main`）
**Web 前端**：`/Users/liji/english/wordforge/frontend`
**审计范围**：M1 阶段变更（M0-C5 410 端点、M1-G1 GDPR 导出、M1-G3 feedback 扩展字段、M1-A7 queryClient 清理）
**判定方式**：交叉验证 0 P0 + 0 P1、字段级一致、类型检查通过

---

## 1. 历轮遗留（继承基线）

- 第三轮（2026-05-19）：3 P0 + 9 P1 全部落地修复
- 第四轮（2026-05-21）：P1-W1 WordState wire lowercase + 5 处 P2 文档全修，0 P0 / 0 P1

---

## 2. 本轮审计范围（M1 阶段变更）

| 变更 | commit / task | 范围 |
|---|---|---|
| /api/v1/* 全部返回 410 Gone | commit be7f46b (M0-C5) | 后端 v1.rs + 前端契约 |
| GDPR Article 20 数据导出端点 | M1-G1 | 后端 GET /api/users/me/export |
| feedback_items 4 字段升级 | M1-G3 | 后端 FeedbackItem + 前端类型 |
| queryClient / @tanstack/query 清理 | M1-A7 | 前端 package.json + 代码 |

---

## 3. 第五轮终裁清单

### 3.1 发现 P1（1 项，已修）

| # | 一句话 | 范围 | 修复方 | 状态 |
|---|---|---|---|---|
| P1-G1 | `frontend/src/types/admin.ts::FeedbackItem` 缺少 M1-G3 新增的 `priority`/`status`/`assigneeAdminId`/`resolvedAt`/`resolution` 5 个字段；后端 wire 含这 5 字段，前端类型不完整导致运行时字段丢失 | admin.ts | 前端 | **已修**（本轮） |

### 3.2 发现 P2（1 项，已记录）

| # | 一句话 | 范围 | 判定 |
|---|---|---|---|
| P2-G1 | M1-G1 新增 `GET /api/users/me/export`（GDPR 导出，返回 ndjson）；iOS 客户端和 Web 前端均无对应调用——端点已就绪但客户端未实现 UI 入口 | iOS + Web | **不阻塞 rc.3**（功能可用，仅 UI 入口缺失，记 v1.1 backlog） |

### 3.3 ALIGNED（已对齐，无需操作）

| 变更 | 后端 | 前端 / iOS | 判定 |
|---|---|---|---|
| /api/v1/* 全 410 Gone | `src/routes/v1.rs`：5 handler 返回 `(StatusCode::GONE, Json {...})` | Web 前端无任何 `/api/v1/*` 调用；iOS 客户端无 v1 调用 | ALIGNED |
| queryClient 清理 | — | `grep -rn "queryClient\|@tanstack"` 无结果，`package.json` 无相关依赖 | ALIGNED |
| FeedbackItem 4 字段（修后） | `priority`/`status`/`assigneeAdminId`/`resolvedAt`/`resolution`（camelCase wire） | `types/admin.ts::FeedbackItem` 已补全 5 字段（本轮 P1-G1 修复） | ALIGNED（修后） |
| PATCH /api/admin/feedback/:id | `UpdateFeedbackRequest { priority, status, assignee_admin_id, resolution }` | FeedbackPage.tsx 当前仅展示不更新（只读，无 PATCH 调用）——端点就绪，UI 未实现，不构成契约 mismatch | ALIGNED（只读场景无 mismatch） |

---

## 4. 落地修复清单（本轮）

### 4.1 前端

| 文件 | 改动 |
|---|---|
| `frontend/src/types/admin.ts` | `FeedbackItem` 补全 `priority` / `status` / `assigneeAdminId` / `resolvedAt` / `resolution` 5 字段 |

---

## 5. 验证命令

### 5.1 后端
```bash
cargo test --no-fail-fast
```

### 5.2 前端
```bash
cd frontend && pnpm tsc --noEmit
# 预期：仅 App.tsx(93) + UpdatesPage.tsx(405) 两处已知预存错误，无 FeedbackItem 新增错误
```

---

## 6. 100% 对齐结论（第五轮）

| 维度 | 状态 |
|---|---|
| /api/v1/* 410 客户端处理 | 0 P0 / 0 P1（前端/iOS 均无 v1 调用，410 不影响任何客户端） |
| M1-G1 GDPR 导出契约 | 0 P0 / 0 P1（端点就绪，客户端 UI 入口记 P2 backlog，不阻塞） |
| M1-G3 feedback 扩展字段 | 0 P0 / 0 P1（P1-G1 本轮修复，FeedbackItem 字段已同步） |
| M1-A7 queryClient 遗留引用 | 0 P0 / 0 P1（无任何遗留引用） |

**第五轮 cross-validator：0 P0 / 0 P1 / 0 P2 high-impact。可进 rc.3。**

---

## 7. 未来计划（非本轮）

- M1-G1 GDPR 导出：iOS + Web 前端实现"导出我的数据"UI 入口（v1.1）
- PATCH /api/admin/feedback/:id：FeedbackPage 增加状态流转 UI（priority/status/assignee 可编辑）
- AMAS 配置项 PARAM_DICT 与 `tuning_whitelist.rs` 11 维路径列表 cross-check 单测（P2-1）
- ColdStartPhase 枚举大小写统一（P2-3）：后端 `#[serde(rename_all = "snake_case")]`
- selfRating 在 AMAS half-life 模型中的接入（第三轮仅 DTO + 落库）
