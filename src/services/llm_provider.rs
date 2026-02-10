use serde::{Deserialize, Serialize};

use crate::config::LLMConfig;

#[derive(Debug, Clone)]
pub struct LlmProvider {
    config: LLMConfig,
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl LlmProvider {
    pub fn new(config: &LLMConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            config: config.clone(),
            client,
        }
    }

    /// Validate LLM configuration at startup.
    /// Panics if `enabled=true` and `mock=false` since real LLM mode is not yet implemented.
    pub fn validate_config(config: &LLMConfig) {
        if config.enabled && !config.mock {
            panic!(
                "Invalid LLM configuration: enabled=true and mock=false, \
                 but real LLM API integration is not yet implemented. \
                 Set LLM_MOCK=true or LLM_ENABLED=false."
            );
        }
    }

    pub async fn chat(&self, _messages: Vec<ChatMessage>) -> Result<String, LlmError> {
        if !self.config.enabled {
            return Err(LlmError::Disabled);
        }
        if self.config.mock {
            return Ok("Mock LLM response".to_string());
        }

        Err(LlmError::ApiError {
            status: 501,
            message: "Real LLM API integration is not implemented yet".to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("llm is disabled")]
    Disabled,
    #[error("llm request timed out")]
    Timeout,
    #[error("llm network error: {0}")]
    Network(String),
    #[error("llm api error: status={status}, message={message}")]
    ApiError { status: u16, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_mode_returns_error() {
        let cfg = LLMConfig {
            enabled: false,
            mock: true,
            api_url: String::new(),
            api_key: String::new(),
            timeout_secs: 1,
        };
        let provider = LlmProvider::new(&cfg);
        let result = provider.chat(vec![]).await;
        assert!(matches!(result, Err(LlmError::Disabled)));
    }

    #[tokio::test]
    async fn mock_mode_returns_text() {
        let cfg = LLMConfig {
            enabled: true,
            mock: true,
            api_url: String::new(),
            api_key: String::new(),
            timeout_secs: 1,
        };
        let provider = LlmProvider::new(&cfg);
        let result = provider.chat(vec![]).await.unwrap();
        assert_eq!(result, "Mock LLM response");
    }
}
