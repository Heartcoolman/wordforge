//! 系统广播历史的 Store 操作（m032）。
//!
//! `broadcasts` 表只记录广播元数据 + 发送量（sent_count）；已读统计由关联
//! `notifications` 表实时算：每条广播给每个用户写一条通知，通知 id 形如
//! `{broadcast_id}_{user_id}`，read 0/1。readRate = read_count / sent_count。

use rusqlite::params;

use crate::store::{Store, StoreError};

/// 把广播行映射为元组（list 两种绑定路径共用，避免重复闭包）。
fn row_to_tuple(
    r: &rusqlite::Row,
) -> rusqlite::Result<(String, String, String, String, i64, String)> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

/// 单条广播历史（含实时算出的已读数）。
#[derive(Debug, Clone)]
pub struct BroadcastRow {
    pub id: String,
    pub title: String,
    pub message: String,
    pub admin_id: String,
    pub sent_count: i64,
    pub read_count: i64,
    pub created_at: String,
}

/// E2:历史列表筛选维度。下推后端跨页生效,不再前端单页 filter。
/// - `All`:窗口内全部
/// - `Week`:created_at 在近 7 天内
/// - `Failed`:sent_count = 0（受众零命中等导致未送达）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastFilter {
    All,
    Week,
    Failed,
}

impl BroadcastFilter {
    /// 把 admin-ui 传入的 filter 字符串解析为枚举（未知值回退 All）。
    pub fn from_param(s: Option<&str>) -> Self {
        match s {
            Some("week") => Self::Week,
            Some("failed") => Self::Failed,
            _ => Self::All,
        }
    }

    /// 拼接附加 WHERE 子句（不含起始 `AND`,由调用方按需拼）。
    /// week 用绑定参数 ?2 接 7 天前的 RFC3339 时间戳;failed 无绑定参数。
    fn extra_where(&self) -> &'static str {
        match self {
            Self::All => "",
            Self::Week => " AND created_at >= ?2",
            Self::Failed => " AND sent_count = 0",
        }
    }
}

