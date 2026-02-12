// TODO: 实现 LLM 学习建议 worker。需要调用外部 LLM API，基于用户学习数据
// 生成个性化学习建议和策略调整推荐，存储到通知系统供用户查看。
use crate::store::Store;

pub async fn run(_store: &Store) {
    tracing::debug!("llm_advisor: start");
    tracing::debug!("llm_advisor: done (stub)");
}
