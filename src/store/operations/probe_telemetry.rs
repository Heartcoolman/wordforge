//! 数据探针看板（probe-telemetry）的 Store 操作。
//!
//! 诚实约束(详见路由层 doc):
//!   - 看板 4 个"派生探针"(click/lesson_start/word_answer/error_js)不是 telemetry
//!     event_type,而是对多源(telemetry_summaries / learning_sessions /
//!     learning_records)派生的观测指标,各自用真实源聚合。
//!   - 采样真实作用在 telemetry_events 的真实 event_type 上(probe_sampling_config)。
//!   - 不实现任何 retention / DELETE,sinks 的 retentionDays 一律 null。

use rusqlite::params;
use serde::Serialize;

use crate::store::{Store, StoreError};

/// 单条采样规则行(对应 probe_sampling_config 一行)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingRule {
    pub event_type: String,
    pub sample_rate: f64,
    pub enabled: bool,
    pub locked: bool,
    pub priority: i64,
}

/// 派生探针的真实聚合结果(24h count / lastTs)。
#[derive(Debug, Clone, Default)]
pub struct ProbeStat {
    pub count24h: i64,
    pub last_ts: Option<String>,
}

/// 单个 sink(真实 SQLite 表)的状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SinkStatus {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub row_count: i64,
    pub last_write_ts: Option<String>,
    /// 不实现 retention 清理 → 恒 null(前端显示"永久/不限")。
    pub retention_days: Option<i64>,
    pub lag_secs: i64,
}

/// stream 端点单条事件(已 humanize:设备摘要 + 关键指标 + 原始 payload)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEvent {
    pub id: String,
    pub ts: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub device_id: String,
    /// 从 payload.device 解析出的设备摘要(无 device 字段 → None)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<StreamDevice>,
    /// 顶层 + behavior.* 的标量字段(最多 8 条),供前端贴中文标签。
    pub metrics: Vec<StreamMetric>,
    /// 完整原始 payload(供"原始 JSON"展开;超长截断到 4000 字符)。
    pub payload_raw: String,
}

/// payload.device 的人话摘要(字段缺失即 None,前端跳过不渲染)。
/// 兼容两种 payload:端侧 osName/osVersion 分离、admin 端 osName 已含版本。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDevice {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// payload 里的一个标量指标(key 原样保留,前端映射中文标签)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMetric {
    pub key: String,
    pub value: String,
}

/// audit 端点单行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingAuditRow {
    pub ts: String,
    pub action: String,
    pub event_type: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub admin_id: Option<String>,
}

/// overview KPI 聚合所需原始计数。
#[derive(Debug, Clone, Default)]
pub struct OverviewRaw {
    /// 24h 内 learning_records + telemetry_events 总量。
    pub events_cur: i64,
    /// 前一个 24h 窗口的同口径总量(算 deltaPct)。
    pub events_prev: i64,
    /// 队列积压:probe_executions WHERE completed_at IS NULL。
    pub queue_backlog: i64,
    /// 24h 内"有错误的事件数"(分子,= telemetry_summaries 中 error_count>0 的行数)。
    pub error_events: i64,
    /// 24h 总遥测事件数(分母,= telemetry_events 24h count)。
    pub telemetry_total: i64,
}

impl Store {
    // ---------------------------------------------------------------------
    // 派生探针:4 个观测指标的 24h 真实聚合
    // ---------------------------------------------------------------------

