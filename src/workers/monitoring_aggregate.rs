// TODO: 实现监控聚合 worker。需要定期聚合各项系统指标（请求延迟、错误率、
// 活跃用户数等），写入时序数据供 admin monitoring 面板查询。
use crate::store::Store;

pub async fn run(_store: &Store) {
    tracing::debug!("monitoring_aggregate: start");
    tracing::debug!("monitoring_aggregate: done (stub)");
}
