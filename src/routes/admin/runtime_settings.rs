//! 运行时可热更设置板块（live sections）。
//!
//! 与 [`settings_sections`](super::settings_sections) 的「只存 JSON、运行期无人读取」不同，
//! 这里的 3 个 section 直接镜像 [`Config`] 中**每请求读取**的字段，保存即热替换运行时
//! 配置（[`AppState::swap_config`](crate::state::AppState::swap_config)），无需重启：
//!
//! - `live-ratelimit` — 限流窗口与配额（限流器窗口由 swap 同步更新，配额每请求读 config）
//! - `live-limits`    — 请求配额 / 连接上限 / 分页大小（各 handler 每请求读 config）
//! - `live-auth`      — 令牌 TTL（签发时每请求读 config，改后立即对新签发令牌生效）
//!
//! 字段全部扁平基本类型，匹配 admin-ui 通用 `SectionEditor`（不渲染嵌套对象）。每个字段都
//! 有真实运行期消费者——不暴露任何「只存不生效」的假字段。
//!
//! 刻意**不**暴露 `cookie_secure` / `trust_proxy`：二者是部署期安全设置（HTTPS 终止、是否
//! 真处于可信反代后），运行时随手改会引入会话失效或 X-Forwarded-For IP 伪造绕过限流的风险，
//! 应由 env / 部署配置固定，不放进运维可热改面板。

use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::response::AppError;
use crate::store::Store;

/// 运行时热更 section 白名单。
pub const LIVE_SECTIONS: &[&str] = &["live-ratelimit", "live-limits", "live-auth"];

/// 判定 section 是否为运行时热更板块。
pub fn is_live_section(section: &str) -> bool {
    LIVE_SECTIONS.contains(&section)
}

/// 把当前 [`Config`] 投影为某 live section 的扁平 JSON（GET 时下发，永远反映运行期真实值）。
pub fn live_section_json(cfg: &Config, section: &str) -> Value {
    match section {
        "live-ratelimit" => json!({
            "apiWindowSecs": cfg.rate_limit.window_secs,
            "apiAnonMaxRequests": cfg.rate_limit.anonymous_max_requests(),
            "apiAuthedMaxRequests": cfg.rate_limit.authenticated_max_requests(),
            "authWindowSecs": cfg.auth_rate_limit.window_secs,
            "authMaxRequests": cfg.auth_rate_limit.max_requests,
            "telemetryWindowSecs": cfg.telemetry_rate_limit.window_secs,
            "telemetryMaxRequests": cfg.telemetry_rate_limit.max_requests,
        }),
        "live-limits" => json!({
            "maxBatchSize": cfg.limits.max_batch_size,
            "maxImportWords": cfg.limits.max_import_words,
            "maxRecordsFetch": cfg.limits.max_records_fetch,
            "maxStatsRecords": cfg.limits.max_stats_records,
            "maxExcludeWordIds": cfg.limits.max_exclude_word_ids,
            "maxSseConnections": cfg.limits.max_sse_connections,
            "maxSseConnectionsPerUser": cfg.limits.max_sse_connections_per_user,
            "candidateWordPoolSize": cfg.limits.candidate_word_pool_size,
            "rateLimitMaxEntries": cfg.limits.rate_limit_max_entries,
            "defaultPageSize": cfg.pagination.default_page_size,
            "maxPageSize": cfg.pagination.max_page_size,
        }),
        "live-auth" => json!({
            "accessTokenHours": cfg.jwt_expires_in_hours,
            "refreshTokenHours": cfg.refresh_token_expires_in_hours,
            "adminTokenHours": cfg.admin_jwt_expires_in_hours,
        }),
        _ => json!({}),
    }
}

