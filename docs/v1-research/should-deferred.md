# SHOULD 项 S1/S2 v1.1 延后说明

> 起草日期：2026-05-22
> 范围：S1（routes 拆分）+ S2（records → AMAS 事件总线化）
> 决策：v1.0 GA 前不实施代码拆分，文档化承诺到 v1.1

## S1 · `routes/learning.rs` + `routes/records.rs` 拆分

### 现状

- `src/routes/learning.rs`：约 1398 行（含 8 个 lifecycle handler：session_start / study / progress / pick / submit / complete / list / detail）
- `src/routes/records.rs`：约 849 行（含 3 类：single / batch / sync）

### v1.0 延后的理由

1. **可见性约束**：`pub(crate)` 项被多个内部模块共用（`CreateRecordRequest` / `CreateRecordResponse` / `UserStateSnapshot` / `capture_user_state_snapshot` / `restore_user_state_snapshot`）；按 lifecycle 拆分后 `pub use` 重新导出违反 Rust E0364/E0365 可见性规则，需将 `pub(crate)` 升 `pub` 或重新设计 module 层级。
2. **`IntoResponse` 导入散落**：handler 大量 `Json(...).into_response()` 调用，拆分后每个子模块需补 `use axum::response::IntoResponse;` 否则编译断（E0599）。
3. **测试断言依赖**：`tests/learning_http.rs` / `tests/records_*.rs` 大量直接 `use learning_backend::routes::learning::*;` 全量导入，拆分后需逐项重写测试 import。

GA §6.2 门校验中 dev-arch-2 一次拆分尝试触发 10 个编译错（pub(crate) 重导出 + IntoResponse 缺失 + 测试 import 漂移），评估代价超出"SHOULD 不阻塞 GA"的预算。

### v1.1 实施路径

1. 先做"可见性扁平化"：把 `pub(crate)` 升 `pub`，跑一次 `cargo build` 验证无副作用
2. 然后按 lifecycle 拆 `routes/learning/{session,study,progress,pick,submit,complete,list,detail}.rs`
3. records.rs 拆 `records/{single,batch,sync}.rs`
4. 公开路由签名（`pub fn router() -> Router<AppState>`）保持不变
5. 测试 import 用 `use learning_backend::routes::learning::Router;` 入口形式
6. clippy 同步清债（包含 v1.0 接受的 56 历史警告）

### v1.0 已做的最小准备

- M1-A1 已删 services 层，handler 直接依赖 `Store` + `AMASEngine`（拆分时不再受 service 影响）
- M1-A3 已删 4 stub worker，避免拆分时清理无效引用
- M1-A6 strict-mode/maintenance 已改路由元数据驱动，拆分后元数据自动继承

## S2 · records → AMAS 事件总线化

### 现状

`process_event` 走"先 commit 学习记录 → 同步调用 AMAS engine 更新状态"路径。

- 优点：单事务，无中间状态可见性问题
- 缺点：AMAS 更新失败时需手动 `rollback_record`（`src/routes/records.rs:create_record` line 280-330 有 23 行手动 rollback 逻辑）
- 风险：事务一致性不是 ACID 严格保证，是"应用层 best-effort rollback"。

### v1 内承诺方向（不实装）

引入 in-process 事件通道或 outbox 表：

```
records.create_record()
  ↓ commit DB record (single tx)
  ↓ enqueue AMAS event (in-memory channel OR DB outbox table)
  ↓ return HTTP response (low latency)
        ↓
   AMAS engine consumer task (async)
        ↓ process event → update word_states / amas_user_state
        ↓ on failure：retry with backoff + DLQ table
```

### v1.0 已做的最小准备

- M1-A2 AMASEngine 锁中毒防护已加（防止事件总线消费侧 panic 整库 hang）
- M0-P1 /metrics 端点已加 `amas_process_event_duration_seconds` histogram（事件总线化后可监控吞吐）
- M1-A5 cron scheduler 健康监测加了 worker_last_run 表（事件总线 worker 上线后可复用）

### v1.1 实施考虑

- 引入 `outbox` 表 + 后台 worker 消费（不强依赖 channel，避免重启丢失）
- 加 `events_dead_letter` 表存重试失败事件
- admin /admin/monitoring 加 outbox lag / dead-letter count 监控
- 兼容老 process_event 同步路径（feature flag 渐进切换）

---

## 总结

S1 + S2 在 v1.0 阶段以**文档化承诺**形式收口，承诺 v1.1 实装。v1.0 已为这两项做了必要的前置基础设施工作（M1-A1/A2/A3/A6 + M0-P1）。这与 RFC §4.2 "SHOULD（v1.0 内尽量，但不阻塞 GA）" 定位一致。
