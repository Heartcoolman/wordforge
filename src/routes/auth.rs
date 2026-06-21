use axum::extract::State;
use axum::http::{header::SET_COOKIE, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{
    extract_refresh_token_from_headers, generate_dummy_argon2_hash, hash_password, hash_token,
    sign_jwt_for_user, sign_refresh_token_for_user, verify_jwt, verify_password, AuthUser,
};
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::sessions::Session;
use crate::store::operations::users::User;
use crate::validation::{is_valid_email, validate_password, validate_username};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
        .route("/verify-reset-token", post(verify_reset_token))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResetTokenRequest {
    pub token: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub id: String,
    pub email: String,
    pub username: String,
    pub is_banned: bool,
}

impl From<&User> for UserProfile {
    fn from(value: &User) -> Self {
        Self {
            id: value.id.clone(),
            email: value.email.clone(),
            username: value.username.clone(),
            is_banned: value.is_banned,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub access_token: String,
    /// refresh token 也通过 JSON body 暴露，便于 iOS 等无 cookie jar 场景
    /// 持久化到 Keychain；同时仍下发 HttpOnly cookie 兼容 Web 浏览器。
    pub refresh_token: String,
    pub user: UserProfile,
}

/// 每用户最大并发会话数
const MAX_SESSIONS_PER_USER: usize = 10;

/// Issue an access + refresh token pair and persist the access session.
async fn issue_token_pair(user_id: &str, state: &AppState) -> Result<(String, String), AppError> {
    let user_id = user_id.to_string();
    // 单快照：令牌 exp 与会话 expires_at 用同一 config，避免并发 swap_config 期间二者被撕裂
    // （热改 TTL 时签出的令牌 exp 与落库会话有效期不一致）。
    let config = state.config();
    let access_token = sign_jwt_for_user(
        &user_id,
        &config.jwt_secret,
        config.jwt_expires_in_hours,
    )?;

    let refresh_token = sign_refresh_token_for_user(
        &user_id,
        &config.refresh_jwt_secret,
        config.refresh_token_expires_in_hours,
    )?;

    let access_session = Session {
        token_hash: hash_token(&access_token),
        user_id: user_id.clone(),
        token_type: "user".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(config.jwt_expires_in_hours as i64),
        revoked: false,
    };

    let refresh_session = Session {
        token_hash: hash_token(&refresh_token),
        user_id: user_id.clone(),
        token_type: "refresh".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(config.refresh_token_expires_in_hours as i64),
        revoked: false,
    };

    let cleanup_user_id = user_id.clone();
    state
        .run_store_task(
            "auth.issue_token_pair",
            move |store| -> Result<(), AppError> {
                // 每次登录/刷新写入 access+refresh 两行，故上限须按「会话对」换算为行数（*2），
                // 否则 MAX_SESSIONS_PER_USER 行实际只够约一半的并发设备数，并会成对裁掉仍有效的旧设备。
                if let Err(e) = store
                    .cleanup_oldest_user_sessions(&cleanup_user_id, MAX_SESSIONS_PER_USER * 2)
                {
                    tracing::warn!(user_id = %cleanup_user_id, error = %e, "清理多余会话失败");
                }

                store.create_session(&access_session)?;
                store.create_session(&refresh_session)?;
                Ok(())
            },
        )
        .await??;

    Ok((access_token, refresh_token))
}

async fn register(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<RegisterRequest>,
) -> Result<Response, AppError> {
    let email = req.email.trim().to_lowercase();
    if !is_valid_email(&email) {
        return Err(AppError::bad_request("AUTH_INVALID_EMAIL", "邮箱格式无效"));
    }
    let username = req.username.trim().to_string();
    if let Err(msg) = validate_username(&username) {
        return Err(AppError::bad_request("AUTH_INVALID_USERNAME", msg));
    }
    if let Err(msg) = validate_password(&req.password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }

    let password = req.password;
    let email_for_lookup = email.clone();
    let user = state
        .run_store_task(
            "auth.register.create_user",
            move |store| -> Result<User, AppError> {
                let system_settings = store.get_system_settings()?;
                if !system_settings.registration_enabled {
                    return Err(AppError::forbidden("注册功能已关闭"));
                }
                if system_settings.maintenance_mode {
                    return Err(AppError::forbidden("系统正在维护中"));
                }
                if store.get_user_by_email(&email_for_lookup)?.is_some() {
                    return Err(AppError::conflict("AUTH_EMAIL_EXISTS", "该邮箱已被注册"));
                }
                if store.count_users()? >= system_settings.max_users as usize {
                    return Err(AppError::forbidden("用户注册数量已达上限"));
                }
                let password_hash = hash_password(&password)?;
                let now = Utc::now();
                let user = User {
                    id: uuid::Uuid::new_v4().to_string(),
                    email: email_for_lookup,
                    username,
                    password_hash,
                    is_banned: false,
                    created_at: now,
                    updated_at: now,
                    failed_login_count: 0,
                    locked_until: None,
                    role: "user".to_string(),
                    status: "active".to_string(),
                    last_login_at: None,
                    // m025:公开注册流暂不记录 referral,未来由前端 req.referrer 传入
                    referrer_source: None,
                };
                store.create_user(&user)?;
                Ok(user)
            },
        )
        .await??;

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state).await?;

    tracing::info!(
        user_id = %user.id,
        email = %mask_email_for_log(&user.email),
        "用户注册成功"
    );

    let payload = AuthResponse {
        access_token: access_token.clone(),
        refresh_token: refresh_token.clone(),
        user: UserProfile::from(&user),
    };

    let secure = state.config().cookie_secure;
    let mut response = created(payload).into_response();
    set_token_cookie(
        &mut response,
        &access_token,
        state.config().jwt_expires_in_hours * 3600,
        secure,
    )?;
    set_refresh_token_cookie(
        &mut response,
        &refresh_token,
        state.config().refresh_token_expires_in_hours * 3600,
        secure,
    )?;
    Ok(response)
}

// m025:从代理 / 直连请求里 best-effort 取 client IP,无则返回 None。
// 优先不可伪造的 x-real-ip(nginx $remote_addr),次 XFF 最右段(最近可信跳),均无 → None。
// 取最右段而非首段以防客户端注入 XFF 首值伪造审计 IP,与 rate_limit 限流口径一致。
fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    trust_proxy: bool,
    connect_ip: Option<std::net::IpAddr>,
) -> Option<String> {
    // 仅在 trust_proxy 时信任客户端可伪造的 x-real-ip/XFF 头(与 rate_limit 口径一致),
    // 否则落连接对端 socket IP——直连暴露下客户端可任意伪造头,信任会污染登录审计 IP。
    if trust_proxy {
        if let Some(real) = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return Some(real);
        }
        if let Some(xff) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|xff| xff.split(',').next_back())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return Some(xff);
        }
    }
    connect_ip.map(|ip| ip.to_string())
}

