//! m034:settings.html「管理员与角色」(RBAC) + 「API 密钥」路由。
//!
//! 挂载于 /api/admin/settings 下:
//!   - /admins              GET 列表 / POST 邀请
//!   - /admins/:id/role     PATCH 改角色(守卫最后一个 super-admin)
//!   - /admins/:id          DELETE 删除(守卫最后一个 super-admin / 不能删自己)
//!   - /api-keys            GET 列表(掩码) / POST 生成(明文仅返回一次)
//!   - /api-keys/:id/rotate POST 轮换(返回新明文一次)
//!   - /api-keys/:id        DELETE 吊销
//!
//! 安全:库里只存 argon2 hash + 前缀掩码,明文绝不二次返回。

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use rand::Rng;
use serde::Deserialize;

use crate::auth::{hash_password, AdminAuthUser};
use crate::extractors::JsonBody;
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::admin_rbac::NewApiKey;

const VALID_ROLES: &[&str] = &["super_admin", "admin"];
const VALID_SCOPES: &[&str] = &["read", "write", "admin"];
const API_KEY_PLAINTEXT_PREFIX: &str = "wf_live_";
const MAX_NAME_LEN: usize = 120;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admins", get(list_admins).post(invite_admin))
        .route("/admins/:id/role", axum::routing::patch(patch_admin_role))
        .route("/admins/:id", axum::routing::delete(delete_admin))
        .route("/api-keys", get(list_api_keys).post(generate_api_key))
        .route("/api-keys/:id/rotate", post(rotate_api_key))
        .route("/api-keys/:id", axum::routing::delete(revoke_api_key))
}

// ─────────────── 管理员与角色 (RBAC) ───────────────

async fn list_admins(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let admins = state
        .run_store_task("admin.rbac.list_admins", |store| store.list_admins())
        .await??;
    Ok(ok(serde_json::json!({ "admins": admins })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InviteAdminRequest {
    email: String,
    password: String,
    /// 'super_admin' / 'admin'。默认 'admin'。
    #[serde(default)]
    role: Option<String>,
}

fn normalize_admin_role(raw: Option<&str>) -> Result<String, AppError> {
    let v = raw.unwrap_or("admin").trim();
    if VALID_ROLES.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(AppError::bad_request(
            "INVALID_ADMIN_ROLE",
            "role 必须是 'super_admin' / 'admin' 之一",
        ))
    }
}

/// 特权守卫:仅 super_admin 可调用。校验调用方(而非目标)角色,非 super_admin 返回 403。
async fn require_super_admin(state: &AppState, caller_admin_id: &str) -> Result<(), AppError> {
    let caller_id = caller_admin_id.to_string();
    let role = state
        .run_store_task("admin.rbac.require_super_admin", move |store| {
            store.get_admin_role(&caller_id)
        })
        .await??;
    match role.as_deref() {
        Some("super_admin") => Ok(()),
        _ => Err(AppError::forbidden("仅超级管理员可执行此操作")),
    }
}

async fn invite_admin(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<InviteAdminRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    require_super_admin(&state, &admin.admin_id).await?;
    let email = req.email.trim().to_lowercase();
    if !crate::validation::is_valid_email(&email) {
        return Err(AppError::bad_request("AUTH_INVALID_EMAIL", "邮箱格式无效"));
    }
    if let Err(msg) = crate::validation::validate_password(&req.password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }
    let role = normalize_admin_role(req.role.as_deref())?;
    let password_hash = hash_password(&req.password)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let view = state
        .run_store_task("admin.rbac.invite_admin", move |store| {
            store.invite_admin(&id, &email, &password_hash, &role, &now)
        })
        .await??;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "invite_admin",
        target_admin_id = %view.id,
        role = %view.role,
        "管理员邀请新管理员"
    );
    Ok(created(view))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchAdminRoleRequest {
    role: String,
}

async fn patch_admin_role(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<PatchAdminRoleRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    require_super_admin(&state, &admin.admin_id).await?;
    let new_role = normalize_admin_role(Some(&req.role))?;
    let target_id = id.clone();
    let now = Utc::now().to_rfc3339();
    let role_for_task = new_role.clone();

    let view = state
        .run_store_task(
            "admin.rbac.update_role",
            move |store| -> Result<_, AppError> {
                let current = store
                    .get_admin_role(&target_id)?
                    .ok_or_else(|| AppError::not_found("管理员不存在"))?;
                // 守卫:降级最后一个 super-admin 会导致无人能管角色。
                if current == "super_admin" && role_for_task != "super_admin" {
                    let supers = store.count_admins_with_role("super_admin")?;
                    if supers <= 1 {
                        return Err(AppError::conflict(
                            "LAST_SUPER_ADMIN",
                            "不能降级最后一个超级管理员",
                        ));
                    }
                }
                let view = store
                    .update_admin_role(&target_id, &role_for_task, &now)?
                    .ok_or_else(|| AppError::not_found("管理员不存在"))?;
                Ok(view)
            },
        )
        .await??;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "update_admin_role",
        target_admin_id = %id,
        new_role = %new_role,
        "管理员变更管理员角色"
    );
    Ok(ok(view))
}

async fn delete_admin(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    require_super_admin(&state, &admin.admin_id).await?;
    if id == admin.admin_id {
        return Err(AppError::bad_request(
            "CANNOT_DELETE_SELF",
            "不能删除当前登录的管理员账户",
        ));
    }
    let target_id = id.clone();
    state
        .run_store_task(
            "admin.rbac.delete_admin",
            move |store| -> Result<(), AppError> {
                let current = store
                    .get_admin_role(&target_id)?
                    .ok_or_else(|| AppError::not_found("管理员不存在"))?;
                // 守卫:删除最后一个 super-admin 会锁死 RBAC 管理。
                if current == "super_admin" {
                    let supers = store.count_admins_with_role("super_admin")?;
                    if supers <= 1 {
                        return Err(AppError::conflict(
                            "LAST_SUPER_ADMIN",
                            "不能删除最后一个超级管理员",
                        ));
                    }
                }
                if !store.delete_admin(&target_id)? {
                    return Err(AppError::not_found("管理员不存在"));
                }
                Ok(())
            },
        )
        .await??;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "delete_admin",
        target_admin_id = %id,
        "管理员删除管理员"
    );
    Ok(ok(serde_json::json!({ "deleted": true, "adminId": id })))
}

