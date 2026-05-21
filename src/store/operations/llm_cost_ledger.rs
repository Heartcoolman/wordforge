//! M1-G2：LLM 顾问月度成本台账。
//!
//! 每次 LLM 调用完成后，将折算的人民币成本写入 `llm_advisor_cost_ledger` 表；
//! worker 运行前查询当月累计，若已达上限则跳过本次调用。

use rusqlite::{params, OptionalExtension};

use crate::store::{Store, StoreError};

impl Store {
    /// 查询指定日历月（YYYY-MM）的累计人民币成本；无记录返回 0.0。
    pub fn get_llm_cost_this_month(&self, month: &str) -> Result<f64, StoreError> {
        let conn = self.conn()?;
        let total: Option<f64> = conn
            .query_row(
                "SELECT total_yuan FROM llm_advisor_cost_ledger WHERE month=?1",
                params![month],
                |r| r.get(0),
            )
            .optional()?;
        Ok(total.unwrap_or(0.0))
    }

    /// 将本次调用成本（人民币）累加到指定月份的台账（upsert）。
    pub fn add_llm_cost(&self, month: &str, cost_yuan: f64) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO llm_advisor_cost_ledger (month, total_yuan, last_updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(month) DO UPDATE SET
                total_yuan = total_yuan + ?2,
                last_updated_at = ?3",
            params![month, cost_yuan, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    #[test]
    fn returns_zero_when_no_record() {
        let s = store();
        assert!((s.get_llm_cost_this_month("2026-05").unwrap() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn add_and_get_roundtrip() {
        let s = store();
        s.add_llm_cost("2026-05", 3.14).unwrap();
        let got = s.get_llm_cost_this_month("2026-05").unwrap();
        assert!((got - 3.14).abs() < 1e-6);
    }

    #[test]
    fn add_accumulates_across_calls() {
        let s = store();
        s.add_llm_cost("2026-05", 1.0).unwrap();
        s.add_llm_cost("2026-05", 2.5).unwrap();
        let got = s.get_llm_cost_this_month("2026-05").unwrap();
        assert!((got - 3.5).abs() < 1e-6);
    }

    #[test]
    fn different_months_are_independent() {
        let s = store();
        s.add_llm_cost("2026-05", 10.0).unwrap();
        s.add_llm_cost("2026-06", 5.0).unwrap();
        assert!((s.get_llm_cost_this_month("2026-05").unwrap() - 10.0).abs() < 1e-6);
        assert!((s.get_llm_cost_this_month("2026-06").unwrap() - 5.0).abs() < 1e-6);
    }
}