async fn login(
    State(state): State<AppState>,
    connect_info: Option<axum::extract::ConnectInfo<std::net::SocketAddr>>,
    headers: axum::http::HeaderMap,
    JsonBody(req): JsonBody<LoginRequest>,
) -> Result<Response, AppError> {
    let connect_ip = connect_info.map(|ci| ci.0.ip());
    let client_ip = extract_client_ip(&headers, state.config().trust_proxy, connect_ip);
    let email = req.email.trim().to_lowercase();
    let (user, is_locked) = state
        .run_store_task("auth.login.lookup", move |store| -> Result<_, AppError> {
            if store.get_system_settings()?.maintenance_mode {
                return Err(AppError::forbidden("系统正在维护中"));
            }

            let user = store.get_user_by_email(&email)?;
            // 锁定状态直接从已取出的 user 行派生（locked_until > now），不再为「存在账号」
            // 多跑一条 is_account_locked SELECT——该非对称额外查询会泄露账号存在性的时序 oracle。
            let is_locked = user
                .as_ref()
                .and_then(|u| u.locked_until)
                .is_some_and(|t| t > Utc::now());
            Ok((user, is_locked))
        })
        .await??;

    // 先做密码校验（始终对真实或 dummy hash 跑，constant-time 防时序），校验失败一律返回通用 401，
    // 不暴露账号是否存在/封禁/锁定——把封禁与锁定状态的区分推迟到密码校验通过之后，避免枚举 oracle。
    let stored_hash = user
        .as_ref()
        .map(|user| user.password_hash.clone())
        .unwrap_or_else(generate_dummy_argon2_hash);
    let password = req.password;
    let verified = crate::blocking::run_blocking("auth.login.verify_password", move || {
        verify_password(&password, &stored_hash)
    })
    .await??;
    if !verified || user.is_none() {
        // 把失败计数写出同步响应路径(detached spawn):无论账号是否存在,响应延迟都只含
        // argon2 校验,不再因「存在账号多一次 DB 写」而泄露账号存在性(枚举时序 oracle)。
        if let Some(ref u) = user {
            let user_id = u.id.clone();
            let log_id = u.id.clone();
            let state = state.clone();
            tokio::spawn(async move {
                match state
                    .run_store_task("auth.login.record_failed_login", move |store| {
                        store.record_failed_login(&user_id)
                    })
                    .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => {
                        tracing::warn!(user_id = %log_id, error = %e, "Failed to record login failure")
                    }
                    Err(e) => {
                        tracing::warn!(user_id = %log_id, error = %e, "Login failure task failed")
                    }
                }
            });
        }
        return Err(AppError::unauthorized("邮箱或密码错误"));
    }

    let user = user.unwrap();

    // 密码已验证通过，方可披露封禁/锁定状态（仅对持有正确密码的账号本人，非枚举攻击者）。
    if user.is_banned {
        return Err(AppError::forbidden("用户已被封禁"));
    }
    // 安全权衡（已知可接受风险）：账户锁定是暴力破解的核心防御,但知道受害者邮箱的攻击者可故意
    // 提交若干次错误密码把账户打到锁定态,形成定向 DoS。此处不软化锁定语义——放行「锁定期内的正确
    // 密码」会重新打开锁定窗口内的无限猜测(攻击者借响应区分命中/未命中)。爆炸半径已被 record_failed_login
    // 的 already_locked 守卫限制为单个不可延长的 15 分钟自动解锁窗口,且 IP 维度另有 auth_rate_limit。
    // 彻底消除定向 DoS 需 CAPTCHA / 步进验证等额外设施(本构建邮件重置亦关闭),超出本次修复范围。
    if is_locked {
        return Err(AppError::account_locked(
            "账户因多次登录失败已被临时锁定，请稍后再试",
        ));
    }

    let user_id = user.id.clone();
    match state
        .run_store_task("auth.login.reset_login_attempts", move |store| {
            store.reset_login_attempts(&user_id)
        })
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!(user_id = %user.id, error = %e, "Failed to reset login attempts")
        }
        Err(e) => {
            tracing::warn!(user_id = %user.id, error = %e, "Reset login attempts task failed")
        }
    }

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state).await?;

    tracing::info!(
        user_id = %user.id,
        email = %mask_email_for_log(&user.email),
        "用户登录成功"
    );

    // m025:用户活动日志(失败仅 warn,不阻塞登录响应)
    {
        let store = state.store().clone();
        let user_id = user.id.clone();
        let ip = client_ip.clone();
        let ua = headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        tokio::task::spawn_blocking(move || {
            if let Err(e) = store.insert_user_activity(
                &user_id,
                "user.login",
                Some(&serde_json::json!({ "ua": ua })),
                ip.as_deref(),
            ) {
                tracing::warn!(error = %e, "写 user.login activity 失败(不影响登录主流程)");
            }
        });
    }

    let payload = AuthResponse {
        access_token: access_token.clone(),
        refresh_token: refresh_token.clone(),
        user: UserProfile::from(&user),
    };

    let secure = state.config().cookie_secure;
    let mut response = ok(payload).into_response();
    set_token_cookie(
        &mut response,
        &access_token,
        state.config().jwt_expires_in_hours * 3600,
        secure,
    )?;
    set_refresh_token_cookie(
        &mut response,
        &refresh_token,
        state.config().refresh_token_expires_in_hours * 3600,
        secure,
    )?;
    Ok(response)
}

