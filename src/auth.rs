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
    "$argon2id$v=19$m=19456,t=2,p=1$ZHVtbXlzYWx0ZHVtbXk$YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY".to_string()
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
    .map_err(|_| AppError::unauthorized("Invalid or expired token"))
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

    Err(AppError::unauthorized("Missing bearer token"))
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

    Err(AppError::unauthorized("Missing refresh token"))
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
            return Err(AppError::unauthorized("Invalid token type"));
        }

        let token_hash = hash_token(&token);
        let session = app_state
            .store()
            .get_session(&token_hash)?
            .ok_or_else(|| AppError::unauthorized("Session not found or expired"))?;

        if session.user_id != claims.sub {
            return Err(AppError::unauthorized("Session mismatch"));
        }

        let user = app_state
            .store()
            .get_user_by_id(&claims.sub)?
            .ok_or_else(|| AppError::unauthorized("User not found"))?;

        if user.is_banned {
            return Err(AppError::forbidden("User is banned"));
        }

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
            return Err(AppError::unauthorized("Invalid token type"));
        }

        let token_hash = hash_token(&token);
        let session = app_state
            .store()
            .get_admin_session(&token_hash)?
            .ok_or_else(|| AppError::unauthorized("Admin session not found or expired"))?;

        if session.user_id != claims.sub {
            return Err(AppError::unauthorized("Admin session mismatch"));
        }

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
}
