//! AMAS 指标看板的真实聚合（对齐 admin 设计稿 amas-metrics.html 全部面板）。
//!
//! 数据源全部为线上真实表，不造数：
//!   - `engine_monitoring_events`：决策事件（采样后）—— 疲劳时序 / 决策直方图 / 异常列表 / 版本对比
//!   - `engine_user_states`：每用户真实累计 total_event_count（**未采样**）—— 阶段分布 / 状态流转
//!   - `learning_records`：每答一题一行（**未采样**）—— 状态流转 recent / 学习风格聚类特征
//!   - `user_elo` + `user_elo_history`：ELO 散点 + 7d Δ 着色
//!   - `word_learning_states` × `words.difficulty`：MDM 遗忘热图（按难度段，无词频 rank 时的真实近似）
//!
//! 阶段阈值（与设计稿一致）：cold `< 15`，transition `[15, 200)`，stable `>= 200`。

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

/// 阶段阈值常量，前后端口径统一。
const COLD_MAX: i64 = 15;
const STABLE_MIN: i64 = 200;

fn stage_of(count: i64) -> &'static str {
    if count < COLD_MAX {
        "cold"
    } else if count < STABLE_MIN {
        "transition"
    } else {
        "stable"
    }
}

// ─────────────────────────── 1. 阶段分布（cold / transition / stable） ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageStat {
    pub stage: String,
    pub users: u64,
    pub pct: f64,
    pub avg_decisions: f64,
    pub retention7d: f64,
    pub main_route: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageTrendPoint {
    pub date: String,
    pub cold: f64,
    pub transition: f64,
    pub stable: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageDistribution {
    pub total_users: u64,
    pub stages: Vec<StageStat>,
    pub trend: Vec<StageTrendPoint>,
}

impl Store {
    /// 阶段分布：按 engine_user_states.total_event_count 把用户分 cold/transition/stable，
    /// 每阶段统计人数 / 占比 / 平均决策数 / 7d 留存（last_active_at 在 7 天内）/ 主路由算法。
    /// trend 复用 monitoring_timeseries 的每日快照（period_id = `amas_stage:YYYY-MM-DD`），
    /// 并保证含当日实时点。
    pub fn aggregate_amas_stage_distribution(&self) -> Result<StageDistribution, StoreError> {
        let conn = self.conn()?;
        let cutoff7d = (Utc::now() - Duration::days(7)).to_rfc3339();

        // 每阶段：人数、平均决策、7d 留存数
        let mut stmt = conn.prepare(
            "SELECT
                CASE WHEN total_event_count < ?1 THEN 'cold'
                     WHEN total_event_count < ?2 THEN 'transition'
                     ELSE 'stable' END AS stage,
                COUNT(*),
                COALESCE(AVG(total_event_count), 0.0),
                COALESCE(SUM(CASE WHEN last_active_at >= ?3 THEN 1 ELSE 0 END), 0)
             FROM engine_user_states
             GROUP BY stage",
        )?;
        let mut rows: std::collections::HashMap<String, (u64, f64, u64)> = Default::default();
        let mapped = stmt.query_map(rusqlite::params![COLD_MAX, STABLE_MIN, cutoff7d], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, f64>(2)?,
                row.get::<_, i64>(3)? as u64,
            ))
        })?;
        for r in mapped {
            let (stage, users, avg, retained) = r?;
            rows.insert(stage, (users, avg, retained));
        }

        // 各阶段主路由：events JOIN states，按 stage × routing_algo 计数取最大
        let mut route_stmt = conn.prepare(
            "SELECT
                CASE WHEN s.total_event_count < ?1 THEN 'cold'
                     WHEN s.total_event_count < ?2 THEN 'transition'
                     ELSE 'stable' END AS stage,
                e.routing_algo, COUNT(*) c
             FROM engine_monitoring_events e
             JOIN engine_user_states s ON s.user_id = e.user_id
             WHERE e.routing_algo != ''
             GROUP BY stage, e.routing_algo",
        )?;
        let mut route_best: std::collections::HashMap<String, (String, u64)> = Default::default();
        let route_rows = route_stmt.query_map(rusqlite::params![COLD_MAX, STABLE_MIN], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u64,
            ))
        })?;
        for r in route_rows {
            let (stage, algo, c) = r?;
            let e = route_best.entry(stage).or_insert((String::new(), 0));
            if c > e.1 {
                *e = (algo, c);
            }
        }
        // 释放本方法的连接，再调用 stage_trend（它会另取连接；pool=1 时嵌套会死锁）
        drop(route_stmt);
        drop(stmt);
        drop(conn);

        let total_users: u64 = rows.values().map(|(u, _, _)| *u).sum();
        let stages = ["cold", "transition", "stable"]
            .iter()
            .map(|&name| {
                let (users, avg, retained) = rows.get(name).copied().unwrap_or((0, 0.0, 0));
                StageStat {
                    stage: name.to_string(),
                    users,
                    pct: if total_users > 0 {
                        users as f64 / total_users as f64
                    } else {
                        0.0
                    },
                    avg_decisions: avg,
                    retention7d: if users > 0 {
                        retained as f64 / users as f64
                    } else {
                        0.0
                    },
                    main_route: route_best
                        .get(name)
                        .map(|(a, _)| a.clone())
                        .unwrap_or_default(),
                }
            })
            .collect();

        let trend = self.stage_trend(7)?;
        Ok(StageDistribution {
            total_users,
            stages,
            trend,
        })
    }

    /// 阶段切换 N 天趋势：读 monitoring_timeseries 的每日快照，必要时追加当日实时点。
    fn stage_trend(&self, days: u32) -> Result<Vec<StageTrendPoint>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT period_id, data_json FROM monitoring_timeseries
             WHERE period_id LIKE 'amas_stage:%'
             ORDER BY period_id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([days as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // 释放连接，再调用 stage_counts_now（pool=1 时嵌套取连接会死锁）
        drop(stmt);
        drop(conn);

        let mut points: Vec<StageTrendPoint> = Vec::new();
        for (period, json) in rows.into_iter().rev() {
            let date = period
                .strip_prefix("amas_stage:")
                .unwrap_or(&period)
                .to_string();
            let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
            let (c, t, s) = (
                v.get("cold").and_then(|x| x.as_f64()).unwrap_or(0.0),
                v.get("transition").and_then(|x| x.as_f64()).unwrap_or(0.0),
                v.get("stable").and_then(|x| x.as_f64()).unwrap_or(0.0),
            );
            points.push(to_trend_point(date, c, t, s));
        }

        // 保证含当日实时点（worker 未跑或刚启动也有数据）
        let today = Utc::now().format("%Y-%m-%d").to_string();
        if !points.iter().any(|p| p.date == today) {
            let (c, t, s) = self.stage_counts_now()?;
            points.push(to_trend_point(today, c as f64, t as f64, s as f64));
        }
        Ok(points)
    }

    /// 当前各阶段用户数（实时，供 worker 快照 / trend 兜底）。
    pub fn stage_counts_now(&self) -> Result<(u64, u64, u64), StoreError> {
        let conn = self.conn()?;
        let mut cold = 0u64;
        let mut tran = 0u64;
        let mut stable = 0u64;
        let mut stmt = conn.prepare("SELECT total_event_count FROM engine_user_states")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        for r in rows {
            match stage_of(r?) {
                "cold" => cold += 1,
                "transition" => tran += 1,
                _ => stable += 1,
            }
        }
        Ok((cold, tran, stable))
    }
}