async fn refresh(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    // Extract the refresh token from Authorization header or cookie
    let old_token = extract_refresh_token_from_headers(&headers)?;

    // Verify the JWT is valid and has token_type == "refresh"
    let claims = verify_jwt(&old_token, &state.config().refresh_jwt_secret)?;
    if claims.token_type != "refresh" {
        return Err(AppError::unauthorized("令牌类型无效：需要刷新令牌"));
    }

    // Verify the refresh session exists in the store
    let old_hash = hash_token(&old_token);
    let old_hash_for_lookup = old_hash.clone();
    let session = state
        .run_store_task("auth.refresh.get_session", move |store| {
            store.get_session(&old_hash_for_lookup)
        })
        .await??
        .ok_or_else(|| AppError::unauthorized("刷新会话不存在或已过期"))?;

    if session.user_id != claims.sub {
        return Err(AppError::unauthorized("刷新会话不匹配"));
    }

    // 原子性删除旧的 refresh 会话，防止 token 重放攻击
    let old_hash_for_delete = old_hash.clone();
    let was_deleted = state
        .run_store_task("auth.refresh.delete_session", move |store| {
            store.delete_session_if_exists(&old_hash_for_delete)
        })
        .await??;
    if !was_deleted {
        // token 已被使用（可能是重放攻击），拒绝请求
        return Err(AppError::unauthorized("刷新令牌已被使用"));
    }

    // 在签发新 token 前检查用户状态（封禁检查）
    let user = state
        .run_store_task("auth.refresh.get_user", {
            let user_id = claims.sub.clone();
            move |store| store.get_user_by_id(&user_id)
        })
        .await??
        .ok_or_else(|| AppError::unauthorized("用户不存在"))?;

    if user.is_banned {
        return Err(AppError::forbidden("用户已被封禁"));
    }

    // Issue a new token pair
    let (access_token, refresh_token) = issue_token_pair(&claims.sub, &state).await?;

    let secure = state.config().cookie_secure;
    let mut response = ok(AuthResponse {
        access_token: access_token.clone(),
        refresh_token: refresh_token.clone(),
        user: UserProfile::from(&user),
    })
    .into_response();
    set_token_cookie(
        &mut response,
        &access_token,
        state.config().jwt_expires_in_hours * 3600,
        secure,
    )?;
    set_refresh_token_cookie(
        &mut response,
        &refresh_token,
        state.config().refresh_token_expires_in_hours * 3600,
        secure,
    )?;
    Ok(response)
}

