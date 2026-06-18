//! T1.3 真实留存 A/B 验证闭环的 Store 层：实验注册、用户桶归属、mastery 事件、按 arm 的
//! 北极星指标原始计数聚合。
//!
//! 设计要点：
//! - 切分维度独立于 config_version（experiment_id + arm），使 no-op A/A 可分。
//! - 留存/完成率/reviewsPerDay/masteredCount 一律走全量业务表（learning_records /
//!   learning_sessions / word_mastery_events），**不读 5% 采样的 engine_monitoring_events**
//!   （该表 anomaly/cold_start 必采 → 有选择性偏置，不可做无偏率估计）。
//! - cohort Day-0 锚点 = user_bucket_assignment.first_assigned_at（首次进桶日，决策3）。
//! - 本层只产原始计数 {successes, n} / {mean, var, n}；CI、p 值、采纳门由 route 层注入
//!   预注册阈值后用 amas::experiment::stats 计算（store 层无配置访问，对齐 compare_ext 模式）。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

/// 一条实验注册记录。camelCase 序列化供 admin API。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentRow {
    pub experiment_id: String,
    pub suggestion_id: Option<i64>,
    pub canary_version_hash: String,
    pub baseline_version_hash: String,
    pub canary_cohort_lo: u32,
    pub canary_cohort_hi: u32,
    pub primary_metric: String,
    pub min_sample: u64,
    pub alpha: f64,
    pub power: f64,
    pub mde: f64,
    pub offline_delta: Option<f64>,
    pub status: String,
    pub registered_at: String,
    pub concluded_at: Option<String>,
    pub notes: Option<String>,
}

/// 注册新实验的入参。
#[derive(Debug, Clone)]
pub struct NewExperiment {
    pub experiment_id: String,
    pub suggestion_id: Option<i64>,
    pub canary_version_hash: String,
    pub baseline_version_hash: String,
    pub canary_cohort_lo: u32,
    pub canary_cohort_hi: u32,
    pub primary_metric: String,
    pub min_sample: u64,
    pub alpha: f64,
    pub power: f64,
    pub mde: f64,
    pub offline_delta: Option<f64>,
    pub notes: Option<String>,
}

const EXP_COLS: &str = "experiment_id, suggestion_id, canary_version_hash, baseline_version_hash, \
    canary_cohort_lo, canary_cohort_hi, primary_metric, min_sample, alpha, power, mde, \
    offline_delta, status, registered_at, concluded_at, notes";

fn row_to_experiment(r: &rusqlite::Row<'_>) -> rusqlite::Result<ExperimentRow> {
    Ok(ExperimentRow {
        experiment_id: r.get(0)?,
        suggestion_id: r.get(1)?,
        canary_version_hash: r.get(2)?,
        baseline_version_hash: r.get(3)?,
        canary_cohort_lo: r.get::<_, i64>(4)? as u32,
        canary_cohort_hi: r.get::<_, i64>(5)? as u32,
        primary_metric: r.get(6)?,
        min_sample: r.get::<_, i64>(7)? as u64,
        alpha: r.get(8)?,
        power: r.get(9)?,
        mde: r.get(10)?,
        offline_delta: r.get(11)?,
        status: r.get(12)?,
        registered_at: r.get(13)?,
        concluded_at: r.get(14)?,
        notes: r.get(15)?,
    })
}

/// 单臂某比例指标的原始计数（successes / n）。CI 与检验在 route 层算。
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProportionRaw {
    pub successes: u64,
    pub n: u64,
}

/// 单臂某连续指标的原始矩（均值 / 样本方差 / 样本量）。
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeanRaw {
    pub mean: f64,
    /// 样本方差（n-1 分母）；n<2 时为 0。
    pub var: f64,
    pub n: u64,
}

/// 单臂全部北极星指标的原始计数。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmRaw {
    pub arm: String,
    pub enrolled: u64,
    pub day7_retention: ProportionRaw,
    pub day30_retention: ProportionRaw,
    pub reviews_per_day: MeanRaw,
    pub session_completion: ProportionRaw,
    pub mastered_hold: ProportionRaw,
}

/// 一个实验两臂的原始计数对（canary vs baseline）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentRawMetrics {
    pub experiment_id: String,
    pub canary: ArmRaw,
    pub baseline: ArmRaw,
}

impl Store {
    // ---- 实验注册表 CRUD ----

