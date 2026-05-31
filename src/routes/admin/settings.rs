use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::Router;

use crate::extractors::JsonBody;
use serde::Deserialize;

use crate::amas::config::AMASConfig;
use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

use super::settings_sections::{mask_secrets, parse_typed_section, preserve_secrets, TYPED_SECTIONS};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_settings).put(update_settings))
        .route("/maintenance", post(set_maintenance))
        .route("/reload-amas", post(reload_amas_config))
        // m033:可扩展设置配置存储 —— settings.html 的 11 个面板各自持久化
        .route("/config", get(get_settings_config))
        .route("/config/:section", put(put_settings_config))
        .route(
            "/snapshots",
            get(list_settings_snapshots).post(create_settings_snapshot),
        )
        .route(
            "/snapshots/:id/restore",
            post(restore_settings_snapshot),
        )
        .route("/export.toml", get(export_settings_toml))
        // m034:管理员与角色 (RBAC) + API 密钥
        .merge(super::rbac::router())
}

/// settings.html 的面板 section 白名单。PUT /config/:section 仅接受其一。
/// 其中 TYPED_SECTIONS(site/auth/ratelimit/audit-config/backup-policy)走强类型校验,
/// 其余按裸 JSON object 透传。legacy 名(limits/audit/backup)保留向后兼容。
const KNOWN_SECTIONS: &[&str] = &[
    "site", "auth", "roles", "limits", "audit", "backup", "keys",
    // m033b:本地生效面板的强类型 section
    "ratelimit", "audit-config", "backup-policy",
];

/// 导出 TOML 时永不写出的敏感 section(安全:绝不导出密钥/凭据)。
const EXPORT_DENY_SECTIONS: &[&str] = &["keys", "secrets"];

const MAX_SNAPSHOT_LABEL_LEN: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSystemSettings {
    max_users: Option<u64>,
    registration_enabled: Option<bool>,
    maintenance_mode: Option<bool>,
    default_daily_words: Option<u32>,
    wordbook_center_url: Option<String>,
    amas_auto_apply_enabled: Option<bool>,
    amas_auto_apply_max_per_day: Option<u32>,
    amas_auto_apply_min_confidence: Option<f64>,
    llm_advisor_max_cost_per_month_yuan: Option<f64>,
}

impl UpdateSystemSettings {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(v) = self.max_users {
            if !(1..=1_000_000).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_MAX_USERS",
                    "最大用户数必须在1到1000000之间",
                ));
            }
        }
        if let Some(v) = self.default_daily_words {
            if !(1..=500).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_DAILY_WORDS",
                    "每日默认单词数必须在1到500之间",
                ));
            }
        }
        if let Some(v) = self.amas_auto_apply_max_per_day {
            if v > 20 {
                return Err(AppError::bad_request(
                    "INVALID_AUTO_APPLY_LIMIT",
                    "每日自动应用上限必须 ≤ 20",
                ));
            }
        }
        if let Some(v) = self.amas_auto_apply_min_confidence {
            if !(0.0..=1.0).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_CONFIDENCE",
                    "最低置信度必须在 0-1 之间",
                ));
            }
        }
        if let Some(v) = self.llm_advisor_max_cost_per_month_yuan {
            if !(1.0..=10_000.0).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_LLM_BUDGET",
                    "LLM 月度成本上限必须在 1-10000 元之间",
                ));
            }
        }
        Ok(())
    }
}

async fn get_settings(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state
        .run_store_task("admin.settings.get_settings", |store| {
            store.get_system_settings()
        })
        .await??;
    Ok(ok(settings))
}

