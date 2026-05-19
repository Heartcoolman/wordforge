use std::sync::Arc;

use chrono::{Duration, Utc};
use serde::Serialize;

use crate::auth::{hash_password, hash_token};
use crate::blocking;
use crate::response::AppError;
use crate::store::operations::users::User;
use crate::store::Store;

/// Shared admin service entry point for transaction-oriented admin workflows.
#[derive(Clone)]
pub struct AdminService {
    store: Arc<Store>,
}

impl AdminService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub async fn list_users(
        &self,
        page: u64,
        per_page: u64,
        search: Option<String>,
        banned: Option<bool>,
    ) -> Result<(Vec<User>, u64), AppError> {
        let store = self.store.clone();
        let limit = per_page as usize;
        let offset = ((page - 1) * per_page) as usize;
        let has_filter = search.is_some() || banned.is_some();

        blocking::run_blocking("admin_service.list_users", move || {
            if has_filter {
                let mut all = store.list_users(10_000, 0)?;
                all.retain(|user| {
                    let banned_match = banned.map(|v| user.is_banned == v).unwrap_or(true);
                    let search_match = search.as_ref().map_or(true, |needle| {
                        user.username.to_ascii_lowercase().contains(needle)
                            || user.email.to_ascii_lowercase().contains(needle)
                    });
                    banned_match && search_match
                });
                let total = all.len() as u64;
                let page_slice = all.into_iter().skip(offset).take(limit).collect();
                Ok::<_, crate::store::StoreError>((page_slice, total))
            } else {
                let page_slice = store.list_users(limit, offset)?;
                let total = store.count_users()? as u64;
                Ok((page_slice, total))
            }
        })
        .await?
        .map_err(AppError::from)
    }

    pub async fn ban_user(&self, user_id: String) -> Result<u32, AppError> {
        let store = self.store.clone();
        blocking::run_blocking("admin_service.ban_user", move || -> Result<_, AppError> {
            if store.get_user_by_id(&user_id)?.is_none() {
                return Err(AppError::not_found("用户不存在"));
            }
            store.ban_user(&user_id)?;
            Ok(store.delete_user_sessions(&user_id)?)
        })
        .await?
    }

    pub async fn unban_user(&self, user_id: String) -> Result<(), AppError> {
        let store = self.store.clone();
        blocking::run_blocking(
            "admin_service.unban_user",
            move || -> Result<_, AppError> {
                if store.get_user_by_id(&user_id)?.is_none() {
                    return Err(AppError::not_found("用户不存在"));
                }
                store.unban_user(&user_id)?;
                Ok(())
            },
        )
        .await?
    }

    pub async fn stats(&self) -> Result<AdminStats, AppError> {
        let store = self.store.clone();
        let today = Utc::now().date_naive();
        let today_str = today.format("%Y-%m-%d").to_string();
        let yesterday_str = (today - Duration::days(1)).format("%Y-%m-%d").to_string();

        let counts = blocking::run_blocking("admin_service.stats", move || {
            Ok::<_, crate::store::StoreError>(AdminStatsCounts {
                users: store.count_users()?,
                words: store.count_words()?,
                records: store.count_all_records()?,
                users_today: store.count_users_registered_on_date(&today_str)?,
                users_yesterday: store.count_users_registered_on_date(&yesterday_str)?,
                records_today: store.count_records_on_date(&today_str)?,
                records_yesterday: store.count_records_on_date(&yesterday_str)?,
            })
        })
        .await??;

        Ok(AdminStats {
            users: counts.users,
            words: counts.words,
            records: counts.records,
            trend: AdminStatsTrend {
                users: TrendValue {
                    value: calc_trend(counts.users_today, counts.users_yesterday),
                    label: "较昨日",
                },
                records: TrendValue {
                    value: calc_trend(counts.records_today, counts.records_yesterday),
                    label: "较昨日",
                },
            },
        })
    }

    pub async fn create_password_reset(
        &self,
        user_id: String,
    ) -> Result<PasswordResetKey, AppError> {
        let store = self.store.clone();
        let raw_token = uuid::Uuid::new_v4().simple().to_string();
        let token_hash = hash_token(&raw_token);
        let expires_in_hours = 4;
        let expires_at = (Utc::now() + Duration::hours(expires_in_hours)).to_rfc3339();

        blocking::run_blocking(
            "admin_service.create_password_reset",
            move || -> Result<_, AppError> {
                if store.get_user_by_id(&user_id)?.is_none() {
                    return Err(AppError::not_found("用户不存在"));
                }
                store
                    .create_password_reset_token(&token_hash, &user_id, &expires_at)
                    .map_err(|e| AppError::internal(&e.to_string()))?;
                Ok(())
            },
        )
        .await??;

        Ok(PasswordResetKey {
            reset_key: raw_token,
            expires_in_hours: expires_in_hours as u8,
        })
    }

    pub async fn set_user_password(
        &self,
        user_id: String,
        new_password: String,
    ) -> Result<u32, AppError> {
        let store = self.store.clone();
        blocking::run_blocking(
            "admin_service.set_user_password",
            move || -> Result<_, AppError> {
                let mut user = store
                    .get_user_by_id(&user_id)?
                    .ok_or_else(|| AppError::not_found("用户不存在"))?;

                user.password_hash = hash_password(&new_password)?;
                user.updated_at = Utc::now();
                store.update_user(&user)?;

                Ok(store.delete_user_sessions(&user_id)?)
            },
        )
        .await?
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminStats {
    pub users: usize,
    pub words: u64,
    pub records: usize,
    pub trend: AdminStatsTrend,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminStatsTrend {
    pub users: TrendValue,
    pub records: TrendValue,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendValue {
    pub value: i64,
    pub label: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordResetKey {
    pub reset_key: String,
    pub expires_in_hours: u8,
}

struct AdminStatsCounts {
    users: usize,
    words: u64,
    records: usize,
    users_today: usize,
    users_yesterday: usize,
    records_today: usize,
    records_yesterday: usize,
}

fn calc_trend(today: usize, yesterday: usize) -> i64 {
    if yesterday == 0 {
        return 0;
    }
    ((today as f64 - yesterday as f64) / yesterday as f64 * 100.0).round() as i64
}