fn to_trend_point(date: String, c: f64, t: f64, s: f64) -> StageTrendPoint {
    let total = (c + t + s).max(1.0);
    StageTrendPoint {
        date,
        cold: c / total,
        transition: t / total,
        stable: s / total,
    }
}

// ─────────────────────────── 2. ELO 散点 ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EloPoint {
    pub elo: f64,
    pub decisions: u64,
    pub delta_elo: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EloScatter {
    pub points: Vec<EloPoint>,
    pub total: u64,
    pub mean_elo: f64,
}

impl Store {
    /// ELO 散点：x=rating，y=games（答题数），color=7d Δ ELO（user_elo_history 无快照时为 0）。
    /// 取 games>0 的用户，按 games 降序最多 limit 个。
    pub fn aggregate_amas_elo_scatter(&self, limit: u32) -> Result<EloScatter, StoreError> {
        let conn = self.conn()?;
        let cutoff = (Utc::now() - Duration::days(7))
            .format("%Y-%m-%d")
            .to_string();
        let mut stmt = conn.prepare(
            "SELECT e.rating, e.games,
                (SELECT h.rating FROM user_elo_history h
                 WHERE h.user_id = e.user_id AND h.snapshot_date <= ?1
                 ORDER BY h.snapshot_date DESC LIMIT 1)
             FROM user_elo e
             WHERE e.games > 0
             ORDER BY e.games DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![cutoff, limit], |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, Option<f64>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut points = Vec::with_capacity(rows.len());
        let mut sum = 0.0;
        for (rating, games, hist) in rows {
            sum += rating;
            points.push(EloPoint {
                elo: rating,
                decisions: games,
                delta_elo: hist.map(|h| rating - h).unwrap_or(0.0),
            });
        }
        let total = points.len() as u64;
        Ok(EloScatter {
            mean_elo: if total > 0 { sum / total as f64 } else { 0.0 },
            total,
            points,
        })
    }
}

// ─────────────────────────── 3. MDM 遗忘概率热图 ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MdmHeatmap {
    /// 行标签（日期，最近 N 天，升序）
    pub days: Vec<String>,
    /// 列数（难度段数，固定 14）
    pub band_count: u32,
    /// cells[day_idx][band_idx] = 平均遗忘概率 0..1（无样本为 -1 表示空白）
    pub cells: Vec<Vec<f64>>,
    pub peak: f64,
}