async fn update_settings(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateSystemSettings>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    req.validate()?;
    let maintenance_mode_changed = req.maintenance_mode.is_some();
    let settings = state
        .run_store_task(
            "admin.settings.update_settings",
            move |store| -> Result<_, AppError> {
                let mut settings = store.get_system_settings()?;

                if let Some(v) = req.max_users {
                    settings.max_users = v;
                }
                if let Some(v) = req.registration_enabled {
                    settings.registration_enabled = v;
                }
                if let Some(v) = req.maintenance_mode {
                    settings.maintenance_mode = v;
                }
                if let Some(v) = req.default_daily_words {
                    settings.default_daily_words = v;
                }
                if let Some(ref v) = req.wordbook_center_url {
                    settings.wordbook_center_url =
                        if v.is_empty() { None } else { Some(v.clone()) };
                }
                if let Some(v) = req.amas_auto_apply_enabled {
                    settings.amas_auto_apply_enabled = v;
                }
                if let Some(v) = req.amas_auto_apply_max_per_day {
                    settings.amas_auto_apply_max_per_day = v;
                }
                if let Some(v) = req.amas_auto_apply_min_confidence {
                    settings.amas_auto_apply_min_confidence = v;
                }
                if let Some(v) = req.llm_advisor_max_cost_per_month_yuan {
                    settings.llm_advisor_max_cost_per_month_yuan = v;
                }

                store.save_system_settings(&settings)?;
                Ok(settings)
            },
        )
        .await??;

    if maintenance_mode_changed {
        state.set_maintenance(settings.maintenance_mode);
    }

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "update_settings",
        "管理员更新系统设置: max_users={}, registration={}, maintenance={}, daily_words={}",
        settings.max_users, settings.registration_enabled, settings.maintenance_mode, settings.default_daily_words
    );

    Ok(ok(settings))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetMaintenanceRequest {
    active: bool,
}

async fn set_maintenance(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<SetMaintenanceRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let active = req.active;
    state
        .run_store_task("admin.settings.set_maintenance", move |store| -> Result<_, AppError> {
            let mut settings = store.get_system_settings()?;
            settings.maintenance_mode = active;
            store.save_system_settings(&settings)?;
            Ok(settings)
        })
        .await??;

    state.set_maintenance(active);

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "set_maintenance",
        active,
        "管理员切换维护模式"
    );

    Ok(ok(serde_json::json!({ "active": active })))
}

async fn reload_amas_config(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(new_config): JsonBody<AMASConfig>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    new_config
        .validate()
        .map_err(|e| AppError::bad_request("INVALID_AMAS_CONFIG", &e))?;
    state
        .amas()
        .reload_config(new_config)
        .map_err(|e| AppError::bad_request("INVALID_AMAS_CONFIG", &e))?;
    let config = state.amas().get_config();

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "reload_amas_config",
        "管理员重载 AMAS 配置"
    );

    Ok(ok(config))
}

// ─────────────── m033:可扩展设置配置存储 ───────────────

/// GET /api/admin/settings/config —— 返回全部 section 配置。
async fn get_settings_config(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let mut sections = state
        .run_store_task("admin.settings.list_config", |store| {
            store.list_settings_config()
        })
        .await??;
    // m035:GET 时遮蔽各 section 的敏感字段(SMTP/SMS/存储/DB 密钥),绝不回显明文。
    for s in &mut sections {
        s.json = mask_secrets(&s.section, &s.json);
    }
    Ok(ok(serde_json::json!({ "sections": sections })))
}

/// PUT /api/admin/settings/config/:section —— upsert 单个 section(白名单校验)。
/// body 为该 section 的任意 JSON object。
async fn put_settings_config(
    admin: AdminAuthUser,
    Path(section): Path<String>,
    State(state): State<AppState>,
    JsonBody(body): JsonBody<serde_json::Value>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !KNOWN_SECTIONS.contains(&section.as_str()) {
        return Err(AppError::bad_request(
            "UNKNOWN_SECTION",
            "未知的设置板块",
        ));
    }
    if !body.is_object() {
        return Err(AppError::bad_request(
            "INVALID_SECTION_BODY",
            "section 配置体必须是 JSON 对象",
        ));
    }

    // 强类型 section:serde 解析 + 字段校验,通过后归一化再持久化。
    // registration_open 仅 auth section 非 None,用于同步 system_settings(向后兼容)。
    let (normalized, registration_open) = if TYPED_SECTIONS.contains(&section.as_str()) {
        let (json, reg) = parse_typed_section(&section, &body)?;
        (json, reg)
    } else {
        (body, None)
    };

    let section_for_task = section.clone();
    let saved = state
        .run_store_task("admin.settings.upsert_config", move |store| -> Result<_, AppError> {
            // m035:密钥保留——前端回显遮蔽串、原样提交时,用旧值回填空/遮蔽的敏感字段,
            // 避免把真实密钥冲掉。
            let mut to_store = normalized;
            let existing = store.get_settings_config(&section_for_task)?;
            preserve_secrets(&section_for_task, &mut to_store, existing.as_ref());

            let mut saved = store.upsert_settings_config(&section_for_task, &to_store)?;
            // 向后兼容:auth 注册策略同步到 system_settings.registration_enabled,
            // 让既有注册门控(routes/auth.rs)无需改动即可生效。
            if let Some(open) = registration_open {
                let mut sys = store.get_system_settings()?;
                if sys.registration_enabled != open {
                    sys.registration_enabled = open;
                    store.save_system_settings(&sys)?;
                }
            }
            // m035:响应体同样遮蔽密钥,绝不回写明文。
            saved.json = mask_secrets(&section_for_task, &saved.json);
            Ok(saved)
        })
        .await??;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "update_settings_config",
        section = %section,
        "管理员更新设置板块"
    );

    Ok(ok(saved))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSnapshotRequest {
    #[serde(default)]
    label: Option<String>,
}