    /// 注册一个新实验。要求当前无 running 实验（单 active 实验模型，保证桶归属唯一）。
    /// cohort 区间须 ⊂ [0,100] 且 lo<hi。
    pub fn register_experiment(
        &self,
        exp: &NewExperiment,
        now: &str,
    ) -> Result<ExperimentRow, StoreError> {
        if exp.canary_cohort_lo >= exp.canary_cohort_hi || exp.canary_cohort_hi > 100 {
            return Err(StoreError::Validation(format!(
                "非法 canary cohort 区间 [{}, {})",
                exp.canary_cohort_lo, exp.canary_cohort_hi
            )));
        }
        // 输入域守卫：alpha/power 越界会击穿统计闸（alpha≥1 使 p<alpha 恒真→显著性判定 no-op；
        // alpha≤0 → z=±inf → CI 变 NaN/null）。mde<0、min_sample<1 同样让采纳门失效。
        if !(exp.alpha > 0.0 && exp.alpha < 1.0) {
            return Err(StoreError::Validation(format!(
                "alpha 须 ∈ (0,1)，实际 {}",
                exp.alpha
            )));
        }
        if !(exp.power > 0.0 && exp.power < 1.0) {
            return Err(StoreError::Validation(format!(
                "power 须 ∈ (0,1)，实际 {}",
                exp.power
            )));
        }
        if exp.mde < 0.0 {
            return Err(StoreError::Validation(format!("mde 须 ≥ 0，实际 {}", exp.mde)));
        }
        if exp.min_sample < 1 {
            return Err(StoreError::Validation("min_sample 须 ≥ 1".into()));
        }
        let conn = self.conn()?;
        let running: i64 = conn.query_row(
            "SELECT COUNT(*) FROM amas_experiments WHERE status = 'running'",
            [],
            |r| r.get(0),
        )?;
        if running > 0 {
            return Err(StoreError::Validation(
                "已有 running 实验，先结束它再注册新实验（单 active 实验模型）".into(),
            ));
        }
        conn.execute(
            "INSERT INTO amas_experiments (
                experiment_id, suggestion_id, canary_version_hash, baseline_version_hash,
                canary_cohort_lo, canary_cohort_hi, primary_metric, min_sample, alpha, power,
                mde, offline_delta, status, registered_at, concluded_at, notes
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'running',?13,NULL,?14)",
            params![
                exp.experiment_id,
                exp.suggestion_id,
                exp.canary_version_hash,
                exp.baseline_version_hash,
                exp.canary_cohort_lo as i64,
                exp.canary_cohort_hi as i64,
                exp.primary_metric,
                exp.min_sample as i64,
                exp.alpha,
                exp.power,
                exp.mde,
                exp.offline_delta,
                now,
                exp.notes,
            ],
        )
        .map_err(|e| match e {
            // 唯一部分索引兜底跨连接 TOCTOU：并发注册时另一行 INSERT 命中 status='running' 唯一约束。
            rusqlite::Error::SqliteFailure(f, _)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                StoreError::Validation("已有 running 实验（并发注册被唯一索引拦截）".into())
            }
            other => StoreError::Sqlite(other),
        })?;
        // 先释放本连接再回读：get_experiment 会另取一个连接，pool_size=1 下若不释放会自死锁。
        drop(conn);
        self.get_experiment(&exp.experiment_id)?.ok_or_else(|| {
            StoreError::Validation("register_experiment 后立即回读失败".into())
        })
    }

    pub fn get_experiment(&self, experiment_id: &str) -> Result<Option<ExperimentRow>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                &format!("SELECT {EXP_COLS} FROM amas_experiments WHERE experiment_id = ?1"),
                params![experiment_id],
                row_to_experiment,
            )
            .optional()?;
        Ok(row)
    }

    /// 当前 running 实验（至多一个）。供引擎热路径按桶定 arm。
    pub fn get_running_experiment(&self) -> Result<Option<ExperimentRow>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                &format!(
                    "SELECT {EXP_COLS} FROM amas_experiments WHERE status='running' \
                     ORDER BY registered_at DESC LIMIT 1"
                ),
                [],
                row_to_experiment,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list_experiments(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<ExperimentRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {EXP_COLS} FROM amas_experiments \
             WHERE (?1 IS NULL OR status = ?1) ORDER BY registered_at DESC"
        ))?;
        let rows: Result<Vec<ExperimentRow>, rusqlite::Error> =
            stmt.query_map(params![status], row_to_experiment)?.collect();
        Ok(rows?)
    }

    /// 结束实验。status 须为 concluded_adopt / concluded_reject。
    pub fn conclude_experiment(
        &self,
        experiment_id: &str,
        status: &str,
        now: &str,
    ) -> Result<(), StoreError> {
        if status != "concluded_adopt" && status != "concluded_reject" {
            return Err(StoreError::Validation(format!("非法 conclude status: {status}")));
        }
        let conn = self.conn()?;
        let n = conn.execute(
            "UPDATE amas_experiments SET status=?2, concluded_at=?3 \
             WHERE experiment_id=?1 AND status='running'",
            params![experiment_id, status, now],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound {
                entity: "running experiment".into(),
                key: experiment_id.into(),
            });
        }
        Ok(())
    }

    // ---- 桶归属 + mastery 事件（写路径） ----

    /// 冻结用户首次进桶归属。INSERT OR IGNORE：已存在则保留首次 arm/bucket/first_assigned_at，
    /// scale/rollback 后跨桶漂移不改写 → enrollment 时点归因，不串桶。
    pub fn enroll_user_bucket(
        &self,
        user_id: &str,
        experiment_id: &str,
        arm: &str,
        bucket: u32,
        now: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO user_bucket_assignment
                (user_id, experiment_id, arm, bucket, first_assigned_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![user_id, experiment_id, arm, bucket as i64, now],
        )?;
        Ok(())
    }

    /// 记一条 mastered/forgotten 转移事件（append-only）。
    pub fn insert_word_mastery_event(
        &self,
        user_id: &str,
        word_id: &str,
        event: &str,
        now: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO word_mastery_events (user_id, word_id, event, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![user_id, word_id, event, now],
        )?;
        Ok(())
    }

    // ---- 按 arm 的北极星指标原始计数聚合（读路径，走全量业务表） ----

    /// 取实验两臂的原始计数。`now_date` = 'YYYY-MM-DD' 截断（eligible 守卫用）。
    pub fn experiment_raw_metrics(
        &self,
        experiment_id: &str,
    ) -> Result<ExperimentRawMetrics, StoreError> {
        let now_date = chrono::Utc::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        Ok(ExperimentRawMetrics {
            experiment_id: experiment_id.to_string(),
            canary: self.arm_raw(experiment_id, "canary", &now_date)?,
            baseline: self.arm_raw(experiment_id, "baseline", &now_date)?,
        })
    }

    fn arm_raw(
        &self,
        experiment_id: &str,
        arm: &str,
        now_date: &str,
    ) -> Result<ArmRaw, StoreError> {
        let conn = self.conn()?;

        // enrolled + Day-7/Day-30 留存（注册队列锚=first_assigned_at）。eligible 守卫要求**测量窗口
        // 完全过去**：Day-7 窗口 [+6,+8) → 须 +8天<=今天；Day-30 窗口 [+29,+31) → 须 +31天<=今天。
        // 避免把窗口末日尚未走完的边界 cohort 计入分母而系统性低估留存。
        let (enrolled, d7_elig, d7_ret, d30_elig, d30_ret): (i64, i64, i64, i64, i64) = conn
            .query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(CASE WHEN DATE(uba.first_assigned_at,'+8 days') <= ?3 THEN 1 ELSE 0 END),0),
                    COALESCE(SUM(CASE WHEN DATE(uba.first_assigned_at,'+8 days') <= ?3 AND EXISTS(
                        SELECT 1 FROM learning_records lr WHERE lr.user_id = uba.user_id
                          AND DATE(lr.created_at) >= DATE(uba.first_assigned_at,'+6 days')
                          AND DATE(lr.created_at) <  DATE(uba.first_assigned_at,'+8 days')
                    ) THEN 1 ELSE 0 END),0),
                    COALESCE(SUM(CASE WHEN DATE(uba.first_assigned_at,'+31 days') <= ?3 THEN 1 ELSE 0 END),0),
                    COALESCE(SUM(CASE WHEN DATE(uba.first_assigned_at,'+31 days') <= ?3 AND EXISTS(
                        SELECT 1 FROM learning_records lr WHERE lr.user_id = uba.user_id
                          AND DATE(lr.created_at) >= DATE(uba.first_assigned_at,'+29 days')
                          AND DATE(lr.created_at) <  DATE(uba.first_assigned_at,'+31 days')
                    ) THEN 1 ELSE 0 END),0)
                 FROM user_bucket_assignment uba
                 WHERE uba.experiment_id = ?1 AND uba.arm = ?2",
                params![experiment_id, arm, now_date],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;

        // 会话完成率：completed / (completed + abandoned)，进桶后创建的会话。
        let (sess_completed, sess_total): (i64, i64) = conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN ls.status='completed' THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN ls.status IN ('completed','abandoned') THEN 1 ELSE 0 END),0)
             FROM user_bucket_assignment uba
             JOIN learning_sessions ls
               ON ls.user_id = uba.user_id AND ls.created_at >= uba.first_assigned_at
             WHERE uba.experiment_id = ?1 AND uba.arm = ?2",
            params![experiment_id, arm],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        // 长期 masteredCount：mastered 后 Day-30 仍未 Forgotten（决策2）。**词级**口径：先按
        // (user,word) 取该词最后一次 mastered 事件（同词 master→forget→re-master 只算一个词，不按
        // mastered 次数膨胀分母），再判 eligible（最后一次 mastered 已过完30天）与 held（其后30天内
        // 无 forgotten）。分母=词数，与 schema 注释及全仓 masteredCount 词级口径一致。
        let (mh_elig, mh_held): (i64, i64) = conn.query_row(
            "WITH last_mastered AS (
                SELECT m.user_id, m.word_id, MAX(m.created_at) AS mastered_at
                FROM word_mastery_events m
                JOIN user_bucket_assignment uba
                  ON uba.user_id = m.user_id AND uba.experiment_id = ?1 AND uba.arm = ?2
                WHERE m.event='mastered' AND m.created_at >= uba.first_assigned_at
                GROUP BY m.user_id, m.word_id
             )
             SELECT
                COALESCE(SUM(CASE WHEN DATE(lm.mastered_at,'+30 days') <= ?3 THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN DATE(lm.mastered_at,'+30 days') <= ?3 AND NOT EXISTS(
                    SELECT 1 FROM word_mastery_events f
                    WHERE f.user_id=lm.user_id AND f.word_id=lm.word_id AND f.event='forgotten'
                      AND f.created_at >= lm.mastered_at
                      AND DATE(f.created_at) < DATE(lm.mastered_at,'+30 days')
                ) THEN 1 ELSE 0 END),0)
             FROM last_mastered lm",
            params![experiment_id, arm, now_date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        // reviewsPerDay：每个留存用户 = 记录数 / 活跃天数；跨用户取均值 + 样本方差（Rust 算）。
        let mut stmt = conn.prepare(
            "SELECT CAST(COUNT(lr.id) AS REAL) / COUNT(DISTINCT DATE(lr.created_at))
             FROM user_bucket_assignment uba
             JOIN learning_records lr
               ON lr.user_id = uba.user_id AND lr.created_at >= uba.first_assigned_at
             WHERE uba.experiment_id = ?1 AND uba.arm = ?2
             GROUP BY uba.user_id",
        )?;
        let rpd: Vec<f64> = stmt
            .query_map(params![experiment_id, arm], |r| r.get::<_, f64>(0))?
            .filter_map(Result::ok)
            .collect();
        let reviews_per_day = mean_and_var(&rpd);

        Ok(ArmRaw {
            arm: arm.to_string(),
            enrolled: enrolled as u64,
            day7_retention: ProportionRaw {
                successes: d7_ret as u64,
                n: d7_elig as u64,
            },
            day30_retention: ProportionRaw {
                successes: d30_ret as u64,
                n: d30_elig as u64,
            },
            reviews_per_day,
            session_completion: ProportionRaw {
                successes: sess_completed as u64,
                n: sess_total as u64,
            },
            mastered_hold: ProportionRaw {
                successes: mh_held as u64,
                n: mh_elig as u64,
            },
        })
    }
}

