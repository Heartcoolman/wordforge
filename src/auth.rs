use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::{request::Parts, HeaderMap};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::response::AppError;
use crate::state::AppState;

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|v| v.to_string())
        .map_err(|e| AppError::internal(&format!("password hash failed: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| AppError::internal(&format!("invalid password hash: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Pre-computed argon2 hash for timing-attack prevention.
/// Used when the requested account doesn't exist so that the response time
/// is indistinguishable from a real password verification.
pub fn generate_dummy_argon2_hash() -> String {
    "$argon2id$v=19$m=19456,t=2,p=1$ZHVtbXlzYWx0ZHVtbXk$YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY"
        .to_string()
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub token_type: String,
    pub iat: i64,
    pub exp: i64,
    #[serde(default)]
    pub jti: String,
}

pub fn sign_jwt_for_user(
    user_id: &str,
    secret: &str,
    expires_in_hours: u64,
) -> Result<String, AppError> {
    sign_jwt(user_id, "user", secret, expires_in_hours)
}

/// Refresh tokens use a dedicated secret and independent expiry
/// with a distinct `token_type` so they cannot be used as access tokens.
pub fn sign_refresh_token_for_user(
    user_id: &str,
    secret: &str,
    refresh_expires_in_hours: u64,
) -> Result<String, AppError> {
    sign_jwt(user_id, "refresh", secret, refresh_expires_in_hours)
}

pub fn sign_jwt_for_admin(
    admin_id: &str,
    secret: &str,
    expires_in_hours: u64,
) -> Result<String, AppError> {
    sign_jwt(admin_id, "admin", secret, expires_in_hours)
}

fn sign_jwt(
    subject_id: &str,
    token_type: &str,
    secret: &str,
    expires_in_hours: u64,
) -> Result<String, AppError> {
    if expires_in_hours == 0 {
        return Err(AppError::internal("JWT expiry cannot be zero hours"));
    }
    let now = Utc::now();
    let exp = now + Duration::hours(expires_in_hours as i64);
    let claims = Claims {
        sub: subject_id.to_string(),
        token_type: token_type.to_string(),
        iat: now.timestamp(),
        exp: exp.timestamp(),
        jti: uuid::Uuid::new_v4().to_string(),
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::internal(&format!("jwt sign failed: {e}")))
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims, AppError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.algorithms = vec![Algorithm::HS256];

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::unauthorized("令牌无效或已过期"))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|auth_header| auth_header.strip_prefix("Bearer "))
        .map(|token| token.trim().to_string())
}

fn extract_cookie_token(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookie| {
            cookie.split(';').find_map(|part| {
                let p = part.trim();
                p.strip_prefix(&format!("{cookie_name}="))
                    .map(str::to_string)
            })
        })
}

pub fn extract_token_from_headers(headers: &HeaderMap) -> Result<String, AppError> {
    if let Some(token) = extract_bearer_token(headers) {
        return Ok(token);
    }

    if let Some(token) = extract_cookie_token(headers, "token") {
        return Ok(token);
    }

    Err(AppError::unauthorized("缺少认证令牌"))
}

pub fn extract_refresh_token_from_headers(headers: &HeaderMap) -> Result<String, AppError> {
    if let Some(token) = extract_bearer_token(headers) {
        return Ok(token);
    }

    if let Some(token) = extract_cookie_token(headers, "refresh_token") {
        return Ok(token);
    }

    if let Some(token) = extract_cookie_token(headers, "token") {
        return Ok(token);
    }

    Err(AppError::unauthorized("缺少刷新令牌"))
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
}

#[derive(Debug, Clone)]
pub struct AdminAuthUser {
    pub admin_id: String,
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let token = extract_token_from_headers(&parts.headers)?;
        let claims = verify_jwt(&token, &app_state.config().jwt_secret)?;

        if claims.token_type != "user" {
            return Err(AppError::unauthorized("令牌类型无效"));
        }

        let token_hash = hash_token(&token);
        let sub = claims.sub.clone();
        app_state
            .run_store_task("auth.load_user", move |store| -> Result<(), AppError> {
                let session = store
                    .get_session(&token_hash)?
                    .ok_or_else(|| AppError::unauthorized("会话不存在或已过期"))?;

                if session.user_id != sub {
                    return Err(AppError::unauthorized("会话不匹配"));
                }

                let user = store
                    .get_user_by_id(&sub)?
                    .ok_or_else(|| AppError::unauthorized("用户不存在"))?;

                if user.is_banned {
                    return Err(AppError::forbidden("用户已被封禁"));
                }
                Ok(())
            })
            .await??;

        Ok(AuthUser {
            user_id: claims.sub,
        })
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for AdminAuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let token = extract_token_from_headers(&parts.headers)?;
        let claims = verify_jwt(&token, &app_state.config().admin_jwt_secret)?;

        if claims.token_type != "admin" {
            return Err(AppError::unauthorized("令牌类型无效"));
        }