impl Store {
    /// MDM 遗忘热图：word_learning_states.mastery_level 取 `forget = 1 - mastery_level`，
    /// 按 (updated_at 当天 × words.difficulty 14 段) 求平均。无词频 rank，用难度段真实近似。
    pub fn aggregate_amas_mdm_heatmap(&self, days: u32) -> Result<MdmHeatmap, StoreError> {
        const BANDS: usize = 14;
        let conn = self.conn()?;
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT substr(wls.updated_at, 1, 10) AS d, w.difficulty, wls.mastery_level
             FROM word_learning_states wls
             JOIN words w ON w.id = wls.word_id
             WHERE wls.updated_at >= ?1",
        )?;
        let rows = stmt
            .query_map([&cutoff], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // 收集日期集合（升序），按 (day, band) 累计 forget 和与计数
        let mut day_set: std::collections::BTreeSet<String> = Default::default();
        for (d, _, _) in &rows {
            day_set.insert(d.clone());
        }
        let day_list: Vec<String> = day_set.into_iter().collect();
        let day_idx: std::collections::HashMap<&str, usize> = day_list
            .iter()
            .enumerate()
            .map(|(i, d)| (d.as_str(), i))
            .collect();

        let mut sums = vec![vec![0.0f64; BANDS]; day_list.len()];
        let mut counts = vec![vec![0u64; BANDS]; day_list.len()];
        for (d, difficulty, mastery) in &rows {
            let di = match day_idx.get(d.as_str()) {
                Some(i) => *i,
                None => continue,
            };
            let band =
                ((difficulty.clamp(0.0, 1.0) * BANDS as f64).floor() as usize).min(BANDS - 1);
            let forget = (1.0 - mastery).clamp(0.0, 1.0);
            sums[di][band] += forget;
            counts[di][band] += 1;
        }

        let mut peak = 0.0f64;
        let cells: Vec<Vec<f64>> = (0..day_list.len())
            .map(|di| {
                (0..BANDS)
                    .map(|b| {
                        if counts[di][b] == 0 {
                            -1.0
                        } else {
                            let avg = sums[di][b] / counts[di][b] as f64;
                            if avg > peak {
                                peak = avg;
                            }
                            avg
                        }
                    })
                    .collect()
            })
            .collect();

        Ok(MdmHeatmap {
            days: day_list,
            band_count: BANDS as u32,
            cells,
            peak,
        })
    }
}

// ─────────────────────────── 4. 疲劳信号时序 ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FatiguePoint {
    pub date: String,
    pub avg_fatigue: f64,
    pub peak_fatigue: f64,
    pub trigger_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FatigueTimeseries {
    pub points: Vec<FatiguePoint>,
    pub avg_intensity: f64,
    pub total_triggers: u64,
    pub threshold: f64,
}