    /// click 探针:telemetry_summaries WHERE click_count IS NOT NULL,窗口 window_days 天。
    pub fn probe_stat_click(&self, window_days: i64) -> Result<ProbeStat, StoreError> {
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT COUNT(*), MAX(server_ts) FROM telemetry_summaries
             WHERE click_count IS NOT NULL
               AND datetime(server_ts) > datetime('now', ?1)",
            params![window_offset(window_days)],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)),
        )?;
        Ok(ProbeStat {
            count24h: row.0,
            last_ts: row.1,
        })
    }

    /// lesson_start 探针:learning_sessions,窗口 window_days 天。
    pub fn probe_stat_lesson_start(&self, window_days: i64) -> Result<ProbeStat, StoreError> {
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT COUNT(*), MAX(created_at) FROM learning_sessions
             WHERE datetime(created_at) > datetime('now', ?1)",
            params![window_offset(window_days)],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)),
        )?;
        Ok(ProbeStat {
            count24h: row.0,
            last_ts: row.1,
        })
    }

    /// word_answer 探针:learning_records,窗口 window_days 天。
    pub fn probe_stat_word_answer(&self, window_days: i64) -> Result<ProbeStat, StoreError> {
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT COUNT(*), MAX(created_at) FROM learning_records
             WHERE datetime(created_at) > datetime('now', ?1)",
            params![window_offset(window_days)],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)),
        )?;
        Ok(ProbeStat {
            count24h: row.0,
            last_ts: row.1,
        })
    }

    /// error_js 探针:SUM(error_count) FROM telemetry_summaries,窗口 window_days 天。
    /// count24h = 错误总数(SUM);lastTs = 有错误的最近一行 server_ts。
    pub fn probe_stat_error_js(&self, window_days: i64) -> Result<ProbeStat, StoreError> {
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT COALESCE(SUM(error_count), 0),
                    MAX(CASE WHEN error_count > 0 THEN server_ts END)
             FROM telemetry_summaries
             WHERE datetime(server_ts) > datetime('now', ?1)",
            params![window_offset(window_days)],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)),
        )?;
        Ok(ProbeStat {
            count24h: row.0,
            last_ts: row.1,
        })
    }

    // ---------------------------------------------------------------------
    // overview KPI
    // ---------------------------------------------------------------------

    pub fn overview_kpis(&self, window_days: i64) -> Result<OverviewRaw, StoreError> {
        let conn = self.conn()?;
        // 当前窗口偏移 `-N day` 与前一窗口起点 `-2N day`(两窗口等长,分界点归前窗口)。
        let cur_off = window_offset(window_days);
        let prev_off = window_offset(window_days * 2);

        // 当前窗口:learning_records + telemetry_events
        let lr_cur: i64 = conn.query_row(
            "SELECT COUNT(*) FROM learning_records
             WHERE datetime(created_at) > datetime('now', ?1)",
            params![cur_off],
            |r| r.get(0),
        )?;
        let te_cur: i64 = conn.query_row(
            "SELECT COUNT(*) FROM telemetry_events
             WHERE datetime(server_ts) > datetime('now', ?1)",
            params![cur_off],
            |r| r.get(0),
        )?;

        // 前一个等长窗口 (now-2N, now-N]
        let lr_prev: i64 = conn.query_row(
            "SELECT COUNT(*) FROM learning_records
             WHERE datetime(created_at) > datetime('now', ?1)
               AND datetime(created_at) <= datetime('now', ?2)",
            params![prev_off, cur_off],
            |r| r.get(0),
        )?;
        let te_prev: i64 = conn.query_row(
            "SELECT COUNT(*) FROM telemetry_events
             WHERE datetime(server_ts) > datetime('now', ?1)
               AND datetime(server_ts) <= datetime('now', ?2)",
            params![prev_off, cur_off],
            |r| r.get(0),
        )?;

        // 队列积压:未完成且未终态的 probe_executions。
        // 排除终态行:仅统计 pending / confirm_pending,避免离线行(completed_at 恒 NULL)
        // 永久污染积压指标。
        let queue_backlog: i64 = conn.query_row(
            "SELECT COUNT(*) FROM probe_executions
             WHERE completed_at IS NULL
               AND status IN ('pending', 'confirm_pending')",
            [],
            |r| r.get(0),
        )?;

        // 采集错误率分子:当前窗口内"有错误的事件数"(error_count>0 的行计数),
        // 而非 SUM(error_count)。这样分子 ≤ 分母,前端 *100 不会超过 100%。
        let error_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM telemetry_summaries
             WHERE error_count > 0
               AND datetime(server_ts) > datetime('now', ?1)",
            params![cur_off],
            |r| r.get(0),
        )?;

        Ok(OverviewRaw {
            events_cur: lr_cur + te_cur,
            events_prev: lr_prev + te_prev,
            queue_backlog,
            error_events,
            telemetry_total: te_cur,
        })
    }

    // ---------------------------------------------------------------------
    // 采样规则 CRUD
    // ---------------------------------------------------------------------

    /// 全局默认采样率(system_settings.telemetry_sample_rate)。
    pub fn global_sample_rate(&self) -> Result<f64, StoreError> {
        let conn = self.conn()?;
        let rate: f64 = conn
            .query_row(
                "SELECT telemetry_sample_rate FROM system_settings WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1.0);
        Ok(rate)
    }

    /// 列出全部采样规则,按 priority 升序(优先级高在前)。
    pub fn list_sampling_rules(&self) -> Result<Vec<SamplingRule>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT event_type, sample_rate, enabled, locked, priority
             FROM probe_sampling_config
             ORDER BY priority ASC, event_type ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(SamplingRule {
                    event_type: r.get::<_, String>(0)?,
                    sample_rate: r.get::<_, f64>(1)?,
                    enabled: r.get::<_, i64>(2)? != 0,
                    locked: r.get::<_, i64>(3)? != 0,
                    priority: r.get::<_, i64>(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 取单条规则(不存在返回 None)。
    pub fn get_sampling_rule(&self, event_type: &str) -> Result<Option<SamplingRule>, StoreError> {
        let conn = self.conn()?;
        let rule = conn
            .query_row(
                "SELECT event_type, sample_rate, enabled, locked, priority
                 FROM probe_sampling_config WHERE event_type = ?1",
                params![event_type],
                |r| {
                    Ok(SamplingRule {
                        event_type: r.get::<_, String>(0)?,
                        sample_rate: r.get::<_, f64>(1)?,
                        enabled: r.get::<_, i64>(2)? != 0,
                        locked: r.get::<_, i64>(3)? != 0,
                        priority: r.get::<_, i64>(4)?,
                    })
                },
            )
            .ok();
        Ok(rule)
    }

    /// upsert 采样规则 + 写 audit。返回更新后的规则。
    ///
    /// 语义:
    ///   - 行不存在:
    ///     * `require_exists=true` → 返回 `StoreError::Validation("SAMPLING_RULE_NOT_FOUND")`
    ///       (路由层翻译成 404),禁止凭 PATCH 创建新行。
    ///     * `require_exists=false` → 视为新增('add' audit),priority 默认 100、locked=0。
    ///   - 行已存在且 locked=1 且本次试图改 sample_rate(与现值不同)**或** enabled(与现值不同)
    ///     → 返回 `StoreError::Validation("SAMPLING_RULE_LOCKED")`(方案A:locked 行禁止任何
    ///     enabled/rate 变更,路由层翻译成 409)。
    ///   - 否则更新('mod' audit;若仅 enabled 0→1/1→0 记 'pause')。
    ///
    /// `new_rate` / `new_enabled` 任一为 None 表示该字段保持不变。
    ///
    /// 整个 SELECT + 校验 + INSERT/UPDATE 收进单条 `BEGIN IMMEDIATE` 事务(`with_user_tx`),
    /// BEGIN 即取写锁,消除 DEFERRED 读后写的升级死锁 / 丢更新窗口。
    pub fn upsert_sampling_rule(
        &self,
        event_type: &str,
        new_rate: Option<f64>,
        new_enabled: Option<bool>,
        admin_id: Option<&str>,
        require_exists: bool,
    ) -> Result<SamplingRule, StoreError> {
        // rate 范围校验(无须持锁,先做)。
        if let Some(rate) = new_rate {
            if !(0.0..=1.0).contains(&rate) {
                return Err(StoreError::Validation(
                    "SAMPLING_RATE_OUT_OF_RANGE".to_string(),
                ));
            }
        }

        self.with_user_tx(|tx| {
            let existing: Option<(f64, bool, bool, i64)> = tx
                .query_row(
                    "SELECT sample_rate, enabled, locked, priority
                     FROM probe_sampling_config WHERE event_type = ?1",
                    params![event_type],
                    |r| {
                        Ok((
                            r.get::<_, f64>(0)?,
                            r.get::<_, i64>(1)? != 0,
                            r.get::<_, i64>(2)? != 0,
                            r.get::<_, i64>(3)?,
                        ))
                    },
                )
                .ok();

            // require_exists:禁止凭 PATCH 创建新行(白名单 / 仅改已存在行)。
            if existing.is_none() && require_exists {
                return Err(StoreError::Validation(
                    "SAMPLING_RULE_NOT_FOUND".to_string(),
                ));
            }

            let (old_rate, old_enabled, locked, priority, action) = match existing {
                Some((r, e, l, p)) => (r, e, l, p, "mod"),
                None => (1.0, true, false, 100, "add"),
            };

            // 方案A:locked 行禁止任何 enabled / rate 变更(与现值不同即拒绝),保持"locked
            // 恒落库"语义不变(effective_sample_rate 对 locked 行恒返回 1.0)。
            if locked {
                let rate_changes = new_rate
                    .map(|rate| (rate - old_rate).abs() > f64::EPSILON)
                    .unwrap_or(false);
                let enabled_changes = new_enabled.map(|e| e != old_enabled).unwrap_or(false);
                if rate_changes || enabled_changes {
                    return Err(StoreError::Validation("SAMPLING_RULE_LOCKED".to_string()));
                }
            }

            let final_rate = new_rate.unwrap_or(old_rate);
            let final_enabled = new_enabled.unwrap_or(old_enabled);

            let old_value = serde_json::json!({
                "sampleRate": old_rate, "enabled": old_enabled
            })
            .to_string();
            let new_value = serde_json::json!({
                "sampleRate": final_rate, "enabled": final_enabled
            })
            .to_string();

            tx.execute(
                "INSERT INTO probe_sampling_config
                    (event_type, sample_rate, enabled, locked, priority, updated_at, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), ?6)
                 ON CONFLICT(event_type) DO UPDATE SET
                    sample_rate = ?2, enabled = ?3,
                    updated_at = datetime('now'), updated_by = ?6",
                params![
                    event_type,
                    final_rate,
                    final_enabled as i64,
                    locked as i64,
                    priority,
                    admin_id,
                ],
            )?;

            // audit:enabled 0->1/1->0 且无 rate 变化时记 'pause' 更贴切;否则 add/mod。
            let audit_action = if action == "mod"
                && new_rate.is_none()
                && new_enabled.is_some()
                && final_enabled != old_enabled
            {
                "pause"
            } else {
                action
            };
            tx.execute(
                "INSERT INTO probe_sampling_audit
                    (event_type, action, old_value, new_value, admin_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event_type,
                    audit_action,
                    old_value,
                    new_value,
                    admin_id,
                    chrono::Utc::now().to_rfc3339(),
                ],
            )?;

            Ok(SamplingRule {
                event_type: event_type.to_string(),
                sample_rate: final_rate,
                enabled: final_enabled,
                locked,
                priority,
            })
        })
    }

    /// audit 行(最近 limit 条,created_at 降序)。
    pub fn list_sampling_audit(&self, limit: u32) -> Result<Vec<SamplingAuditRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT created_at, action, event_type, old_value, new_value, admin_id
             FROM probe_sampling_audit
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |r| {
                Ok(SamplingAuditRow {
                    ts: r.get::<_, String>(0)?,
                    action: r.get::<_, String>(1)?,
                    event_type: r.get::<_, String>(2)?,
                    old_value: r.get::<_, Option<String>>(3)?,
                    new_value: r.get::<_, Option<String>>(4)?,
                    admin_id: r.get::<_, Option<String>>(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // ---------------------------------------------------------------------
    // 采样决策(submit_telemetry 注入点用)
    // ---------------------------------------------------------------------

    /// 计算某 telemetry event_type 的有效采样率。
    ///
    /// locked 命中行 → 恒 1.0(绝不丢弃);
    /// 否则:命中行(enabled)→ 行 rate;'*' 行(enabled)→ '*' rate;
    /// 都没命中 → 全局默认。`enabled=0` 的命中行视为关闭采集(rate=0.0)。
    pub fn effective_sample_rate(&self, event_type: &str) -> Result<f64, StoreError> {
        let conn = self.conn()?;

        // 1) 精确命中行
        let exact: Option<(f64, bool, bool)> = conn
            .query_row(
                "SELECT sample_rate, enabled, locked FROM probe_sampling_config
                 WHERE event_type = ?1",
                params![event_type],
                |r| {
                    Ok((
                        r.get::<_, f64>(0)?,
                        r.get::<_, i64>(1)? != 0,
                        r.get::<_, i64>(2)? != 0,
                    ))
                },
            )
            .ok();
        if let Some((rate, enabled, locked)) = exact {
            if locked {
                return Ok(1.0);
            }
            return Ok(if enabled { rate } else { 0.0 });
        }

        // 2) '*' 兜底行
        let star: Option<(f64, bool)> = conn
            .query_row(
                "SELECT sample_rate, enabled FROM probe_sampling_config
                 WHERE event_type = '*'",
                [],
                |r| Ok((r.get::<_, f64>(0)?, r.get::<_, i64>(1)? != 0)),
            )
            .ok();
        if let Some((rate, enabled)) = star {
            return Ok(if enabled { rate } else { 0.0 });
        }

        // 3) 全局默认
        self.global_sample_rate()
    }

    // ---------------------------------------------------------------------
    // sinks / schema / stream
    // ---------------------------------------------------------------------

    /// 单个 SQLite 表的 rowCount + lastWriteTs(用给定时间戳列)。
    fn sink_count_and_last(
        &self,
        table: &str,
        ts_col: &str,
    ) -> Result<(i64, Option<String>), StoreError> {
        let conn = self.conn()?;
        // table / ts_col 来自硬编码白名单(下方 sinks_status),非用户输入。
        let count: i64 =
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        let last: Option<String> = conn
            .query_row(&format!("SELECT MAX({ts_col}) FROM {table}"), [], |r| {
                r.get::<_, Option<String>>(0)
            })
            .unwrap_or(None);
        Ok((count, last))
    }

    /// 五个真实 sink 表的状态。lagSecs = now - lastWriteTs(无写入返回 0)。
    pub fn sinks_status(&self) -> Result<Vec<SinkStatus>, StoreError> {
        // (id, label, ts_col)
        const SINKS: [(&str, &str, &str); 5] = [
            ("telemetry_events", "遥测事件", "server_ts"),
            ("telemetry_summaries", "遥测摘要", "server_ts"),
            ("learning_records", "答题记录", "created_at"),
            ("learning_sessions", "学习会话", "created_at"),
            ("engine_monitoring_events", "引擎监控事件", "timestamp"),
        ];
        let now = chrono::Utc::now();
        let mut out = Vec::with_capacity(SINKS.len());
        for (table, label, ts_col) in SINKS {
            let (count, last) = self.sink_count_and_last(table, ts_col)?;
            let lag_secs = last
                .as_deref()
                .and_then(parse_lag_secs)
                .map(|delta| (now.timestamp() - delta).max(0))
                .unwrap_or(0);
            out.push(SinkStatus {
                id: table.to_string(),
                label: label.to_string(),
                kind: "sqlite_table".to_string(),
                row_count: count,
                last_write_ts: last,
                retention_days: None,
                lag_secs,
            });
        }
        Ok(out)
    }

    /// 取某 telemetry event_type 最新 1 行 payload_json(无数据 None)。
    pub fn schema_sample(&self, event_type: &str) -> Result<Option<(String, String)>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT server_ts, payload_json FROM telemetry_events
                 WHERE event_type = ?1 ORDER BY server_ts DESC LIMIT 1",
                params![event_type],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .ok();
        Ok(row)
    }

    /// stream:telemetry_events 最近 N 条(取完整 payload_json,解析为人话字段)。
    pub fn recent_telemetry_events(&self, limit: u32) -> Result<Vec<StreamEvent>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, server_ts, event_type, device_id, payload_json
             FROM telemetry_events
             ORDER BY server_ts DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |r| {
                let payload_json: String = r.get::<_, Option<String>>(4)?.unwrap_or_default();
                let (device, metrics) = humanize_payload(&payload_json);
                let payload_raw = if payload_json.chars().count() > 4000 {
                    payload_json.chars().take(4000).collect::<String>() + "…"
                } else {
                    payload_json
                };
                Ok(StreamEvent {
                    id: r.get::<_, String>(0)?,
                    ts: r.get::<_, String>(1)?,
                    event_type: r.get::<_, String>(2)?,
                    device_id: r.get::<_, String>(3)?,
                    device,
                    metrics,
                    payload_raw,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// 解析 payload_json → (设备摘要, 标量指标列表)。解析失败 / 非 JSON 对象 → (None, 空)。
fn humanize_payload(payload_json: &str) -> (Option<StreamDevice>, Vec<StreamMetric>) {
    let value: serde_json::Value = match serde_json::from_str(payload_json) {
        Ok(v) => v,
        Err(_) => return (None, Vec::new()),
    };
    let Some(obj) = value.as_object() else {
        return (None, Vec::new());
    };

    // 空 device 对象或无任何可读字段 → 整体省略,前端不渲染空白设备行。
    let device = obj
        .get("device")
        .and_then(|d| d.as_object())
        .map(parse_device)
        .filter(|d| d.os.is_some() || d.model.is_some() || d.online.is_some() || d.language.is_some());

    // 顶层标量 + 下钻一层 behavior.*(admin 端行为缓冲),合计最多 8 条。
    let mut metrics = Vec::new();
    collect_scalar_metrics(obj, &mut metrics);
    if let Some(behavior) = obj.get("behavior").and_then(|b| b.as_object()) {
        collect_scalar_metrics(behavior, &mut metrics);
    }
    metrics.truncate(8);

    (device, metrics)
}

/// 从 device 对象抽取关心字段(缺失即 None)。osName 端侧不含版本(另有 osVersion),
/// admin 端 osName 已含版本 → 仅当存在非空 osVersion 时拼接。
fn parse_device(d: &serde_json::Map<String, serde_json::Value>) -> StreamDevice {
    let os = d
        .get("osName")
        .and_then(|v| v.as_str())
        .map(|name| match d.get("osVersion").and_then(|v| v.as_str()) {
            Some(ver) if !ver.is_empty() => format!("{name} {ver}"),
            _ => name.to_string(),
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    StreamDevice {
        os,
        model: d
            .get("model")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        online: parse_online(d.get("onlineStatus")),
        language: d
            .get("language")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    }
}

/// onlineStatus 兼容 bool 与 "online"/"offline" 字符串两种形态。
fn parse_online(v: Option<&serde_json::Value>) -> Option<bool> {
    match v {
        Some(serde_json::Value::Bool(b)) => Some(*b),
        Some(serde_json::Value::String(s)) => match s.to_lowercase().as_str() {
            "online" | "true" | "1" => Some(true),
            "offline" | "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// 收集对象里的标量字段(数字/布尔/非空字符串)为指标;跳过 device/behavior 与嵌套对象/数组。
fn collect_scalar_metrics(
    obj: &serde_json::Map<String, serde_json::Value>,
    out: &mut Vec<StreamMetric>,
) {
    for (key, val) in obj {
        if key == "device" || key == "behavior" {
            continue;
        }
        let value = match val {
            serde_json::Value::Number(n) => format_number(n),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::String(s) if !s.is_empty() => s.clone(),
            _ => continue,
        };
        out.push(StreamMetric {
            key: key.clone(),
            value,
        });
    }
}

/// 数字格式:整数原样;小数最多两位并去尾零。
/// 先 i64 / u64 原样输出,避免超 i64 的大整数被 `as i64` 饱和钳值失真。
fn format_number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    if let Some(f) = n.as_f64() {
        if f.fract() == 0.0 {
            return format!("{f:.0}");
        }
        return format!("{f:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
    }
    n.to_string()
}

/// 生成 SQLite datetime 修饰符 `-N day`(N>=1;非法值兜底 1 天)。
fn window_offset(days: i64) -> String {
    format!("-{} day", days.max(1))
}

/// 把 RFC3339 或 `YYYY-MM-DD HH:MM:SS`(SQLite datetime('now') 格式)解析为
/// epoch 秒。两种格式都尝试。失败返回 None。
fn parse_lag_secs(ts: &str) -> Option<i64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return Some(dt.timestamp());
    }
    // SQLite datetime('now') → "2026-05-29 12:34:56"(UTC,无时区)
    chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| naive.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn test_store() -> Store {
        // Store::open(":memory:") 跑全量 schema + seed(含 probe_sampling_config 四行)。
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn validation_code(err: StoreError) -> String {
        match err {
            StoreError::Validation(c) => c,
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    // --- 方案A:locked 行禁止任何 enabled / rate 变更 -----------------------

    #[test]
    fn locked_row_rejects_enabled_change() {
        let store = test_store();
        // on_demand 为 seed 的 locked 行(enabled=1)。试图 pause(enabled=false)→ 拒绝。
        let err = store
            .upsert_sampling_rule("on_demand", None, Some(false), Some("a1"), false)
            .unwrap_err();
        assert_eq!(validation_code(err), "SAMPLING_RULE_LOCKED");

        // 确认未落库:行仍 enabled=1。
        let rule = store.get_sampling_rule("on_demand").unwrap().unwrap();
        assert!(rule.enabled, "locked 行 enabled 不应被改动");
        assert!(rule.locked);
    }

    #[test]
    fn locked_row_rejects_rate_change() {
        let store = test_store();
        let err = store
            .upsert_sampling_rule("session_start", Some(0.5), None, Some("a1"), false)
            .unwrap_err();
        assert_eq!(validation_code(err), "SAMPLING_RULE_LOCKED");
    }

    #[test]
    fn locked_row_noop_same_values_ok() {
        let store = test_store();
        // 与现值一致(rate=1.0, enabled=true)→ 不算变更,允许通过(幂等)。
        let rule = store
            .upsert_sampling_rule("on_demand", Some(1.0), Some(true), Some("a1"), false)
            .unwrap();
        assert!(rule.locked);
        assert!(rule.enabled);
        assert_eq!(rule.sample_rate, 1.0);
    }

    // --- require_exists 白名单:禁止凭 PATCH 创建新行 -----------------------

    #[test]
    fn require_exists_blocks_unknown_event_type() {
        let store = test_store();
        let err = store
            .upsert_sampling_rule("totally_unknown", Some(0.3), None, Some("a1"), true)
            .unwrap_err();
        assert_eq!(validation_code(err), "SAMPLING_RULE_NOT_FOUND");
        // 确认未创建该行。
        assert!(store.get_sampling_rule("totally_unknown").unwrap().is_none());
    }

    #[test]
    fn require_exists_false_allows_create() {
        let store = test_store();
        let rule = store
            .upsert_sampling_rule("new_evt", Some(0.3), None, Some("a1"), false)
            .unwrap();
        assert_eq!(rule.event_type, "new_evt");
        assert_eq!(rule.sample_rate, 0.3);
        assert!(!rule.locked);
    }

    // --- 非 locked 行正常改 rate / enabled -----------------------------------

    #[test]
    fn unlocked_row_updates_rate_and_enabled() {
        let store = test_store();
        // periodic 为 seed 的非 locked 行。
        let rule = store
            .upsert_sampling_rule("periodic", Some(0.25), Some(false), Some("a1"), true)
            .unwrap();
        assert_eq!(rule.sample_rate, 0.25);
        assert!(!rule.enabled);
        // 落库核验。
        let reread = store.get_sampling_rule("periodic").unwrap().unwrap();
        assert_eq!(reread.sample_rate, 0.25);
        assert!(!reread.enabled);
    }

    #[test]
    fn rate_out_of_range_rejected() {
        let store = test_store();
        let err = store
            .upsert_sampling_rule("periodic", Some(1.5), None, Some("a1"), true)
            .unwrap_err();
        assert_eq!(validation_code(err), "SAMPLING_RATE_OUT_OF_RANGE");
    }

    // --- 错误率分子口径:有错误的事件数(行计数),而非 SUM(error_count) ------

    #[test]
    fn error_rate_numerator_counts_rows_not_sum() {
        let store = test_store();
        {
            let conn = store.conn().unwrap();
            // telemetry_events 分母:3 行(window 内)。
            for i in 0..3 {
                conn.execute(
                    "INSERT INTO telemetry_events (id, device_id, event_type, server_ts, client_ts, payload_json)
                     VALUES (?1, 'd1', 'periodic', datetime('now', '-1 hour'), datetime('now', '-1 hour'), '{}')",
                    params![format!("te{i}")],
                )
                .unwrap();
            }
            // telemetry_summaries:2 行有错误(error_count=5,10),1 行无错误(0)。
            // 旧口径 SUM=15 会让 rate=15/3=5.0(>1);新口径分子=有错误行数=2。
            for (i, ec) in [5_i64, 10, 0].into_iter().enumerate() {
                conn.execute(
                    "INSERT INTO telemetry_summaries
                        (id, device_id, event_type, server_ts, error_count)
                     VALUES (?1, 'd1', 'periodic', datetime('now', '-1 hour'), ?2)",
                    params![format!("ts{i}"), ec],
                )
                .unwrap();
            }
        }
        let raw = store.overview_kpis(1).unwrap();
        assert_eq!(raw.error_events, 2, "分子应为有错误的事件数(行计数)");
        assert_eq!(raw.telemetry_total, 3, "分母应为 telemetry_events 行数");
        // rate = 2/3 ≤ 1.0
        let rate = raw.error_events as f64 / raw.telemetry_total as f64;
        assert!(rate <= 1.0);
    }

    // --- queue_backlog 排除终态行 -------------------------------------------

    #[test]
    fn queue_backlog_excludes_terminal_rows() {
        let store = test_store();
        {
            let conn = store.conn().unwrap();
            // status / completed_at 组合:
            //   pending(NULL)        → 计入
            //   confirm_pending(NULL)→ 计入
            //   ok(NULL,离线/异常态)→ 不计入(非 pending/confirm_pending)
            //   error(已完成)       → 不计入
            //   offline(NULL)        → 不计入(终态,曾永久污染积压)
            let rows = [
                ("e1", "pending", None::<&str>),
                ("e2", "confirm_pending", None),
                ("e3", "ok", None),
                ("e4", "error", Some("2026-06-14 00:00:00")),
                ("e5", "offline", None),
            ];
            for (id, status, completed) in rows {
                conn.execute(
                    "INSERT INTO probe_executions
                        (id, batch_id, device_id, admin_id, admin_username, script_body,
                         script_sha256, timeout_ms, status, dispatched_at, completed_at)
                     VALUES (?1, 'b1', 'd1', 'a1', 'a@x', 'noop', 'sha', 1000, ?2,
                             '2026-06-14 00:00:00', ?3)",
                    params![id, status, completed],
                )
                .unwrap();
            }
        }
        let raw = store.overview_kpis(1).unwrap();
        assert_eq!(
            raw.queue_backlog, 2,
            "仅 pending / confirm_pending 计入,offline 等终态不污染"
        );
    }
}