        let token_hash = hash_token(&token);
        let sub = claims.sub.clone();
        app_state
            .run_store_task("auth.load_admin", move |store| -> Result<(), AppError> {
                let session = store
                    .get_admin_session(&token_hash)?
                    .ok_or_else(|| AppError::unauthorized("管理员会话不存在或已过期"))?;

                if session.user_id != sub {
                    return Err(AppError::unauthorized("管理员会话不匹配"));
                }
                // 注：B3 审计提议在此添加 admin ban 检查（与 AuthUser 对齐），但 `admins` 表
                // 当前 schema 不含 `is_banned` 字段（cols: id, email, password_hash, created_at,
                // updated_at, failed_login_count, locked_until — 见 store/operations/admins.rs:9-10）。
                // 管理员"禁用"目前靠两条独立机制：
                //   1) `locked_until` 字段在登录失败次数超限后阻断登录（store/operations/admins.rs:225）；
                //   2) 显式 `delete_admin_session` 撤销当前 token。
                // 真正的"管理员禁用"语义需要 schema 迁移加 `is_disabled` 列 + 对应 admin 路由，
                // 超出 v1.1 P0 范围。这里保留 session-only 校验，明确不做用户表跨查（之前的实现
                // 把 admin_id 当 user_id 去 users 表查 is_banned，会把所有正常 admin 拒之门外）。
                Ok(())
            })
            .await??;

        Ok(AdminAuthUser {
            admin_id: claims.sub,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_and_verify() {
        let hash = hash_password("Passw0rd!").unwrap();
        assert!(verify_password("Passw0rd!", &hash).unwrap());
        assert!(!verify_password("bad", &hash).unwrap());
    }

    #[test]
    fn jwt_sign_and_verify() {
        let secret = "secret";
        let token = sign_jwt_for_user("u1", secret, 1).unwrap();
        let claims = verify_jwt(&token, secret).unwrap();
        assert_eq!(claims.sub, "u1");
        assert_eq!(claims.token_type, "user");
    }

    #[test]
    fn token_hash_is_stable() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
    }

    #[test]
    fn dummy_argon2_hash_is_constant() {
        let a = generate_dummy_argon2_hash();
        let b = generate_dummy_argon2_hash();
        assert_eq!(a, b);
        assert!(a.starts_with("$argon2id$"));
    }

    #[test]
    fn sign_jwt_zero_hours_returns_error() {
        let err = sign_jwt_for_user("u", "secret", 0).unwrap_err();
        // AppError Debug 应包含 internal 描述
        let dbg = format!("{:?}", err);
        assert!(dbg.contains("JWT") || dbg.contains("jwt") || dbg.contains("expiry"));
    }

    #[test]
    fn verify_jwt_rejects_wrong_secret() {
        let token = sign_jwt_for_user("u1", "secret-a", 1).unwrap();
        let err = verify_jwt(&token, "secret-b").unwrap_err();
        let dbg = format!("{:?}", err);
        assert!(dbg.contains("无效") || dbg.contains("Unauthorized") || dbg.contains("过期"));
    }

    #[test]
    fn verify_jwt_rejects_garbage() {
        assert!(verify_jwt("not.a.jwt", "secret").is_err());
    }

    #[test]
    fn verify_password_invalid_hash_returns_internal_error() {
        let err = verify_password("anything", "not-an-argon2-hash").unwrap_err();
        let dbg = format!("{:?}", err);
        assert!(dbg.contains("hash") || dbg.contains("invalid"));
    }

    #[test]
    fn refresh_token_uses_distinct_type() {
        let secret = "another-secret-for-refresh-tokens";
        let token = sign_refresh_token_for_user("u1", secret, 24).unwrap();
        let claims = verify_jwt(&token, secret).unwrap();
        assert_eq!(claims.token_type, "refresh");
        assert!(!claims.jti.is_empty());
    }

    #[test]
    fn extract_bearer_token_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer my-token".parse().unwrap(),
        );
        assert_eq!(extract_token_from_headers(&headers).unwrap(), "my-token");
    }

    #[test]
    fn extract_token_from_cookie_when_bearer_missing() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "foo=bar; token=cookie-token; baz=qux".parse().unwrap(),
        );
        assert_eq!(
            extract_token_from_headers(&headers).unwrap(),
            "cookie-token"
        );
    }

    #[test]
    fn extract_token_from_headers_returns_err_when_absent() {
        let headers = HeaderMap::new();
        assert!(extract_token_from_headers(&headers).is_err());
    }

    #[test]
    fn extract_refresh_token_falls_back_through_cookies() {
        // bearer 缺失，refresh_token cookie 命中
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "refresh_token=r-cookie; other=x".parse().unwrap(),
        );
        assert_eq!(
            extract_refresh_token_from_headers(&headers).unwrap(),
            "r-cookie"
        );

        // 仅 token cookie，应回退使用之
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "token=access-cookie".parse().unwrap(),
        );
        assert_eq!(
            extract_refresh_token_from_headers(&headers).unwrap(),
            "access-cookie"
        );

        // 无任何 token
        let headers = HeaderMap::new();
        assert!(extract_refresh_token_from_headers(&headers).is_err());
    }

    #[test]
    fn extract_bearer_ignores_non_bearer_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Basic abc123".parse().unwrap(),
        );
        // 应该穿透到 cookie，cookie 也没有，最终 Err
        assert!(extract_token_from_headers(&headers).is_err());
    }
}