impl Store {
    /// 记录一条广播历史。INSERT OR REPLACE 保证幂等（同 broadcast_id 重发覆盖）。
    pub fn record_broadcast(
        &self,
        id: &str,
        title: &str,
        message: &str,
        admin_id: &str,
        sent_count: usize,
        created_at: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO broadcasts
                (id, title, message, admin_id, sent_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, title, message, admin_id, sent_count as i64, created_at],
        )?;
        Ok(())
    }

    /// 近 N 天广播列表（按 created_at 倒序），每行附实时已读数。limit/offset 供分页。
    /// E2:filter 下推 WHERE（week/failed），跨页生效。week_cutoff_rfc3339 为近 7 天起点
    /// （filter=Week 时绑定到 SQL，其余 filter 忽略此参数）。
    pub fn list_broadcasts_30d(
        &self,
        since_rfc3339: &str,
        filter: BroadcastFilter,
        week_cutoff_rfc3339: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BroadcastRow>, StoreError> {
        let conn = self.conn()?;
        // week 占用绑定位 ?2,故 LIMIT/OFFSET 占位需随之顺移到 ?3/?4;其余 filter 用 ?2/?3。
        let sql = match filter {
            BroadcastFilter::Week => format!(
                "SELECT id, title, message, admin_id, sent_count, created_at
                 FROM broadcasts
                 WHERE created_at >= ?1{}
                 ORDER BY created_at DESC
                 LIMIT ?3 OFFSET ?4",
                filter.extra_where()
            ),
            _ => format!(
                "SELECT id, title, message, admin_id, sent_count, created_at
                 FROM broadcasts
                 WHERE created_at >= ?1{}
                 ORDER BY created_at DESC
                 LIMIT ?2 OFFSET ?3",
                filter.extra_where()
            ),
        };
        let mut stmt = conn.prepare(&sql)?;
        // 绑定参数:Week 时 ?1=since,?2=week_cutoff,?3=limit,?4=offset;否则 ?1=since,?2=limit,?3=offset。
        let base: Vec<(String, String, String, String, i64, String)> = match filter {
            BroadcastFilter::Week => stmt.query_map(
                params![since_rfc3339, week_cutoff_rfc3339, limit, offset],
                row_to_tuple,
            )?,
            _ => stmt.query_map(params![since_rfc3339, limit, offset], row_to_tuple)?,
        }
        .collect::<Result<Vec<_>, _>>()?;

        let mut rows = Vec::with_capacity(base.len());
        for (id, title, message, admin_id, sent_count, created_at) in base {
            let read_count = self.read_count_for_broadcast(&conn, &id)?;
            rows.push(BroadcastRow {
                id,
                title,
                message,
                admin_id,
                sent_count,
                read_count,
                created_at,
            });
        }
        Ok(rows)
    }

    /// E2:按 filter 过滤后的条数（供分页 total）。filter=All 时即窗口内全量。
    /// 陷阱要点:pagination.total 不能复用 stats.total（近30天全量），加 filter 后页脚必须用此计数。
    pub fn count_broadcasts_30d(
        &self,
        since_rfc3339: &str,
        filter: BroadcastFilter,
        week_cutoff_rfc3339: &str,
    ) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        let sql = format!(
            "SELECT COUNT(*) FROM broadcasts WHERE created_at >= ?1{}",
            filter.extra_where()
        );
        let count: i64 = match filter {
            BroadcastFilter::Week => {
                conn.query_row(&sql, params![since_rfc3339, week_cutoff_rfc3339], |r| {
                    r.get(0)
                })?
            }
            _ => conn.query_row(&sql, params![since_rfc3339], |r| r.get(0))?,
        };
        Ok(count)
    }

    /// 近 N 天聚合统计：(total 广播条数, total_sent 发送总量, avg_read_rate 加权已读率)。
    /// avg_read_rate = 全部已读数之和 / total_sent（total_sent=0 时 0）。
    pub fn broadcast_stats_30d(&self, since_rfc3339: &str) -> Result<(i64, i64, f64), StoreError> {
        let conn = self.conn()?;
        let (total, total_sent): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(sent_count), 0)
             FROM broadcasts WHERE created_at >= ?1",
            params![since_rfc3339],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        // 已读总数：对窗口内每条广播累加 read_count。admin 低频，逐条统计可接受。
        let ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM broadcasts WHERE created_at >= ?1")?;
            let ids = stmt
                .query_map(params![since_rfc3339], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids
        };
        let mut total_read = 0i64;
        for id in &ids {
            total_read += self.read_count_for_broadcast(&conn, id)?;
        }

        let avg_read_rate = if total_sent > 0 {
            total_read as f64 / total_sent as f64
        } else {
            0.0
        };
        Ok((total, total_sent, avg_read_rate))
    }

    /// 单条广播的已读数：notifications.id LIKE '{broadcast_id}_%' 且 read=1。
    /// 下划线在 LIKE 里是通配符，必须 ESCAPE 转义；broadcast id 是 UUID 本身无下划线。
    fn read_count_for_broadcast(
        &self,
        conn: &rusqlite::Connection,
        broadcast_id: &str,
    ) -> Result<i64, StoreError> {
        let read_count: i64 = conn.query_row(
            "SELECT COALESCE(SUM(read), 0)
             FROM notifications WHERE id LIKE ?1 || '\\_%' ESCAPE '\\'",
            params![broadcast_id],
            |r| r.get(0),
        )?;
        Ok(read_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    /// 直接插一条 notification（绕过 batch helper，便于精确控制 id 与 read）。
    fn insert_notif(store: &Store, user_id: &str, id: &str, read: bool) {
        let conn = store.conn().unwrap();
        conn.execute(
            "INSERT INTO notifications
                (user_id, id, notification_type, title, message, word_id, overdue_hours, read, created_at)
             VALUES (?1, ?2, 'broadcast', 'T', 'M', NULL, NULL, ?3, ?4)",
            params![user_id, id, read as i64, Utc::now().to_rfc3339()],
        )
        .unwrap();
    }

    #[test]
    fn record_and_list_with_read_count() {
        let store = test_store();
        let now = Utc::now().to_rfc3339();
        let bid = "11111111-1111-4111-8111-111111111111";
        store
            .record_broadcast(bid, "标题A", "正文A", "admin-1", 3, &now)
            .unwrap();

        // 3 条投递：2 条已读、1 条未读。
        insert_notif(&store, "u1", &format!("{bid}_u1"), true);
        insert_notif(&store, "u2", &format!("{bid}_u2"), true);
        insert_notif(&store, "u3", &format!("{bid}_u3"), false);

        let since = "1970-01-01T00:00:00+00:00";
        let rows = store
            .list_broadcasts_30d(since, BroadcastFilter::All, since, 50, 0)
            .unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.id, bid);
        assert_eq!(r.title, "标题A");
        assert_eq!(r.admin_id, "admin-1");
        assert_eq!(r.sent_count, 3);
        assert_eq!(r.read_count, 2);
    }

    #[test]
    fn read_count_does_not_leak_across_broadcasts() {
        let store = test_store();
        let now = Utc::now().to_rfc3339();
        let bid_a = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let bid_b = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        store
            .record_broadcast(bid_a, "A", "A", "adm", 1, &now)
            .unwrap();
        store
            .record_broadcast(bid_b, "B", "B", "adm", 1, &now)
            .unwrap();
        insert_notif(&store, "u1", &format!("{bid_a}_u1"), true);
        insert_notif(&store, "u1", &format!("{bid_b}_u1"), false);

        let since = "1970-01-01T00:00:00+00:00";
        let rows = store
            .list_broadcasts_30d(since, BroadcastFilter::All, since, 50, 0)
            .unwrap();
        let a = rows.iter().find(|r| r.id == bid_a).unwrap();
        let b = rows.iter().find(|r| r.id == bid_b).unwrap();
        assert_eq!(a.read_count, 1);
        assert_eq!(b.read_count, 0);
    }

    #[test]
    fn stats_weighted_avg_read_rate() {
        let store = test_store();
        let now = Utc::now().to_rfc3339();
        let bid_a = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let bid_b = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        // A: sent 4, read 1 ; B: sent 2, read 2 → 加权 = 3/6 = 0.5
        store
            .record_broadcast(bid_a, "A", "A", "adm", 4, &now)
            .unwrap();
        store
            .record_broadcast(bid_b, "B", "B", "adm", 2, &now)
            .unwrap();
        insert_notif(&store, "u1", &format!("{bid_a}_u1"), true);
        insert_notif(&store, "u2", &format!("{bid_a}_u2"), false);
        insert_notif(&store, "u1", &format!("{bid_b}_u1"), true);
        insert_notif(&store, "u2", &format!("{bid_b}_u2"), true);

        let since = "1970-01-01T00:00:00+00:00";
        let (total, total_sent, avg) = store.broadcast_stats_30d(since).unwrap();
        assert_eq!(total, 2);
        assert_eq!(total_sent, 6);
        assert!((avg - 0.5).abs() < 1e-9, "avg_read_rate = {avg}");
    }

    #[test]
    fn stats_empty_returns_zero() {
        let store = test_store();
        let (total, total_sent, avg) = store
            .broadcast_stats_30d("1970-01-01T00:00:00+00:00")
            .unwrap();
        assert_eq!(total, 0);
        assert_eq!(total_sent, 0);
        assert_eq!(avg, 0.0);
    }

    #[test]
    fn record_is_idempotent_replace() {
        let store = test_store();
        let now = Utc::now().to_rfc3339();
        let bid = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        store
            .record_broadcast(bid, "v1", "m", "adm", 1, &now)
            .unwrap();
        store
            .record_broadcast(bid, "v2", "m", "adm", 5, &now)
            .unwrap();
        let since = "1970-01-01T00:00:00+00:00";
        let rows = store
            .list_broadcasts_30d(since, BroadcastFilter::All, since, 50, 0)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "v2");
        assert_eq!(rows[0].sent_count, 5);
    }

    #[test]
    fn filter_failed_only_zero_sent_and_count_matches() {
        let store = test_store();
        let now = Utc::now().to_rfc3339();
        let bid_ok = "11111111-1111-4111-8111-111111111111";
        let bid_fail = "22222222-2222-4222-8222-222222222222";
        store
            .record_broadcast(bid_ok, "送达", "m", "adm", 3, &now)
            .unwrap();
        store
            .record_broadcast(bid_fail, "失败", "m", "adm", 0, &now)
            .unwrap();

        let since = "1970-01-01T00:00:00+00:00";
        let rows = store
            .list_broadcasts_30d(since, BroadcastFilter::Failed, since, 50, 0)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, bid_fail);
        // count 必须与 filter 后行数一致（陷阱核验：别只加 WHERE 忘了 count）
        let cnt = store
            .count_broadcasts_30d(since, BroadcastFilter::Failed, since)
            .unwrap();
        assert_eq!(cnt, 1);
        // All 计数仍为全量 2
        let cnt_all = store
            .count_broadcasts_30d(since, BroadcastFilter::All, since)
            .unwrap();
        assert_eq!(cnt_all, 2);
    }

    #[test]
    fn filter_week_excludes_old_and_count_matches() {
        let store = test_store();
        let recent = Utc::now().to_rfc3339();
        let old = (Utc::now() - chrono::Duration::days(20)).to_rfc3339();
        let bid_recent = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let bid_old = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        store
            .record_broadcast(bid_recent, "近", "m", "adm", 1, &recent)
            .unwrap();
        store
            .record_broadcast(bid_old, "旧", "m", "adm", 1, &old)
            .unwrap();

        let since = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let week_cutoff = (Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let rows = store
            .list_broadcasts_30d(&since, BroadcastFilter::Week, &week_cutoff, 50, 0)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, bid_recent);
        let cnt = store
            .count_broadcasts_30d(&since, BroadcastFilter::Week, &week_cutoff)
            .unwrap();
        assert_eq!(cnt, 1);
    }

    #[test]
    fn filter_from_param_parses() {
        assert_eq!(
            BroadcastFilter::from_param(Some("week")),
            BroadcastFilter::Week
        );
        assert_eq!(
            BroadcastFilter::from_param(Some("failed")),
            BroadcastFilter::Failed
        );
        assert_eq!(
            BroadcastFilter::from_param(Some("all")),
            BroadcastFilter::All
        );
        assert_eq!(BroadcastFilter::from_param(None), BroadcastFilter::All);
        assert_eq!(
            BroadcastFilter::from_param(Some("garbage")),
            BroadcastFilter::All
        );
    }
}
