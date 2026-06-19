pub mod amas;
pub mod auth;
pub mod blocking;
pub mod clock_health;
pub mod config;
pub mod constants;
pub mod extractors;
pub mod logging;
pub mod logging_buffer;
pub mod metrics_counters;
pub mod middleware;
// M0-C2：25 个 v1-stable 端点的 OpenAPI 3.1 集中声明
pub mod openapi;
pub mod resource_sampler;
pub mod response;
pub mod routes;
pub mod services;
// BA3b：records 热路径分阶段管线计时 + 单记录瀑布 trace（进程内）
pub mod stage_metrics;
pub mod state;
pub mod store;
pub mod validation;
pub mod workers;
