// M0-C4：Deprecation/Sunset header 中间件（RFC 8594）
pub mod deprecation;
pub mod device;
/// M0-P1 补充：http_request_duration_seconds histogram 采集
pub mod http_metrics;
pub mod maintenance;
pub mod rate_limit;
pub mod request_id;
pub mod strict_mode;

/// M1-A6：路由元数据类型，预留给未来与 M0-C4 deprecation 体系统一（Extension 侧）。
/// 当前用于文档化豁免规则；实际豁免通过路由分组实现（见 routes/mod.rs）。
#[derive(Clone, Debug, Default)]
pub struct RouteMetadata {
    /// 弃用起始日期（RFC 1123），仅信息用途；None 表示未弃用
    pub deprecated_since: Option<&'static str>,
}