/// 校验并把 live section 的字段补丁应用到 `base` 的克隆上，返回新 [`Config`]。
/// 缺失字段保留原值（支持部分更新）；越界 / 类型错 → 400。
pub fn apply_live_section(
    base: &Config,
    section: &str,
    body: &Map<String, Value>,
) -> Result<Config, AppError> {
    // 窗口秒数：1s..1d；配额 / 连接：1..1亿；token：1h..1年。下界取 1 防 0 配额锁死全站。
    const SECS_MAX: u64 = 86_400;
    const COUNT_MAX: u64 = 100_000_000;
    const HOURS_MAX: u64 = 8_760;
    const PAGE_MAX: u64 = 10_000;
    // 限流器键容量下界：per_shard_max = entries/16+1；过小（如 1）会使每分片仅容 1 个 IP，
    // 第二个 IP 即被逐出而绕过限流，等于瘫痪整套限流。1000 → 每分片 63，安全下限。
    const ENTRIES_MIN: u64 = 1_000;

    let mut c = base.clone();
    match section {
        "live-ratelimit" => {
            if let Some(v) = opt_u64(body, "apiWindowSecs", 1, SECS_MAX)? {
                c.rate_limit.window_secs = v;
            }
            if let Some(v) = opt_u64(body, "apiAnonMaxRequests", 1, COUNT_MAX)? {
                c.rate_limit.anonymous_max_requests = v;
            }
            if let Some(v) = opt_u64(body, "apiAuthedMaxRequests", 1, COUNT_MAX)? {
                c.rate_limit.authenticated_max_requests = v;
            }
            if let Some(v) = opt_u64(body, "authWindowSecs", 1, SECS_MAX)? {
                c.auth_rate_limit.window_secs = v;
            }
            if let Some(v) = opt_u64(body, "authMaxRequests", 1, COUNT_MAX)? {
                c.auth_rate_limit.max_requests = v;
            }
            if let Some(v) = opt_u64(body, "telemetryWindowSecs", 1, SECS_MAX)? {
                c.telemetry_rate_limit.window_secs = v;
            }
            if let Some(v) = opt_u64(body, "telemetryMaxRequests", 1, COUNT_MAX)? {
                c.telemetry_rate_limit.max_requests = v;
            }
        }
        "live-limits" => {
            if let Some(v) = opt_usize(body, "maxBatchSize", 1, COUNT_MAX)? {
                c.limits.max_batch_size = v;
            }
            if let Some(v) = opt_usize(body, "maxImportWords", 1, COUNT_MAX)? {
                c.limits.max_import_words = v;
            }
            if let Some(v) = opt_usize(body, "maxRecordsFetch", 1, COUNT_MAX)? {
                c.limits.max_records_fetch = v;
            }
            if let Some(v) = opt_usize(body, "maxStatsRecords", 1, COUNT_MAX)? {
                c.limits.max_stats_records = v;
            }
            if let Some(v) = opt_usize(body, "maxExcludeWordIds", 1, COUNT_MAX)? {
                c.limits.max_exclude_word_ids = v;
            }
            if let Some(v) = opt_usize(body, "maxSseConnections", 1, COUNT_MAX)? {
                c.limits.max_sse_connections = v;
            }
            if let Some(v) = opt_usize(body, "maxSseConnectionsPerUser", 1, COUNT_MAX)? {
                c.limits.max_sse_connections_per_user = v;
            }
            if let Some(v) = opt_usize(body, "candidateWordPoolSize", 1, COUNT_MAX)? {
                c.limits.candidate_word_pool_size = v;
            }
            if let Some(v) = opt_usize(body, "rateLimitMaxEntries", ENTRIES_MIN, COUNT_MAX)? {
                c.limits.rate_limit_max_entries = v;
            }
            if let Some(v) = opt_u64(body, "defaultPageSize", 1, PAGE_MAX)? {
                c.pagination.default_page_size = v;
            }
            if let Some(v) = opt_u64(body, "maxPageSize", 1, PAGE_MAX)? {
                c.pagination.max_page_size = v;
            }
            if c.pagination.default_page_size > c.pagination.max_page_size {
                return Err(AppError::bad_request(
                    "INVALID_PAGE_SIZE",
                    "默认分页大小不能超过最大分页大小",
                ));
            }
        }
        "live-auth" => {
            if let Some(v) = opt_u64(body, "accessTokenHours", 1, HOURS_MAX)? {
                c.jwt_expires_in_hours = v;
            }
            if let Some(v) = opt_u64(body, "refreshTokenHours", 1, HOURS_MAX)? {
                c.refresh_token_expires_in_hours = v;
            }
            if let Some(v) = opt_u64(body, "adminTokenHours", 1, HOURS_MAX)? {
                c.admin_jwt_expires_in_hours = v;
            }
        }
        _ => {
            return Err(AppError::bad_request(
                "UNKNOWN_SECTION",
                "未知的运行时设置板块",
            ))
        }
    }
    Ok(c)
}

/// 启动期把持久化的 live section 覆盖到 env 构造的 `config` 上，使热更设置跨重启保留。
/// 单个 section 解析失败仅 warn 跳过（不阻断启动），返回叠加后的 config。
pub fn overlay_persisted_live_sections(mut config: Config, store: &Store) -> Config {
    for &section in LIVE_SECTIONS {
        match store.get_settings_config(section) {
            Ok(Some(Value::Object(map))) => match apply_live_section(&config, section, &map) {
                Ok(next) => config = next,
                Err(e) => {
                    tracing::warn!(section, error = %e.message, "持久化 live 设置应用失败，跳过")
                }
            },
            Ok(_) => {}
            Err(e) => tracing::warn!(section, error = %e, "读取持久化 live 设置失败，跳过"),
        }
    }
    config
}

// ─────────────────────────── 字段提取助手 ───────────────────────────

