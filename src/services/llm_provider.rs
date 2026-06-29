use serde::{Deserialize, Serialize};

use crate::config::LLMConfig;

/// LLM 客户端：以 DeepSeek（OpenAI 兼容协议）为目标实现，
/// 通过 LLM_API_URL / LLM_MODEL 也可指向其它 OpenAI 兼容 endpoint。
#[derive(Debug, Clone)]
pub struct LlmProvider {
    config: LLMConfig,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub json_object: bool,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub usage: ChatUsage,
    pub model: String,
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

    /// Validate LLM configuration at startup（保留向后兼容 API）。
    /// real LLM 实现已落地，启用 + 非 mock 时要求 api_key 非空。
    pub fn validate_config(config: &LLMConfig) {
        if config.enabled && !config.mock && config.api_key.is_empty() {
            tracing::warn!("LLM enabled=true mock=false 但 LLM_API_KEY 为空；请求将失败");
        }
    }

    pub fn config(&self) -> &LLMConfig {
        &self.config
    }

    /// 估算单次调用成本 USD：input × price_in / 1M + output × price_out / 1M
    pub fn estimate_cost_usd(&self, usage: &ChatUsage) -> f64 {
        let p_in = self.config.input_price_per_mtok_usd;
        let p_out = self.config.output_price_per_mtok_usd;
        (usage.prompt_tokens as f64) * p_in / 1_000_000.0
            + (usage.completion_tokens as f64) * p_out / 1_000_000.0
    }

    /// 通用 chat。返回 content + usage。
    pub async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        if !self.config.enabled {
            return Err(LlmError::Disabled);
        }
        if self.config.mock {
            // mock 模式：返回固定 JSON，便于 advisor 端到端联调
            let usage = ChatUsage {
                prompt_tokens: req
                    .messages
                    .iter()
                    .map(|m| (m.content.len() / 4) as u64)
                    .sum(),
                completion_tokens: 64,
                total_tokens: 0,
            };
            let body = if req.json_object {
                r#"{"patch":{"memoryModel.baseDesiredRetention":0.88},"rationale":"mock：近 7 天 ICI 偏高，建议降低目标留存以拉长间隔；这是 mock 输出","confidence":0.7}"#
            } else {
                "Mock LLM response"
            };
            return Ok(ChatResponse {
                content: body.to_string(),
                usage: ChatUsage {
                    total_tokens: usage.prompt_tokens + usage.completion_tokens,
                    ..usage
                },
                model: format!("{}-mock", self.config.model),
            });
        }

        if self.config.api_key.is_empty() {
            return Err(LlmError::ApiError {
                status: 401,
                message: "LLM_API_KEY 未设置".into(),
            });
        }

        let url = format!(
            "{}/v1/chat/completions",
            self.config.api_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": self.config.model,
            "messages": req.messages,
            "temperature": req.temperature,
            "response_format": if req.json_object { serde_json::json!({"type": "json_object"}) } else { serde_json::json!({"type": "text"}) },
        });

        // 记录请求起点,用于计算 latency 并进 P1 日志管线(target=llm)
        let started = std::time::Instant::now();
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                let latency_ms = started.elapsed().as_millis();
                if e.is_timeout() {
                    tracing::warn!(target: "llm", model = %self.config.model, latency_ms, "llm chat timeout");
                    LlmError::Timeout
                } else {
                    tracing::warn!(target: "llm", model = %self.config.model, latency_ms, error = %e, "llm chat network error");
                    LlmError::Network(e.to_string())
                }
            })?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: text,
            });
        }

        // OpenAI 兼容响应：choices[0].message.content + usage
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            // 按字符截断,避免 200 字节落在多字节 UTF-8 字符中间 panic（上游可能返中文/emoji 错误体）。
            let body: String = text.chars().take(200).collect();
            LlmError::Network(format!("parse: {e}; body: {body}"))
        })?;
        let content = parsed
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| LlmError::Network("响应缺 choices[0].message.content".into()))?
            .to_string();
        let usage = parsed
            .get("usage")
            .and_then(|u| serde_json::from_value::<ChatUsage>(u.clone()).ok())
            .unwrap_or(ChatUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            });
        let model = parsed
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.config.model)
            .to_string();

        // 调用成功:发结构化日志进 P1 管线,日志页可按 target=llm 过滤
        let latency_ms = started.elapsed().as_millis();
        tracing::info!(
            target: "llm",
            model = %model,
            prompt_tokens = usage.prompt_tokens,
            completion_tokens = usage.completion_tokens,
            total_tokens = usage.total_tokens,
            cost_usd = self.estimate_cost_usd(&usage),
            latency_ms,
            "llm chat done"
        );

        Ok(ChatResponse {
            content,
            usage,
            model,
        })
    }
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

    fn cfg(enabled: bool, mock: bool) -> LLMConfig {
        LLMConfig {
            enabled,
            mock,
            api_url: "https://api.deepseek.com".into(),
            api_key: String::new(),
            model: "deepseek-chat".into(),
            timeout_secs: 1,
            daily_cost_cap_usd: 1.0,
            input_price_per_mtok_usd: 0.55,
            output_price_per_mtok_usd: 2.19,
            max_cost_per_month_yuan: 100.0,
            usd_to_cny_rate: 7.3,
        }
    }

    #[tokio::test]
    async fn disabled_mode_returns_error() {
        let provider = LlmProvider::new(&cfg(false, true));
        let r = provider
            .chat(ChatRequest {
                messages: vec![],
                json_object: false,
                temperature: 0.0,
            })
            .await;
        assert!(matches!(r, Err(LlmError::Disabled)));
    }

    #[tokio::test]
    async fn mock_text_response() {
        let provider = LlmProvider::new(&cfg(true, true));
        let r = provider
            .chat(ChatRequest {
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: "hi".into(),
                }],
                json_object: false,
                temperature: 0.0,
            })
            .await
            .unwrap();
        assert_eq!(r.content, "Mock LLM response");
    }

    #[tokio::test]
    async fn mock_json_response_is_parseable() {
        let provider = LlmProvider::new(&cfg(true, true));
        let r = provider
            .chat(ChatRequest {
                messages: vec![],
                json_object: true,
                temperature: 0.0,
            })
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        assert!(v.get("patch").is_some());
        assert!(v.get("rationale").is_some());
        assert!(v.get("confidence").is_some());
    }

    #[test]
    fn cost_estimation() {
        let p = LlmProvider::new(&cfg(true, true));
        let cost = p.estimate_cost_usd(&ChatUsage {
            prompt_tokens: 1_000_000,
            completion_tokens: 1_000_000,
            total_tokens: 2_000_000,
        });
        assert!((cost - (0.55 + 2.19)).abs() < 1e-9);
    }

    #[test]
    fn enabled_real_without_key_warns_but_does_not_panic() {
        // 旧实现会 panic；新实现仅 warn
        let c = cfg(true, false);
        LlmProvider::validate_config(&c); // 不应 panic
    }
}