async fn logout(auth_user: AuthUser, State(state): State<AppState>) -> Result<Response, AppError> {
    let user_id = auth_user.user_id.clone();
    state
        .run_store_task("auth.logout.delete_user_sessions", move |store| {
            store.delete_user_sessions(&user_id)
        })
        .await??;

    let mut response = ok(serde_json::json!({"loggedOut": true})).into_response();
    clear_auth_cookies(&mut response, state.config().cookie_secure)?;
    Ok(response)
}

async fn forgot_password(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ForgotPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    let email = req.email.trim().to_lowercase();
    let user = state
        .run_store_task("auth.forgot_password.get_user_by_email", move |store| {
            store.get_user_by_email(&email)
        })
        .await??;

    // 始终生成并 hash token,使两条分支的 CPU 工作量一致;真正的事务写入移出同步响应路径
    // (detached spawn),让响应延迟不依赖账号是否存在,关闭枚举时序 oracle。
    let raw_token = uuid::Uuid::new_v4().simple().to_string();
    let token_hash = hash_token(&raw_token);
    if let Some(user) = user {
        let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();

        // 仅通过日志输出 token，绝不在响应中返回
        tracing::trace!(
            token_prefix = %&raw_token[..8],
            "Password reset token generated (dev diagnostics only)"
        );
        tracing::info!(
            email = %mask_email_for_log(&user.email),
            "Password reset requested; email delivery disabled in trimmed build"
        );

        let state = state.clone();
        tokio::spawn(async move {
            match state
                .run_store_task("auth.forgot_password.create_token", move |store| {
                    store.create_password_reset_token(&token_hash, &user.id, &expires_at)
                })
                .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "Password reset token creation failed")
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Password reset token creation task failed")
                }
            }
        });
    }

    Ok(ok(serde_json::json!({
        "emailSent": true,
        "message": "如果该邮箱已注册，将会发送密码重置链接",
    })))
}