/// POST /api/admin/settings/snapshots —— 对当前全部 section 创建快照。
async fn create_settings_snapshot(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<CreateSnapshotRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let label = req
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(ref l) = label {
        if l.chars().count() > MAX_SNAPSHOT_LABEL_LEN {
            return Err(AppError::bad_request(
                "LABEL_TOO_LONG",
                "快照标签不能超过 200 个字符",
            ));
        }
    }
    let label_for_task = label.clone();
    let meta = state
        .run_store_task("admin.settings.create_snapshot", move |store| {
            store.create_settings_snapshot(label_for_task.as_deref())
        })
        .await??;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "create_settings_snapshot",
        snapshot_id = %meta.id,
        "管理员创建设置快照"
    );

    Ok(crate::response::created(meta))
}

/// GET /api/admin/settings/snapshots —— 列出快照元信息(最多 100 条)。
async fn list_settings_snapshots(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let snapshots = state
        .run_store_task("admin.settings.list_snapshots", |store| {
            store.list_settings_snapshots(100)
        })
        .await??;
    Ok(ok(serde_json::json!({ "snapshots": snapshots })))
}

/// POST /api/admin/settings/snapshots/:id/restore —— 回滚到指定快照(全量替换 config)。
async fn restore_settings_snapshot(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let id_for_task = id.clone();
    let sections = state
        .run_store_task("admin.settings.restore_snapshot", move |store| {
            store.restore_settings_snapshot(&id_for_task)
        })
        .await??
        .ok_or_else(|| AppError::not_found("快照不存在"))?;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "restore_settings_snapshot",
        snapshot_id = %id,
        "管理员回滚设置快照"
    );

    Ok(ok(serde_json::json!({ "sections": sections })))
}

/// GET /api/admin/settings/export.toml —— 以 text/plain TOML 导出全部 section
/// 配置。安全:永不导出 keys/secrets 等敏感 section。
async fn export_settings_toml(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;

    let sections = state
        .run_store_task("admin.settings.export", |store| {
            store.list_settings_config()
        })
        .await??;

    let mut out = String::from("# WordForge 系统设置导出\n");
    for s in &sections {
        if EXPORT_DENY_SECTIONS.contains(&s.section.as_str()) {
            continue;
        }
        out.push_str(&format!("\n[{}]\n", s.section));
        if let Some(obj) = s.json.as_object() {
            for (k, v) in obj {
                out.push_str(&format!("{} = {}\n", k, toml_scalar(v)));
            }
        }
    }

    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        out,
    )
        .into_response())
}

/// 把 JSON 标量渲染为 TOML 值字面量。嵌套对象/数组与 null 退化为带引号字符串,
/// 保证输出始终是合法 TOML(导出仅作快照查看,不追求结构完全还原)。
fn toml_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => toml_quote(s),
        serde_json::Value::Null => "\"\"".to_string(),
        other => toml_quote(&other.to_string()),
    }
}

/// TOML 基础字符串转义(双引号 / 反斜杠 / 换行)。
fn toml_quote(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}
