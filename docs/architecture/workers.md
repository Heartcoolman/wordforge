# 后台任务系统

系统内置 17+ 定时后台任务，基于 `tokio-cron-scheduler` 自动维护数据质量和学习效果。

通过环境变量 `WORKER_LEADER=true` 控制是否运行后台任务（多实例部署时仅主节点开启）。

## 任务清单

| 任务 | 功能 |
|------|------|
| `session_cleanup` | 清理过期会话 |
| `password_reset_cleanup` | 清理过期密码重置令牌 |
| `forgetting_alert` | 生成遗忘预警通知 |
| `daily_aggregation` | 每日学习数据聚合 |
| `weekly_report` | 周度学习报告生成 |
| `delayed_reward` | 延迟奖励信号计算 |
| `metrics_flush` | 引擎指标持久化 |
| `cache_cleanup` | 缓存数据清理 |
| `algorithm_optimization` | 算法参数自优化 |
| `health_analysis` | 系统健康分析 |
| `monitoring_aggregate` | 监控数据聚合 |
| `log_export` | 日志导出 |

## 代码位置

```
src/workers/
├── session_cleanup.rs
├── password_reset_cleanup.rs
├── forgetting_alert.rs
├── daily_aggregation.rs
├── weekly_report.rs
├── delayed_reward.rs
├── metrics_flush.rs
├── cache_cleanup.rs
├── algorithm_optimization.rs
├── health_analysis.rs
├── monitoring_aggregate.rs
├── log_export.rs
└── ...
```