async fn reset_password(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ResetPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Err(msg) = validate_password(&req.new_password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }

    let token_hash = hash_token(&req.token);

    // 先做非破坏性校验（get 而非 delete）：令牌存在且未过期。哈希可能瞬态失败（panic/runtime
    // shutdown），若此处就消费一次性令牌会白白作废它，逼用户重走 forgot-password。
    let lookup_hash = token_hash.clone();
    let entry = state
        .run_store_task("auth.reset_password.get_token", move |store| {
            store.get_password_reset_token(&lookup_hash)
        })
        .await??
        .ok_or_else(|| AppError::bad_request("AUTH_INVALID_RESET_TOKEN", "重置令牌无效"))?;

    let expires_at =
        chrono::DateTime::parse_from_rfc3339(entry["expires_at"].as_str().unwrap_or_default())
            .map_err(|e| AppError::internal(&format!("reset token expires_at parse error: {e}")))?;

    if expires_at <= Utc::now() {
        return Err(AppError::bad_request(
            "AUTH_EXPIRED_RESET_TOKEN",
            "重置令牌已过期",
        ));
    }

    let user_id = entry["user_id"]
        .as_str()
        .ok_or_else(|| AppError::internal("reset token missing user_id"))?;

    let user_id = user_id.to_string();

    // 哈希在消费令牌之前完成：哈希失败时令牌仍未删除，用户可原样重试。
    let new_password = req.new_password;
    let new_hash = crate::blocking::run_blocking("auth.reset_password.hash_password", move || {
        hash_password(&new_password)
    })
    .await??;

    // 哈希成功后再以 DELETE...RETURNING 原子消费令牌，保证一次性；并发已被取走则视为无效。
    let take_hash = token_hash.clone();
    state
        .run_store_task("auth.reset_password.take_token", move |store| {
            store.take_password_reset_token(&take_hash)
        })
        .await??
        .ok_or_else(|| AppError::bad_request("AUTH_INVALID_RESET_TOKEN", "重置令牌无效"))?;
    // 成功重置密码后清除锁定状态：被 5 次失败登录锁定的用户正是走重置流程恢复账号，
    // 若不清零会让其凭新密码仍被 ACCOUNT_LOCKED 挡到 15 分钟锁定窗口自然过期。
    // 字段级更新：只写 password_hash + 清锁定，避免陈旧整行快照覆盖并发封禁等状态。
    let uid_for_update = user_id.clone();
    match state
        .run_store_task(
            "auth.reset_password.update_user",
            move |store| -> Result<(), AppError> {
                store.update_user_password_clear_lockout(&uid_for_update, &new_hash)?;
                let _ = store.delete_user_sessions(&uid_for_update);
                Ok(())
            },
        )
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(e) => return Err(e.into()),
    }

    Ok(ok(serde_json::json!({})))
}

