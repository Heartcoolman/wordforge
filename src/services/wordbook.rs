use std::sync::Arc;

use crate::blocking;
use crate::response::AppError;
use crate::store::Store;

/// Service boundary for wordbook import, sync, and local catalog workflows.
#[derive(Clone)]
pub struct WordbookService {
    store: Arc<Store>,
}

impl WordbookService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub async fn user_center_url(&self, user_id: &str) -> Result<Option<String>, AppError> {
        let store = self.store.clone();
        let user_id = user_id.to_string();

        blocking::run_blocking(
            "wordbook_service.user_center_url",
            move || -> Result<_, AppError> {
                match store.get_user_preferences(&user_id)? {
                    Some(prefs) => Ok(prefs
                        .get("wordbook_center_url")
                        .or_else(|| prefs.get("wordbookCenterUrl"))
                        .and_then(|value| value.as_str())
                        .filter(|url| !url.is_empty())
                        .map(str::to_string)),
                    None => Ok(None),
                }
            },
        )
        .await?
    }

    pub async fn set_user_center_url(
        &self,
        user_id: &str,
        url: Option<&str>,
    ) -> Result<(), AppError> {
        let store = self.store.clone();
        let user_id = user_id.to_string();
        let url = url.map(str::to_string);

        blocking::run_blocking(
            "wordbook_service.set_user_center_url",
            move || -> Result<_, AppError> {
                let mut prefs = store
                    .get_user_preferences(&user_id)?
                    .unwrap_or(serde_json::json!({}));

                if let Some(obj) = prefs.as_object_mut() {
                    match url.as_deref() {
                        Some(url) if !url.is_empty() => {
                            obj.insert(
                                "wordbook_center_url".to_string(),
                                serde_json::Value::String(url.to_string()),
                            );
                        }
                        _ => {
                            obj.remove("wordbook_center_url");
                        }
                    }
                }

                store.set_user_preferences(&user_id, &prefs)?;
                Ok(())
            },
        )
        .await?
    }
}