/// 均值 + 样本方差（n-1 分母）。空切片 → 全 0；n=1 → var 0。
fn mean_and_var(xs: &[f64]) -> MeanRaw {
    let n = xs.len() as u64;
    if n == 0 {
        return MeanRaw::default();
    }
    let mean = xs.iter().sum::<f64>() / n as f64;
    let var = if n < 2 {
        0.0
    } else {
        xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0)
    };
    MeanRaw { mean, var, n }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        store.run_migrations().unwrap();
        (dir, store)
    }

    fn new_exp() -> NewExperiment {
        NewExperiment {
            experiment_id: "exp1".into(),
            suggestion_id: None,
            canary_version_hash: "vc".into(),
            baseline_version_hash: "vb".into(),
            canary_cohort_lo: 0,
            canary_cohort_hi: 50,
            primary_metric: "day7_retention".into(),
            min_sample: 100,
            alpha: 0.05,
            power: 0.8,
            mde: 0.05,
            offline_delta: None,
            notes: None,
        }
    }

    #[test]
    fn register_rejects_second_running_and_concludes() {
        let (_d, s) = tmp_store();
        s.register_experiment(&new_exp(), "2026-06-01T00:00:00+00:00").unwrap();
        // 第二个 running 被拒
        let mut e2 = new_exp();
        e2.experiment_id = "exp2".into();
        assert!(matches!(
            s.register_experiment(&e2, "2026-06-01T00:00:00+00:00"),
            Err(StoreError::Validation(_))
        ));
        assert!(s.get_running_experiment().unwrap().is_some());
        // 结束后可注册新的
        s.conclude_experiment("exp1", "concluded_adopt", "2026-06-10T00:00:00+00:00")
            .unwrap();
        assert!(s.get_running_experiment().unwrap().is_none());
        s.register_experiment(&e2, "2026-06-11T00:00:00+00:00").unwrap();
        assert_eq!(s.get_running_experiment().unwrap().unwrap().experiment_id, "exp2");
    }

    #[test]
    fn register_rejects_out_of_domain_params() {
        let (_d, s) = tmp_store();
        for bad in [
            {
                let mut e = new_exp();
                e.alpha = 1.5;
                e
            },
            {
                let mut e = new_exp();
                e.alpha = 0.0;
                e
            },
            {
                let mut e = new_exp();
                e.power = 1.0;
                e
            },
            {
                let mut e = new_exp();
                e.mde = -0.1;
                e
            },
            {
                let mut e = new_exp();
                e.min_sample = 0;
                e
            },
        ] {
            assert!(
                matches!(
                    s.register_experiment(&bad, "2026-06-01T00:00:00+00:00"),
                    Err(StoreError::Validation(_))
                ),
                "应拒绝越界参数: alpha={} power={} mde={} min_sample={}",
                bad.alpha,
                bad.power,
                bad.mde,
                bad.min_sample
            );
        }
    }

    #[test]
    fn register_rejects_bad_cohort() {
        let (_d, s) = tmp_store();
        let mut e = new_exp();
        e.canary_cohort_lo = 60;
        e.canary_cohort_hi = 50;
        assert!(matches!(
            s.register_experiment(&e, "2026-06-01T00:00:00+00:00"),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn enroll_freezes_first_assignment() {
        let (_d, s) = tmp_store();
        s.register_experiment(&new_exp(), "2026-06-01T00:00:00+00:00").unwrap();
        s.enroll_user_bucket("u1", "exp1", "baseline", 80, "2026-06-01T00:00:00+00:00")
            .unwrap();
        // 二次 enroll 不改写（即便 arm 变了）
        s.enroll_user_bucket("u1", "exp1", "canary", 10, "2026-06-05T00:00:00+00:00")
            .unwrap();
        let m = s.experiment_raw_metrics("exp1").unwrap();
        assert_eq!(m.baseline.enrolled, 1);
        assert_eq!(m.canary.enrolled, 0);
    }

    #[test]
    fn raw_metrics_empty_is_zero() {
        let (_d, s) = tmp_store();
        s.register_experiment(&new_exp(), "2026-06-01T00:00:00+00:00").unwrap();
        let m = s.experiment_raw_metrics("exp1").unwrap();
        assert_eq!(m.canary.enrolled, 0);
        assert_eq!(m.baseline.day7_retention.n, 0);
        assert_eq!(m.canary.reviews_per_day.n, 0);
    }

    #[test]
    fn mean_and_var_matches_known() {
        let r = mean_and_var(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        assert!((r.mean - 5.0).abs() < 1e-9);
        // 样本方差(n-1)=32/7≈4.571
        assert!((r.var - 32.0 / 7.0).abs() < 1e-9);
        assert_eq!(r.n, 8);
        assert_eq!(mean_and_var(&[]).n, 0);
    }
}
