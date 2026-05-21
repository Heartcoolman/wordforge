// M0-C4：Deprecation/Sunset header 中间件（RFC 8594）
pub mod deprecation;
pub mod device;
/// M0-P1 补充：http_request_duration_seconds histogram 采集
pub mod http_metrics;
pub mod maintenance;
pub mod rate_limit;
pub mod request_id;
pub mod strict_mode;
