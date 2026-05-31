use axum::extract::State;
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{
    extract_token_from_headers, generate_dummy_argon2_hash, hash_password, hash_token,
    sign_jwt_for_admin, verify_password, AdminAuthUser,
};
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::admins::Admin;
use crate::store::operations::sessions::Session;
use crate::validation::{is_valid_email, validate_password};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/setup", post(setup))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/verify", get(verify))
}

/// 不受 auth rate limit 约束的公开路由
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/status", get(auth_status))
        .route("/setup/env-check", get(env_check))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatusResponse {
    initialized: bool,
}

/// setup 页五项环境自检的单项结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvCheckItem {
    key: &'static str,
    label: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvCheckResponse {
    checks: Vec<EnvCheckItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    email: String,
    password: String,
    /// 勾选"30 天保持登录"时为 true，签发 30 天有效期 token（仅限本设备）
    #[serde(default)]
    remember_me: bool,
}

/// 勾选"保持登录"时的 token 有效期：30 天
const REMEMBER_ME_EXPIRES_IN_HOURS: u64 = 24 * 30;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminAuthResponse {
    token: String,
    admin: AdminProfile,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminProfile {
    id: String,
    email: String,
}

async fn auth_status(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let initialized = state
        .run_store_task("admin_auth.status.any_admin_exists", |store| {
            store.any_admin_exists()
        })
        .await??;
    Ok(ok(AuthStatusResponse { initialized }))
}

/// setup 页五项环境自检：公开预认证（与 /status 同级，不需要 admin token）。
/// 全部为只读探测，不修改任何状态。
async fn env_check(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let mut checks = Vec::with_capacity(5);

    // ① DB schema 迁移版本：当前版本 vs 全量迁移数
    let expected = crate::store::migrate::migration_count() as u32;
    let version_res = state
        .run_store_task("admin_setup.env_check.schema_version", |store| {
            crate::store::migrate::get_current_version(&store)
        })
        .await;
    let (version_ok, version_detail) = match version_res {
        Ok(Ok(v)) => (
            v == expected,
            format!("schema_version v{v} / 全量 {expected} 次 migration"),
        ),
        Ok(Err(e)) => (false, format!("读取 schema_version 失败：{e}")),
        Err(e) => (false, format!("迁移版本探测任务失败：{e}")),
    };
    checks.push(EnvCheckItem {
        key: "db_schema",
        label: "DB schema 已迁移",
        ok: version_ok,
        detail: version_detail,
    });

    // ② 静态目录可写：api_only 模式下不托管 static，标记跳过为 ok
    let (static_ok, static_detail) = if state.config().api_only {
        (true, "API_ONLY 模式 · 不托管 static".to_string())
    } else {
        check_static_dir_writable()
    };
    checks.push(EnvCheckItem {
        key: "static_writable",
        label: "静态目录可写",
        ok: static_ok,
        detail: static_detail,
    });

    // ③ 端口监听：进程已起则必然在监听，报告绑定地址
    let cfg = state.config();
    checks.push(EnvCheckItem {
        key: "listening_port",
        label: "端口监听",
        ok: true,
        detail: format!("{}:{}", cfg.host, cfg.port),
    });

    // ④ Ed25519 资源包签名密钥：main.rs 启动期注入，None 表示加载/生成失败
    let signer = state.resource_pack_signer().await;
    let (key_ok, key_detail) = match signer {
        Some(s) => {
            let pk = s.public_key_base64();
            let prefix: String = pk.chars().take(12).collect();
            (true, format!("Ed25519 · 公钥 {prefix}…"))
        }
        None => (false, "签名器未就绪（密钥加载/生成失败）".to_string()),
    };
    checks.push(EnvCheckItem {
        key: "signing_key",
        label: "资源包签名密钥",
        ok: key_ok,
        detail: key_detail,
    });

    // ⑤ admin_users（物理表 admins）可达 / 是否为空
    let admin_res = state
        .run_store_task("admin_setup.env_check.admin_exists", |store| {
            store.any_admin_exists()
        })
        .await;
    let (admin_ok, admin_detail) = match admin_res {
        Ok(Ok(exists)) => (
            true,
            if exists {
                "已存在管理员".to_string()
            } else {
                "表可达 · 尚未初始化".to_string()
            },
        ),
        Ok(Err(e)) => (false, format!("查询 admins 表失败：{e}")),
        Err(e) => (false, format!("admins 表探测任务失败：{e}")),
    };
    checks.push(EnvCheckItem {
        key: "admin_table",
        label: "检测 admin_users 表",
        ok: admin_ok,
        detail: admin_detail,
    });

    Ok(ok(EnvCheckResponse { checks }))
}

/// 探测 static 目录是否存在且可写：写入再删除一个临时探针文件。
fn check_static_dir_writable() -> (bool, String) {
    let dir = std::path::Path::new("static");
    if !dir.is_dir() {
        return (false, "static/ 目录不存在".to_string());
    }
    let probe = dir.join(".env_check_probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            (true, "static/ 可写".to_string())
        }
        Err(e) => (false, format!("static/ 不可写：{e}")),
    }
}