impl Store {
    /// 疲劳时序：按天聚合 user_state_fatigue 的均值 / 峰值 / 触发次数(>=阈值)。
    pub fn aggregate_amas_fatigue_timeseries(
        &self,
        days: u32,
        threshold: f64,
    ) -> Result<FatigueTimeseries, StoreError> {
        let conn = self.conn()?;
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT substr(timestamp, 1, 10) AS d,
                    AVG(user_state_fatigue),
                    MAX(user_state_fatigue),
                    SUM(CASE WHEN user_state_fatigue >= ?2 THEN 1 ELSE 0 END)
             FROM engine_monitoring_events
             WHERE timestamp >= ?1
             GROUP BY d
             ORDER BY d ASC",
        )?;
        let points = stmt
            .query_map(rusqlite::params![cutoff, threshold], |row| {
                Ok(FatiguePoint {
                    date: row.get::<_, String>(0)?,
                    avg_fatigue: row.get::<_, f64>(1)?,
                    peak_fatigue: row.get::<_, f64>(2)?,
                    trigger_count: row.get::<_, i64>(3)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let total_triggers: u64 = points.iter().map(|p| p.trigger_count).sum();
        let avg_intensity = if points.is_empty() {
            0.0
        } else {
            points.iter().map(|p| p.avg_fatigue).sum::<f64>() / points.len() as f64
        };
        Ok(FatigueTimeseries {
            points,
            avg_intensity,
            total_triggers,
            threshold,
        })
    }
}

// ─────────────────────────── 5. 每用户决策数直方图 ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionBucket {
    pub label: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionHistogram {
    pub buckets: Vec<DecisionBucket>,
    pub p50: u64,
    pub p95: u64,
    pub total_users: u64,
}

impl Store {
    /// 每用户决策数直方图：窗口内按 user_id 计 learning_records 行数，分桶 + P50/P95。
    /// 用 learning_records（未采样，每答一题一行）以贴近设计稿"每用户每日决策"语义。
    pub fn aggregate_amas_decision_histogram(
        &self,
        days: u32,
    ) -> Result<DecisionHistogram, StoreError> {
        let conn = self.conn()?;
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) c FROM learning_records
             WHERE created_at >= ?1 GROUP BY user_id",
        )?;
        let mut per_user: Vec<u64> = stmt
            .query_map([&cutoff], |row| Ok(row.get::<_, i64>(0)? as u64))?
            .collect::<Result<Vec<_>, _>>()?;

        const EDGES: [(&str, u64, u64); 7] = [
            ("1-10", 1, 10),
            ("11-30", 11, 30),
            ("31-60", 31, 60),
            ("61-100", 61, 100),
            ("101-200", 101, 200),
            ("201-400", 201, 400),
            ("400+", 401, u64::MAX),
        ];
        let mut buckets: Vec<DecisionBucket> = EDGES
            .iter()
            .map(|(l, _, _)| DecisionBucket {
                label: l.to_string(),
                count: 0,
            })
            .collect();
        for &n in &per_user {
            for (i, (_, lo, hi)) in EDGES.iter().enumerate() {
                if n >= *lo && n <= *hi {
                    buckets[i].count += 1;
                    break;
                }
            }
        }

        per_user.sort_unstable();
        let total_users = per_user.len() as u64;
        Ok(DecisionHistogram {
            buckets,
            p50: percentile_u64(&per_user, 0.50),
            p95: percentile_u64(&per_user, 0.95),
            total_users,
        })
    }
}

fn percentile_u64(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn percentile_f64(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ─────────────────────────── 6. 状态流转（窗口内阶段穿越） ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateTransitions {
    pub window_hours: u32,
    pub transitions: Vec<Transition>,
}

impl Store {
    /// 状态流转：用 engine_user_states.total_event_count（真实累计）作"现在"的决策数，
    /// learning_records 窗口内行数作"近期增量"，before = total - recent。
    /// before 阶段 → total 阶段不同即一次流转；before==0 && total>0 记为"新注册→冷启动"。
    pub fn aggregate_amas_state_transitions(
        &self,
        hours: u32,
    ) -> Result<StateTransitions, StoreError> {
        let conn = self.conn()?;
        let cutoff = (Utc::now() - Duration::hours(hours as i64)).to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT s.total_event_count,
                (SELECT COUNT(*) FROM learning_records r
                 WHERE r.user_id = s.user_id AND r.created_at >= ?1) AS recent
             FROM engine_user_states s",
        )?;
        let rows = stmt
            .query_map([&cutoff], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut counts: std::collections::HashMap<(&str, &str), u64> = Default::default();
        let mut newcomers = 0u64;
        for (total, recent) in rows {
            if recent <= 0 {
                continue; // 窗口内无活动
            }
            let before = (total - recent).max(0);
            if before == 0 && total > 0 {
                newcomers += 1;
            }
            let from = stage_of(before);
            let to = stage_of(total);
            if from != to {
                *counts.entry((from, to)).or_insert(0) += 1;
            }
        }

        // 设计稿四条主流向 + 新注册→冷
        let mut transitions = vec![
            Transition {
                from: "new".into(),
                to: "cold".into(),
                count: newcomers,
            },
            mk_transition(&counts, "cold", "transition"),
            mk_transition(&counts, "transition", "stable"),
            mk_transition(&counts, "stable", "transition"),
        ];
        // 其余非零流向也带上（例如 cold→stable 跨级）
        for ((f, t), c) in &counts {
            let known = transitions.iter().any(|x| x.from == *f && x.to == *t);
            if !known && *c > 0 {
                transitions.push(Transition {
                    from: f.to_string(),
                    to: t.to_string(),
                    count: *c,
                });
            }
        }
        Ok(StateTransitions {
            window_hours: hours,
            transitions,
        })
    }
}

fn mk_transition(
    counts: &std::collections::HashMap<(&str, &str), u64>,
    from: &str,
    to: &str,
) -> Transition {
    Transition {
        from: from.to_string(),
        to: to.to_string(),
        count: counts.get(&(from, to)).copied().unwrap_or(0),
    }
}

// ─────────────────────────── 7. 学习风格聚类（k-means k=4） ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningCluster {
    pub label: String,
    pub count: u64,
    pub pct: f64,
    pub avg_response_ms: f64,
    pub error_rate: f64,
    pub records_per_active_day: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningStyleClusters {
    pub k: u32,
    pub total_users: u64,
    pub clusters: Vec<LearningCluster>,
}

impl Store {
    /// 学习风格 K-Means(k=4)：每用户三维真实特征（平均反应时 / 错误率 / 每活跃日答题数），
    /// 标准化后跑确定性 k-means（按特征分位选初始中心，无随机），按中心特征贴语义标签。
    pub fn aggregate_amas_learning_clusters(&self) -> Result<LearningStyleClusters, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT AVG(response_time_ms), AVG(is_correct), COUNT(*),
                    COUNT(DISTINCT substr(created_at, 1, 10))
             FROM learning_records
             GROUP BY user_id
             HAVING COUNT(*) >= 5",
        )?;
        let feats = stmt
            .query_map([], |row| {
                let resp = row.get::<_, f64>(0)?;
                let acc = row.get::<_, f64>(1)?;
                let cnt = row.get::<_, i64>(2)? as f64;
                let days = row.get::<_, i64>(3)?.max(1) as f64;
                Ok([resp, 1.0 - acc, cnt / days])
            })?
            .collect::<Result<Vec<[f64; 3]>, _>>()?;

        let total_users = feats.len() as u64;
        if total_users < 4 {
            return Ok(LearningStyleClusters {
                k: 4,
                total_users,
                clusters: Vec::new(),
            });
        }

        let (assign, centroids_z, means, stds) = kmeans4(&feats);
        // 还原中心到原尺度 + 统计每簇规模
        let mut agg: Vec<(u64, [f64; 3])> = vec![(0, [0.0; 3]); 4];
        for (i, &c) in assign.iter().enumerate() {
            agg[c].0 += 1;
            for d in 0..3 {
                agg[c].1[d] += feats[i][d];
            }
        }
        let labels = label_clusters(&centroids_z);
        let clusters = (0..4)
            .map(|c| {
                let (n, sums) = agg[c];
                let denom = n.max(1) as f64;
                LearningCluster {
                    label: labels[c].to_string(),
                    count: n,
                    pct: n as f64 / total_users as f64,
                    avg_response_ms: sums[0] / denom,
                    error_rate: sums[1] / denom,
                    records_per_active_day: sums[2] / denom,
                }
            })
            .collect();
        // means/stds 仅内部诊断用途，避免未使用告警
        let _ = (means, stds);
        Ok(LearningStyleClusters {
            k: 4,
            total_users,
            clusters,
        })
    }
}

/// 确定性 k-means（k=4，三维）：z-score 标准化 → 按第 0 维分位选初始中心 → 12 轮迭代。
/// 返回 (每点簇号, z-空间中心[4][3], 各维均值, 各维标准差)。
fn kmeans4(feats: &[[f64; 3]]) -> (Vec<usize>, [[f64; 3]; 4], [f64; 3], [f64; 3]) {
    let n = feats.len();
    let mut means = [0.0; 3];
    for f in feats {
        for d in 0..3 {
            means[d] += f[d];
        }
    }
    for d in 0..3 {
        means[d] /= n as f64;
    }
    let mut stds = [0.0; 3];
    for f in feats {
        for d in 0..3 {
            let dx = f[d] - means[d];
            stds[d] += dx * dx;
        }
    }
    for d in 0..3 {
        stds[d] = (stds[d] / n as f64).sqrt().max(1e-9);
    }
    let z: Vec<[f64; 3]> = feats
        .iter()
        .map(|f| {
            let mut o = [0.0; 3];
            for d in 0..3 {
                o[d] = (f[d] - means[d]) / stds[d];
            }
            o
        })
        .collect();

    // 确定性初始中心：按"综合得分"排序取 4 个分位点
    let mut order: Vec<usize> = (0..n).collect();
    let score = |p: &[f64; 3]| p[0] + p[1] + p[2];
    order.sort_by(|&a, &b| {
        score(&z[a])
            .partial_cmp(&score(&z[b]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut centroids = [[0.0; 3]; 4];
    for c in 0..4 {
        let pick = order[(n - 1) * c / 3];
        centroids[c] = z[pick];
    }

    let mut assign = vec![0usize; n];
    for _ in 0..12 {
        // 分配
        for (i, p) in z.iter().enumerate() {
            let mut best = 0usize;
            let mut best_d = f64::MAX;
            for (c, cen) in centroids.iter().enumerate() {
                let dist = (0..3).map(|d| (p[d] - cen[d]).powi(2)).sum::<f64>();
                if dist < best_d {
                    best_d = dist;
                    best = c;
                }
            }
            assign[i] = best;
        }
        // 更新
        let mut sums = [[0.0; 3]; 4];
        let mut cnts = [0u64; 4];
        for (i, p) in z.iter().enumerate() {
            let c = assign[i];
            cnts[c] += 1;
            for d in 0..3 {
                sums[c][d] += p[d];
            }
        }
        for c in 0..4 {
            if cnts[c] > 0 {
                for d in 0..3 {
                    centroids[c][d] = sums[c][d] / cnts[c] as f64;
                }
            }
        }
    }
    (assign, centroids, means, stds)
}

/// 按 z-空间中心特征给 4 个簇贴语义标签。维度：[0]=反应时, [1]=错误率, [2]=每日答题频次。
/// 短时高频型：低反应时 + 高频次；长时稳定型：低错误率 + 中频；间歇突击型：高频次但非低反应；探索游荡型：高错误率。
fn label_clusters(centroids: &[[f64; 3]; 4]) -> [&'static str; 4] {
    let mut labels = ["", "", "", ""];
    let mut used = [false; 4];

    // 探索游荡型 = 错误率最高
    let explorer = (0..4)
        .max_by(|&a, &b| centroids[a][1].partial_cmp(&centroids[b][1]).unwrap())
        .unwrap();
    labels[explorer] = "探索游荡型";
    used[explorer] = true;

    // 短时高频型 = 频次最高且反应时偏低（综合 freq - resp 最大）
    let burst_score = |c: usize| centroids[c][2] - centroids[c][0];
    let high_freq = (0..4)
        .filter(|c| !used[*c])
        .max_by(|&a, &b| burst_score(a).partial_cmp(&burst_score(b)).unwrap())
        .unwrap();
    labels[high_freq] = "短时高频型";
    used[high_freq] = true;

    // 长时稳定型 = 剩余中错误率最低
    let stable = (0..4)
        .filter(|c| !used[*c])
        .min_by(|&a, &b| centroids[a][1].partial_cmp(&centroids[b][1]).unwrap())
        .unwrap();
    labels[stable] = "长时稳定型";
    used[stable] = true;

    // 间歇突击型 = 最后一个
    for c in 0..4 {
        if !used[c] {
            labels[c] = "间歇突击型";
        }
    }
    labels
}

// ─────────────────────────── 8. 异常分级 + 逐条列表 ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnomalyItem {
    pub id: String,
    pub timestamp: String,
    pub severity: String, // error | warn | info
    pub code: String,
    pub title: String,
    pub user_id: String,
    pub value: f64,
    pub expected_range: String,
    /// 同 code 异常波及的去重用户数（影响面）
    pub impacted_users: u64,
    /// 影响面占比 = impacted_users / 窗口内去重活跃用户
    pub impact_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnomalySeveritySummary {
    pub error: u64,
    pub warn: u64,
    pub info: u64,
    pub invariant_pass: u64,
    pub invariant_total: u64,
    pub pass_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnomalyFeed {
    pub summary: AnomalySeveritySummary,
    pub items: Vec<AnomalyItem>,
}

/// 不变量违反字段 → 严重度。NaN / batch_size → error；越界的核心状态/策略 → warn；其余 → info。
fn severity_of(field: &str, value: f64) -> &'static str {
    if value.is_nan() || field == "batch_size" {
        "error"
    } else if matches!(
        field,
        "attention" | "fatigue" | "confidence" | "motivation" | "difficulty" | "new_ratio"
    ) {
        "warn"
    } else {
        "info"
    }
}

impl Store {
    /// 异常 feed：窗口内 is_anomaly=1 的事件解析首条违反 → 严重度 / code / 描述；
    /// summary 给各严重度计数 + 不变量通过率(=非异常事件/总事件)。
    pub fn aggregate_amas_anomaly_feed(
        &self,
        days: u32,
        limit: u32,
    ) -> Result<AnomalyFeed, StoreError> {
        let conn = self.conn()?;
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();

        // 总事件 / 异常事件计数（通过率）+ 窗口去重活跃用户（影响面分母）
        let (total, anomalies): (u64, u64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(is_anomaly), 0)
             FROM engine_monitoring_events WHERE timestamp >= ?1",
            [&cutoff],
            |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64)),
        )?;
        let distinct_users: u64 = conn.query_row(
            "SELECT COUNT(DISTINCT user_id) FROM engine_monitoring_events WHERE timestamp >= ?1",
            [&cutoff],
            |row| row.get::<_, i64>(0).map(|v| v as u64),
        )?;

        // 异常明细（取近 limit*4 条用于严重度汇总，再切 limit 条做列表）
        let scan = (limit.max(20) * 4).min(2000);
        let mut stmt = conn.prepare(
            "SELECT id, user_id, timestamp, invariant_violations_json
             FROM engine_monitoring_events
             WHERE is_anomaly = 1 AND timestamp >= ?1
             ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![cutoff, scan], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut summary = AnomalySeveritySummary {
            invariant_total: total,
            invariant_pass: total.saturating_sub(anomalies),
            pass_rate: if total > 0 {
                (total - anomalies.min(total)) as f64 / total as f64
            } else {
                1.0
            },
            ..Default::default()
        };
        // 先解析全部扫描行，统计 code → 去重用户集合（影响面），再切 limit 条做列表
        struct Parsed {
            id: String,
            user_id: String,
            ts: String,
            field: String,
            value: f64,
            expected: String,
            sev: &'static str,
        }
        let mut code_users: std::collections::HashMap<String, std::collections::HashSet<String>> =
            Default::default();
        let mut parsed: Vec<Parsed> = Vec::with_capacity(rows.len());
        for (id, user_id, ts, viol_json) in rows {
            let arr: Vec<serde_json::Value> = serde_json::from_str(&viol_json).unwrap_or_default();
            let first = arr.first();
            let field = first
                .and_then(|v| v.get("field"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let value = first
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_f64())
                .unwrap_or(f64::NAN);
            let expected = first
                .and_then(|v| v.get("expected_range"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let sev = severity_of(&field, value);
            match sev {
                "error" => summary.error += 1,
                "warn" => summary.warn += 1,
                _ => summary.info += 1,
            }
            code_users
                .entry(field.clone())
                .or_default()
                .insert(user_id.clone());
            parsed.push(Parsed {
                id,
                user_id,
                ts,
                field,
                value,
                expected,
                sev,
            });
        }

        let items = parsed
            .into_iter()
            .take(limit as usize)
            .map(|p| {
                let impacted = code_users
                    .get(&p.field)
                    .map(|s| s.len() as u64)
                    .unwrap_or(1);
                let val_disp = if p.value.is_nan() {
                    "NaN".to_string()
                } else {
                    format!("{:.3}", p.value)
                };
                AnomalyItem {
                    timestamp: p.ts,
                    severity: p.sev.to_string(),
                    title: format!(
                        "不变量违反：{} = {val_disp}（期望 {}）",
                        p.field,
                        if p.expected.is_empty() {
                            "合法区间"
                        } else {
                            &p.expected
                        }
                    ),
                    code: p.field,
                    user_id: p.user_id,
                    value: if p.value.is_nan() { 0.0 } else { p.value },
                    expected_range: p.expected,
                    impacted_users: impacted,
                    impact_pct: if distinct_users > 0 {
                        impacted as f64 / distinct_users as f64
                    } else {
                        0.0
                    },
                    id: p.id,
                }
            })
            .collect();
        Ok(AnomalyFeed { summary, items })
    }
}

// ─────────────────────────── 9. 版本对比扩展 ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionSliceExt {
    pub version_hash: String,
    pub event_count: u64,
    pub hit_rate: f64,
    pub p95_latency_ms: f64,
    pub mean_latency_ms: f64,
    pub fatigue_rate: f64,
    pub ensemble_share: f64,
    pub mean_reward: f64,
    pub anomaly_rate: f64,
    /// 7d 新用户留存：版本活跃窗口内，前段活跃用户在末 7 天回归的比例
    pub retention7d: f64,
    /// P95 答题完成时间(ms)：版本窗口内 learning_records.response_time_ms 的 95 分位
    pub p95_completion_ms: f64,
    /// 窗口内逐日命中率，供 sparkline
    pub spark: Vec<f64>,
    /// 由 handler 读 config snapshot 填入：真实可调参 memory_model.half_life_base_epsilon
    pub config_epsilon: Option<f64>,
    pub first_event_at: Option<String>,
    pub last_event_at: Option<String>,
}

impl Store {
    /// 单版本扩展指标切片：命中率 / P95 延迟 / 疲劳率 / ensemble 占比 / reward / 异常率 + 逐日命中率 sparkline。
    pub fn aggregate_amas_version_slice_ext(
        &self,
        version_hash: &str,
        fatigue_threshold: f64,
        fallback_epsilon: f64,
    ) -> Result<VersionSliceExt, StoreError> {
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(is_correct), 0),
                COALESCE(AVG(latency_ms), 0.0),
                COALESCE(SUM(CASE WHEN user_state_fatigue >= ?2 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN routing_algo = 'ensemble' THEN 1 ELSE 0 END), 0),
                COALESCE(AVG(reward_value), 0.0),
                COALESCE(SUM(is_anomaly), 0),
                MIN(timestamp), MAX(timestamp)
             FROM engine_monitoring_events
             WHERE config_version = ?1",
            rusqlite::params![version_hash, fatigue_threshold],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? as u64,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)? as u64,
                    row.get::<_, f64>(5)?,
                    row.get::<_, i64>(6)? as u64,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )?;
        let (n, hits, mean_lat, fatigue_n, ensemble_n, mean_reward, anom_n, first, last) = row;
        let rate = |x: u64| if n > 0 { x as f64 / n as f64 } else { 0.0 };

        // P95 延迟（Rust 端百分位）
        let mut lats: Vec<f64> = conn
            .prepare("SELECT latency_ms FROM engine_monitoring_events WHERE config_version = ?1")?
            .query_map([version_hash], |row| row.get::<_, i64>(0).map(|v| v as f64))?
            .collect::<Result<Vec<_>, _>>()?;
        lats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // config_epsilon：按该版本从 amas_config_versions.snapshot_json 解析 half_life_base_epsilon，
        // 版本无快照或字段缺失时回退到 live config 的 epsilon。
        let config_epsilon = conn
            .query_row(
                "SELECT snapshot_json FROM amas_config_versions WHERE version_hash = ?1",
                [version_hash],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["memory_model"]["half_life_base_epsilon"].as_f64())
            .unwrap_or(fallback_epsilon);

        // 逐日命中率 sparkline
        let spark: Vec<f64> = conn
            .prepare(
                "SELECT CASE WHEN COUNT(*)>0 THEN CAST(SUM(is_correct) AS REAL)/COUNT(*) ELSE 0 END
                 FROM engine_monitoring_events WHERE config_version = ?1
                 GROUP BY substr(timestamp,1,10) ORDER BY substr(timestamp,1,10) ASC",
            )?
            .query_map([version_hash], |row| row.get::<_, f64>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // P95 答题完成时间 + 7d 留存：在版本活跃窗口 [first, last] 内按 learning_records 真实聚合。
        let (mut retention7d, mut p95_completion_ms) = (0.0, 0.0);
        if let (Some(f), Some(l)) = (&first, &last) {
            // P95 答题完成时间（response_time_ms）
            let mut rts: Vec<f64> = conn
                .prepare(
                    "SELECT response_time_ms FROM learning_records
                     WHERE created_at BETWEEN ?1 AND ?2 AND response_time_ms > 0",
                )?
                .query_map([f, l], |row| row.get::<_, i64>(0).map(|v| v as f64))?
                .collect::<Result<Vec<_>, _>>()?;
            rts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            p95_completion_ms = percentile_f64(&rts, 0.95);

            // 7d 留存：窗口前段(到 last-7d)活跃用户中，在末 7 天仍回归的比例
            let mid = chrono::DateTime::parse_from_rfc3339(l)
                .map(|d| (d.with_timezone(&Utc) - Duration::days(7)).to_rfc3339())
                .unwrap_or_else(|_| f.clone());
            let base: i64 = conn.query_row(
                "SELECT COUNT(DISTINCT user_id) FROM learning_records
                 WHERE created_at BETWEEN ?1 AND ?2",
                [f, &mid],
                |row| row.get(0),
            )?;
            let returning: i64 = conn.query_row(
                "SELECT COUNT(DISTINCT user_id) FROM learning_records
                 WHERE created_at BETWEEN ?1 AND ?2
                   AND user_id IN (SELECT user_id FROM learning_records WHERE created_at BETWEEN ?3 AND ?4)",
                [&mid, l, f, &mid],
                |row| row.get(0),
            )?;
            if base > 0 {
                retention7d = returning as f64 / base as f64;
            }
        }

        Ok(VersionSliceExt {
            version_hash: version_hash.to_string(),
            event_count: n,
            hit_rate: rate(hits),
            p95_latency_ms: percentile_f64(&lats, 0.95),
            mean_latency_ms: mean_lat,
            fatigue_rate: rate(fatigue_n),
            ensemble_share: rate(ensemble_n),
            mean_reward,
            anomaly_rate: rate(anom_n),
            retention7d,
            p95_completion_ms,
            spark,
            config_epsilon: Some(config_epsilon),
            first_event_at: first,
            last_event_at: last,
        })
    }
}

// ─────────────────────────── worker 快照写入 ───────────────────────────

impl Store {
    /// daily_aggregation worker 调用：把当前 user_elo 快照进 user_elo_history（按日幂等覆盖）。
    /// 供 ELO 散点的 7d Δ 着色随时间积累历史。
    pub fn snapshot_user_elo_daily(&self, date: &str) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "INSERT OR REPLACE INTO user_elo_history (user_id, snapshot_date, rating, games)
             SELECT user_id, ?1, rating, games FROM user_elo WHERE games > 0",
            [date],
        )?;
        Ok(n as u64)
    }

    /// daily_aggregation worker 调用：把当前阶段分布快照进 monitoring_timeseries（趋势线数据源）。
    pub fn snapshot_amas_stage_daily(&self, date: &str) -> Result<(), StoreError> {
        let (cold, transition, stable) = self.stage_counts_now()?;
        let data = serde_json::json!({ "cold": cold, "transition": transition, "stable": stable });
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO monitoring_timeseries (period_id, data_json)
             VALUES (?1, ?2)",
            rusqlite::params![format!("amas_stage:{date}"), data.to_string()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn fresh() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    fn seed_user_state(store: &Store, uid: &str, total: i64, active: bool) {
        let conn = store.conn().unwrap();
        let last = if active {
            Utc::now().to_rfc3339()
        } else {
            (Utc::now() - Duration::days(30)).to_rfc3339()
        };
        conn.execute(
            "INSERT INTO engine_user_states (user_id, total_event_count, last_active_at, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![uid, total, last, Utc::now().to_rfc3339()],
        )
        .unwrap();
    }

    #[test]
    fn stage_distribution_classifies_by_threshold() {
        let store = fresh();
        seed_user_state(&store, "u1", 5, true); // cold
        seed_user_state(&store, "u2", 50, true); // transition
        seed_user_state(&store, "u3", 300, false); // stable, not retained
        let d = store.aggregate_amas_stage_distribution().unwrap();
        assert_eq!(d.total_users, 3);
        let cold = d.stages.iter().find(|s| s.stage == "cold").unwrap();
        let stable = d.stages.iter().find(|s| s.stage == "stable").unwrap();
        assert_eq!(cold.users, 1);
        assert_eq!(stable.users, 1);
        assert!((stable.retention7d - 0.0).abs() < 1e-9);
        assert!(!d.trend.is_empty()); // 含当日实时点
    }

    #[test]
    fn elo_scatter_computes_delta_from_history() {
        let store = fresh();
        {
            let conn = store.conn().unwrap();
            conn.execute(
                "INSERT INTO user_elo (user_id, rating, games) VALUES ('u1', 1300.0, 40)",
                [],
            )
            .unwrap();
            let week_ago = (Utc::now() - Duration::days(8))
                .format("%Y-%m-%d")
                .to_string();
            conn.execute(
                "INSERT INTO user_elo_history (user_id, snapshot_date, rating, games)
                 VALUES ('u1', ?1, 1250.0, 20)",
                params![week_ago],
            )
            .unwrap();
        }
        let s = store.aggregate_amas_elo_scatter(100).unwrap();
        assert_eq!(s.total, 1);
        assert!((s.points[0].delta_elo - 50.0).abs() < 1e-9);
    }

    #[test]
    fn decision_histogram_buckets_and_percentiles() {
        let store = fresh();
        {
            let conn = store.conn().unwrap();
            let now = Utc::now().to_rfc3339();
            // u1: 5 条 (1-10), u2: 20 条 (11-30)
            for i in 0..5 {
                conn.execute(
                    "INSERT INTO learning_records (user_id,id,word_id,is_correct,created_at)
                     VALUES ('u1',?1,'w',1,?2)",
                    params![format!("a{i}"), now],
                )
                .unwrap();
            }
            for i in 0..20 {
                conn.execute(
                    "INSERT INTO learning_records (user_id,id,word_id,is_correct,created_at)
                     VALUES ('u2',?1,'w',1,?2)",
                    params![format!("b{i}"), now],
                )
                .unwrap();
            }
        }
        let h = store.aggregate_amas_decision_histogram(7).unwrap();
        assert_eq!(h.total_users, 2);
        assert_eq!(h.buckets[0].count, 1); // 1-10
        assert_eq!(h.buckets[1].count, 1); // 11-30
    }

    #[test]
    fn anomaly_feed_classifies_severity() {
        let store = fresh();
        {
            let conn = store.conn().unwrap();
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO engine_monitoring_events
                 (id,user_id,session_id,timestamp,is_anomaly,invariant_violations_json)
                 VALUES ('e1','u1','s1',?1,1,?2)",
                params![
                    now,
                    r#"[{"field":"fatigue","value":1.2,"expected_range":"[0, 1]"}]"#
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO engine_monitoring_events
                 (id,user_id,session_id,timestamp,is_anomaly,invariant_violations_json)
                 VALUES ('e2','u2','s1',?1,0,'[]')",
                params![now],
            )
            .unwrap();
        }
        let feed = store.aggregate_amas_anomaly_feed(7, 50).unwrap();
        assert_eq!(feed.summary.warn, 1);
        assert_eq!(feed.summary.invariant_total, 2);
        assert_eq!(feed.summary.invariant_pass, 1);
        assert_eq!(feed.items.len(), 1);
        assert_eq!(feed.items[0].severity, "warn");
    }
}
