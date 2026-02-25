pub mod anthropic;
pub mod openai;
pub mod openai_responses;

use std::time::Duration;

use crate::config::ProfileConfig;
use crate::defaults;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role:    String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: String)      -> Self { Self { role: "user".into(), content } }
    pub fn assistant(content: String) -> Self { Self { role: "assistant".into(), content } }
    pub fn system(content: String)    -> Self { Self { role: "system".into(), content } }
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens:  Option<u32>,
    pub output_tokens: Option<u32>,
}

pub struct ProviderResponse {
    pub content: String,
    pub model:   String,
    pub usage:   Option<Usage>,
}

pub trait Provider: Send + Sync {
    fn chat(
        &self,
        messages: &[ChatMessage],
        stream:   bool,
        verbose:  bool,
        on_chunk: Option<&dyn Fn(&str)>,
    ) -> anyhow::Result<ProviderResponse>;

    fn list_models(&self) -> anyhow::Result<Vec<String>>;

    fn endpoint(&self) -> String;

    fn request_json(&self, messages: &[ChatMessage], stream: bool) -> serde_json::Value;

    fn dry_run_output(&self, messages: &[ChatMessage], stream: bool) -> String {
        let mut out = format!("=== dry run — request not sent ===\n\nPOST {}\n", self.endpoint());
        for msg in messages {
            out.push_str(&format!("\n[{}]\n{}\n", msg.role, msg.content));
        }
        out.push_str("\n--- JSON payload ---\n");
        out.push_str(&serde_json::to_string_pretty(&self.request_json(messages, stream)).unwrap_or_default());
        out
    }
}

pub fn make_provider(cfg: &ProfileConfig, timeout: Duration) -> anyhow::Result<Box<dyn Provider>> {
    let api_key = cfg.api_key.clone().unwrap_or_default();
    match cfg.provider_type.as_str() {
        "anthropic" => {
            let d = defaults::find("anthropic").expect("anthropic in providers.toml");
            let base_url = cfg.base_url.clone().unwrap_or_else(|| d.base_url.clone());
            Ok(Box::new(anthropic::AnthropicProvider::new(
                &base_url, &api_key, cfg.model.clone(), timeout,
            )))
        }
        "openai-responses" => {
            let d = defaults::find("openai-responses").expect("openai-responses in providers.toml");
            let base_url = cfg.base_url.clone().unwrap_or_else(|| d.base_url.clone());
            Ok(Box::new(openai_responses::OpenAIResponsesProvider::new(
                &base_url, &api_key, cfg.model.clone(), timeout,
            )))
        }
        _ => {
            let d = defaults::find("openai").expect("openai in providers.toml");
            let base_url = cfg.base_url.clone().unwrap_or_else(|| d.base_url.clone());
            Ok(Box::new(openai::OpenAIProvider::new(
                &base_url, &api_key, cfg.model.clone(), timeout,
            )))
        }
    }
}

pub fn mask_key(key: &str) -> String {
    if key.len() <= 8 { return "***".into(); }
    format!("***...{}", &key[key.len() - 4..])
}