async fn setup(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<SetupRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !is_valid_email(&req.email) {
        return Err(AppError::bad_request("ADMIN_INVALID_EMAIL", "邮箱格式无效"));
    }
    if let Err(msg) = validate_password(&req.password) {
        return Err(AppError::bad_request("ADMIN_WEAK_PASSWORD", msg));
    }

    let password = req.password;
    let password_hash =
        crate::blocking::run_blocking("admin_auth.setup.hash_password", move || {
            hash_password(&password)
        })
        .await??;
    let admin = Admin {
        id: uuid::Uuid::new_v4().to_string(),
        email: req.email.trim().to_lowercase(),
        password_hash,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        failed_login_count: 0,
        locked_until: None,
    };

    // 使用 create_first_admin 在事务内部原子性检查是否已有 admin，防止 TOCTOU
    let admin_for_create = admin.clone();
    state
        .run_store_task("admin_auth.setup.create_first_admin", move |store| {
            store.create_first_admin(&admin_for_create)
        })
        .await
        .map_err(AppError::from)?
        .map_err(|e| {
            if matches!(e, crate::store::StoreError::Conflict { .. }) {
                AppError::conflict("ADMIN_ALREADY_EXISTS", "管理员账户已存在")
            } else {
                AppError::from(e)
            }
        })?;

    let token = sign_jwt_for_admin(
        &admin.id,
        &state.config().admin_jwt_secret,
        state.config().admin_jwt_expires_in_hours,
    )?;

    let session = Session {
        token_hash: hash_token(&token),
        user_id: admin.id.clone(),
        token_type: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().admin_jwt_expires_in_hours as i64),
        revoked: false,
    };
    state
        .run_store_task("admin_auth.setup.create_admin_session", move |store| {
            store.create_admin_session(&session)
        })
        .await??;

    Ok(created(AdminAuthResponse {
        token,
        admin: AdminProfile {
            id: admin.id,
            email: admin.email,
        },
    }))
}

async fn login(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<LoginRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let email = req.email.trim().to_lowercase();
    let (admin, is_locked) = state
        .run_store_task(
            "admin_auth.login.lookup",
            move |store| -> Result<_, AppError> {
                let admin = store.get_admin_by_email(&email)?;
                let is_locked = if let Some(ref admin) = admin {
                    store.is_admin_account_locked(&admin.id)?
                } else {
                    false
                };
                Ok((admin, is_locked))
            },
        )
        .await??;

    // 检查账户是否因多次登录失败而被锁定
    if is_locked {
        return Err(AppError::too_many_requests(
            "账户因多次登录失败已被临时锁定，请稍后再试",
        ));
    }

    let stored_hash = admin
        .as_ref()
        .map(|admin| admin.password_hash.clone())
        .unwrap_or_else(generate_dummy_argon2_hash);
    let password = req.password;
    let verified = crate::blocking::run_blocking("admin_auth.login.verify_password", move || {
        verify_password(&password, &stored_hash)
    })
    .await??;
    if !verified || admin.is_none() {
        // 记录登录失败，可能触发锁定
        if let Some(ref a) = admin {
            let admin_id = a.id.clone();
            match state
                .run_store_task("admin_auth.login.record_failed_login", move |store| {
                    store.record_admin_failed_login(&admin_id)
                })
                .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => tracing::error!(
                    admin_id = %a.id,
                    error = %e,
                    "记录管理员登录失败次数时出错"
                ),
                Err(e) => tracing::error!(
                    admin_id = %a.id,
                    error = %e,
                    "记录管理员登录失败任务失败"
                ),
            }
        }
        return Err(AppError::unauthorized("邮箱或密码错误"));
    }

    let admin = admin.unwrap();

    // 登录成功，重置失败计数
    let admin_id = admin.id.clone();
    match state
        .run_store_task("admin_auth.login.reset_attempts", move |store| {
            store.reset_admin_login_attempts(&admin_id)
        })
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::error!(
            admin_id = %admin.id,
            error = %e,
            "重置管理员登录失败计数时出错"
        ),
        Err(e) => tracing::error!(
            admin_id = %admin.id,
            error = %e,
            "重置管理员登录失败计数任务失败"
        ),
    }

    // remember_me 勾选时签发 30 天 token，否则用默认 admin_jwt_expires_in_hours
    let expires_in_hours = if req.remember_me {
        REMEMBER_ME_EXPIRES_IN_HOURS
    } else {
        state.config().admin_jwt_expires_in_hours
    };
    let token = sign_jwt_for_admin(&admin.id, &state.config().admin_jwt_secret, expires_in_hours)?;

    let session = Session {
        token_hash: hash_token(&token),
        user_id: admin.id.clone(),
        token_type: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(expires_in_hours as i64),
        revoked: false,
    };
    state
        .run_store_task("admin_auth.login.create_admin_session", move |store| {
            store.create_admin_session(&session)
        })
        .await??;

    Ok(ok(AdminAuthResponse {
        token,
        admin: AdminProfile {
            id: admin.id,
            email: admin.email,
        },
    }))
}

/// 验证当前管理员 token 是否有效，返回管理员基本信息
async fn verify(
    admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let admin_record = state
        .run_store_task("admin_auth.verify.get_admin_by_id", move |store| {
            store.get_admin_by_id(&admin.admin_id)
        })
        .await??
        .ok_or_else(|| AppError::unauthorized("管理员不存在"))?;
    Ok(ok(AdminProfile {
        id: admin_record.id,
        email: admin_record.email,
    }))
}

async fn logout(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let token = extract_token_from_headers(&headers)?;
    let token_hash = hash_token(&token);
    state
        .run_store_task("admin_auth.logout.delete_admin_session", move |store| {
            store.delete_admin_session(&token_hash)
        })
        .await??;
    Ok(ok(serde_json::json!({"loggedOut": true})))
}