/// 取可选有界 u64。缺失 / null / 空串 → None（保留原值）；类型错或越界 → 400。
fn opt_u64(map: &Map<String, Value>, key: &str, min: u64, max: u64) -> Result<Option<u64>, AppError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| {
                AppError::bad_request("INVALID_FIELD", &format!("{key} 必须是非负整数"))
            })?;
            if n < min || n > max {
                return Err(AppError::bad_request(
                    "FIELD_OUT_OF_RANGE",
                    &format!("{key} 取值需在 {min}..={max} 之间"),
                ));
            }
            Ok(Some(n))
        }
    }
}

/// 取可选有界 usize（内部按 u64 校验后转换）。
fn opt_usize(
    map: &Map<String, Value>,
    key: &str,
    min: u64,
    max: u64,
) -> Result<Option<usize>, AppError> {
    Ok(opt_u64(map, key, min, max)?.map(|v| v as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    /// 与既有单测一致：先注入合法 secret 再 from_env（否则 validate_secrets 对默认值 panic）。
    fn test_cfg() -> Config {
        let secret = "test_secret_that_is_at_least_32_characters_long_ok";
        std::env::set_var("JWT_SECRET", secret);
        std::env::set_var("ADMIN_JWT_SECRET", secret);
        std::env::set_var("REFRESH_JWT_SECRET", secret);
        Config::from_env()
    }

    #[test]
    fn roundtrip_all_live_sections() {
        let cfg = test_cfg();
        for &section in LIVE_SECTIONS {
            // 投影出的 JSON 原样回灌，应解析成功且字段不变。
            let proj = live_section_json(&cfg, section);
            let applied = apply_live_section(&cfg, section, &obj(proj.clone())).unwrap();
            assert_eq!(live_section_json(&applied, section), proj, "{section} 往返不一致");
        }
    }

    #[test]
    fn ratelimit_partial_update_applies_and_keeps_others() {
        let cfg = test_cfg();
        let patched = apply_live_section(
            &cfg,
            "live-ratelimit",
            &obj(json!({ "apiWindowSecs": 30, "apiAnonMaxRequests": 7 })),
        )
        .unwrap();
        assert_eq!(patched.rate_limit.window_secs, 30);
        assert_eq!(patched.rate_limit.anonymous_max_requests, 7);
        // 未提交字段保留原值
        assert_eq!(
            patched.auth_rate_limit.window_secs,
            cfg.auth_rate_limit.window_secs
        );
    }

    #[test]
    fn limits_and_auth_apply() {
        let cfg = test_cfg();
        let limits = apply_live_section(
            &cfg,
            "live-limits",
            &obj(json!({ "maxBatchSize": 42, "defaultPageSize": 25, "maxPageSize": 200 })),
        )
        .unwrap();
        assert_eq!(limits.limits.max_batch_size, 42);
        assert_eq!(limits.pagination.default_page_size, 25);

        let auth = apply_live_section(
            &cfg,
            "live-auth",
            &obj(json!({ "accessTokenHours": 12, "adminTokenHours": 3 })),
        )
        .unwrap();
        assert_eq!(auth.jwt_expires_in_hours, 12);
        assert_eq!(auth.admin_jwt_expires_in_hours, 3);
    }

    #[test]
    fn rejects_zero_out_of_range_and_wrong_type() {
        let cfg = test_cfg();
        // 0 配额（越界，下界 1）
        assert!(apply_live_section(&cfg, "live-ratelimit", &obj(json!({ "apiAnonMaxRequests": 0 }))).is_err());
        // 窗口越上界
        assert!(apply_live_section(&cfg, "live-ratelimit", &obj(json!({ "apiWindowSecs": 999_999 }))).is_err());
        // 类型错
        assert!(apply_live_section(&cfg, "live-auth", &obj(json!({ "accessTokenHours": "x" }))).is_err());
        // rateLimitMaxEntries 低于安全下界（会瘫痪限流分片）应拒，达下界放行
        assert!(apply_live_section(&cfg, "live-limits", &obj(json!({ "rateLimitMaxEntries": 1 }))).is_err());
        assert!(apply_live_section(&cfg, "live-limits", &obj(json!({ "rateLimitMaxEntries": 1000 }))).is_ok());
    }

    #[test]
    fn rejects_default_page_gt_max_page() {
        let cfg = test_cfg();
        let r = apply_live_section(
            &cfg,
            "live-limits",
            &obj(json!({ "defaultPageSize": 500, "maxPageSize": 100 })),
        );
        assert!(r.is_err());
    }

    #[test]
    fn empty_string_is_treated_as_no_change() {
        let cfg = test_cfg();
        let patched =
            apply_live_section(&cfg, "live-auth", &obj(json!({ "accessTokenHours": "" }))).unwrap();
        assert_eq!(patched.jwt_expires_in_hours, cfg.jwt_expires_in_hours);
    }
}