// ─────────────── API 密钥 ───────────────

async fn list_api_keys(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let keys = state
        .run_store_task("admin.rbac.list_api_keys", |store| store.list_api_keys())
        .await??;
    Ok(ok(serde_json::json!({ "keys": keys })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateApiKeyRequest {
    name: String,
    /// 'read' / 'write' / 'admin'。默认 'read'。
    #[serde(default)]
    scope: Option<String>,
    /// 可选过期时间(RFC3339)。空表示永不过期。
    #[serde(default)]
    expires_at: Option<String>,
}

fn normalize_scope(raw: Option<&str>) -> Result<String, AppError> {
    let v = raw.unwrap_or("read").trim();
    if VALID_SCOPES.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(AppError::bad_request(
            "INVALID_SCOPE",
            "scope 必须是 'read' / 'write' / 'admin' 之一",
        ))
    }
}

fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request("INVALID_NAME", "名称不能为空"));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(AppError::bad_request("NAME_TOO_LONG", "名称过长"));
    }
    Ok(trimmed.to_string())
}

/// 生成明文密钥:`wf_live_` + 32 个 base62 随机字符。
fn generate_plaintext_key() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let body: String = (0..32)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect();
    format!("{API_KEY_PLAINTEXT_PREFIX}{body}")
}

/// 掩码:保留 `wf_live_` + 前 3 字符 + … + 后 3 字符,如 `wf_live_a3f…9c2`。
fn mask_prefix(plaintext: &str) -> String {
    let body = plaintext
        .strip_prefix(API_KEY_PLAINTEXT_PREFIX)
        .unwrap_or(plaintext);
    let head: String = body.chars().take(3).collect();
    let tail: String = {
        let chars: Vec<char> = body.chars().collect();
        let start = chars.len().saturating_sub(3);
        chars[start..].iter().collect()
    };
    format!("{API_KEY_PLAINTEXT_PREFIX}{head}…{tail}")
}

fn validate_expires_at(raw: Option<&str>) -> Result<Option<String>, AppError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|_| Some(s.to_string()))
            .map_err(|_| {
                AppError::bad_request("INVALID_EXPIRES_AT", "过期时间必须是 RFC3339 格式")
            }),
    }
}

async fn generate_api_key(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<GenerateApiKeyRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    require_super_admin(&state, &admin.admin_id).await?;
    let name = validate_name(&req.name)?;
    let scope = normalize_scope(req.scope.as_deref())?;
    let expires_at = validate_expires_at(req.expires_at.as_deref())?;

    let plaintext = generate_plaintext_key();
    let prefix = mask_prefix(&plaintext);
    let hash = hash_password(&plaintext)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let created_by = admin.admin_id.clone();

    let id_for_task = id.clone();
    let view = state
        .run_store_task("admin.rbac.create_api_key", move |store| {
            store.create_api_key(&NewApiKey {
                id: &id_for_task,
                name: &name,
                scope: &scope,
                prefix: &prefix,
                hash: &hash,
                created_at: &now,
                created_by: Some(&created_by),
                expires_at: expires_at.as_deref(),
            })
        })
        .await??;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "generate_api_key",
        api_key_id = %view.id,
        scope = %view.scope,
        "管理员生成 API 密钥"
    );

    // 明文仅此一次返回,绝不二次给出。
    Ok(created(serde_json::json!({
        "key": view,
        "plaintext": plaintext,
        "message": "请立即保存密钥明文,关闭后将无法再次查看",
    })))
}

async fn rotate_api_key(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    require_super_admin(&state, &admin.admin_id).await?;
    let plaintext = generate_plaintext_key();
    let prefix = mask_prefix(&plaintext);
    let hash = hash_password(&plaintext)?;
    let now = Utc::now().to_rfc3339();
    let id_for_task = id.clone();

    let view = state
        .run_store_task("admin.rbac.rotate_api_key", move |store| {
            store.rotate_api_key(&id_for_task, &prefix, &hash, &now, None)
        })
        .await??
        .ok_or_else(|| AppError::not_found("API 密钥不存在"))?;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "rotate_api_key",
        api_key_id = %id,
        "管理员轮换 API 密钥"
    );

    Ok(ok(serde_json::json!({
        "key": view,
        "plaintext": plaintext,
        "message": "密钥已轮换,旧密钥立即失效;请保存新明文",
    })))
}

async fn revoke_api_key(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    require_super_admin(&state, &admin.admin_id).await?;
    let id_for_task = id.clone();
    let deleted = state
        .run_store_task("admin.rbac.delete_api_key", move |store| {
            store.delete_api_key(&id_for_task)
        })
        .await??;
    if !deleted {
        return Err(AppError::not_found("API 密钥不存在"));
    }
    tracing::info!(
        admin_id = %admin.admin_id,
        action = "revoke_api_key",
        api_key_id = %id,
        "管理员吊销 API 密钥"
    );
    Ok(ok(serde_json::json!({ "revoked": true, "keyId": id })))
}