async fn verify_reset_token(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<VerifyResetTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let token_hash = hash_token(&req.token);

    // 不存在的 token 不抛 400，而是 200 {valid:false} —— 与客户端 VerifyResetTokenResponse 类型契约对齐。
    let entry = match state
        .run_store_task("auth.verify_reset_token.get_token", move |store| {
            store.get_password_reset_token(&token_hash)
        })
        .await??
    {
        Some(entry) => entry,
        None => return Ok(ok(serde_json::json!({"valid": false}))),
    };

    let expires_at =
        chrono::DateTime::parse_from_rfc3339(entry["expires_at"].as_str().unwrap_or_default())
            .map_err(|e| AppError::internal(&format!("reset token expires_at parse error: {e}")))?;

    if expires_at <= Utc::now() {
        return Ok(ok(serde_json::json!({"valid": false})));
    }

    Ok(ok(serde_json::json!({"valid": true})))
}

fn cookie_same_site_flags(secure: bool) -> &'static str {
    if secure {
        "SameSite=None; Secure"
    } else {
        "SameSite=Lax"
    }
}

fn set_token_cookie(
    response: &mut Response,
    token: &str,
    max_age_secs: u64,
    secure: bool,
) -> Result<(), AppError> {
    let flags = cookie_same_site_flags(secure);
    let cookie = format!("token={token}; Path=/; Max-Age={max_age_secs}; {flags}; HttpOnly");
    append_set_cookie(response, &cookie, "token cookie set failed")?;
    Ok(())
}

fn set_refresh_token_cookie(
    response: &mut Response,
    refresh_token: &str,
    max_age_secs: u64,
    secure: bool,
) -> Result<(), AppError> {
    let flags = cookie_same_site_flags(secure);
    let cookie =
        format!("refresh_token={refresh_token}; Path=/; Max-Age={max_age_secs}; {flags}; HttpOnly");
    append_set_cookie(response, &cookie, "refresh token cookie set failed")?;
    Ok(())
}

fn clear_auth_cookies(response: &mut Response, secure: bool) -> Result<(), AppError> {
    let flags = cookie_same_site_flags(secure);
    append_set_cookie(
        response,
        &format!("token=; Path=/; Max-Age=0; {flags}; HttpOnly"),
        "token cookie clear failed",
    )?;
    append_set_cookie(
        response,
        &format!("refresh_token=; Path=/; Max-Age=0; {flags}; HttpOnly"),
        "refresh token cookie clear failed",
    )?;
    Ok(())
}

fn append_set_cookie(
    response: &mut Response,
    cookie: &str,
    error_context: &str,
) -> Result<(), AppError> {
    let value = HeaderValue::from_str(cookie)
        .map_err(|e| AppError::internal(&format!("{error_context}: {e}")))?;
    response.headers_mut().append(SET_COOKIE, value);
    Ok(())
}

fn mask_email_for_log(email: &str) -> String {
    let trimmed = email.trim();
    let Some((local, domain)) = trimmed.split_once('@') else {
        return "***".to_string();
    };

    let local_mask = local
        .chars()
        .next()
        .map(|ch| format!("{ch}***"))
        .unwrap_or_else(|| "***".to_string());
    let domain_mask = domain
        .chars()
        .next()
        .map(|ch| format!("{ch}***"))
        .unwrap_or_else(|| "***".to_string());

    format!("{local_mask}@{domain_mask}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_email_for_log_redacts_sensitive_parts() {
        assert_eq!(mask_email_for_log("alice@example.com"), "a***@e***");
        assert_eq!(mask_email_for_log("x@b.com"), "x***@b***");
        assert_eq!(mask_email_for_log("invalid-email"), "***");
    }
}
