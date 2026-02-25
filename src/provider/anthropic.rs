use std::io::{BufRead, BufReader};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::{mask_key, ChatMessage, Provider, ProviderResponse, Usage};
use crate::constants::SSE_DATA_PREFIX;
use crate::defaults;

pub struct AnthropicProvider {
    base_url: String,
    api_key:  String,
    model:    String,
    agent:    ureq::Agent,
}

impl AnthropicProvider {
    pub fn new(base_url: &str, api_key: &str, model: String, timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .new_agent();
        Self {
            base_url: base_url.trim_end_matches('/').into(),
            api_key:  api_key.into(),
            model,
            agent,
        }
    }
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model:      &'a str,
    max_tokens: u32,
    messages:   Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system:     Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream:     Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct AnthropicMessage {
    role:    String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    model:   String,
    content: Vec<AnthropicBlock>,
    usage:   Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens:  Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct AnthropicModelsResponse { data: Vec<AnthropicModelInfo> }

#[derive(Deserialize)]
struct AnthropicModelInfo { id: String }

// Streaming
#[derive(Deserialize)]
struct AnthropicEvent {
    #[serde(rename = "type")]
    kind:    String,
    delta:   Option<AnthropicDelta>,
    usage:   Option<AnthropicUsage>,
    message: Option<AnthropicMessageStart>,
}

#[derive(Deserialize)]
struct AnthropicDelta {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicMessageStart {
    model: Option<String>,
    usage: Option<AnthropicUsage>,
}

fn build_request<'a>(model: &'a str, messages: &[ChatMessage], stream: bool, max_tokens: u32) -> AnthropicRequest<'a> {
    let system = messages.iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.clone())
        .collect::<Vec<_>>()
        .join("\n\n");
    AnthropicRequest {
        model,
        max_tokens,
        messages: messages.iter()
            .filter(|m| m.role != "system")
            .map(|m| AnthropicMessage { role: m.role.clone(), content: m.content.clone() })
            .collect(),
        system: if system.is_empty() { None } else { Some(system) },
        stream: if stream { Some(true) } else { None },
    }
}

fn log_verbose_request(url: &str, api_key: &str, body: &AnthropicRequest<'_>) {
    let s = serde_json::to_string_pretty(body).unwrap_or_default();
    eprintln!("> POST {url}");
    eprintln!("> x-api-key: {}", mask_key(api_key));
    for line in s.lines() { eprintln!("> {line}"); }
    eprintln!();
}

fn parse_response(resp: AnthropicResponse) -> ProviderResponse {
    let content = resp.content.into_iter()
        .filter(|b| b.kind == "text")
        .filter_map(|b| b.text)
        .collect::<Vec<_>>()
        .join("");
    let usage = resp.usage.map(|u| Usage {
        input_tokens:  u.input_tokens,
        output_tokens: u.output_tokens,
    });
    ProviderResponse { content, model: resp.model, usage }
}

impl Provider for AnthropicProvider {
    fn chat(
        &self,
        messages: &[ChatMessage],
        stream:   bool,
        verbose:  bool,
        on_chunk: Option<&dyn Fn(&str)>,
    ) -> anyhow::Result<ProviderResponse> {
        let d           = defaults::find("anthropic").expect("anthropic in providers.toml");
        let api_version = d.api_version.as_deref().unwrap_or("2023-06-01");
        let max_tokens  = d.max_tokens.unwrap_or(4096);

        let url  = format!("{}/messages", self.base_url);
        let body = build_request(&self.model, messages, stream, max_tokens);

        if verbose { log_verbose_request(&url, &self.api_key, &body); }

        let mut resp = self.agent
            .post(&url)
            .header("x-api-key",         &self.api_key)
            .header("anthropic-version",  api_version)
            .header("content-type",       "application/json")
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("Anthropic request failed: {e}"))?;

        if verbose { eprintln!("< {}", resp.status()); eprintln!(); }

        if stream {
            parse_stream(resp.body_mut(), &self.model, verbose, on_chunk)
        } else {
            let r: AnthropicResponse = resp.body_mut().read_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse Anthropic response: {e}"))?;
            Ok(parse_response(r))
        }
    }

    fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let d           = defaults::find("anthropic").expect("anthropic in providers.toml");
        let api_version = d.api_version.as_deref().unwrap_or("2023-06-01");
        let url = format!("{}/models", self.base_url);
        let r: AnthropicModelsResponse = self.agent
            .get(&url)
            .header("x-api-key",        &self.api_key)
            .header("anthropic-version", api_version)
            .call()
            .map_err(|e| anyhow::anyhow!("list_models: {e}"))?
            .body_mut()
            .read_json()
            .map_err(|e| anyhow::anyhow!("list_models parse: {e}"))?;
        let mut ids: Vec<_> = r.data.into_iter().map(|m| m.id).collect();
        ids.sort();
        Ok(ids)
    }

    fn endpoint(&self) -> String {
        format!("{}/messages", self.base_url)
    }

    fn request_json(&self, messages: &[ChatMessage], stream: bool) -> serde_json::Value {
        let d = defaults::find("anthropic").expect("anthropic in providers.toml");
        let max_tokens = d.max_tokens.unwrap_or(4096);
        serde_json::to_value(build_request(&self.model, messages, stream, max_tokens)).unwrap_or_default()
    }
}

pub(super) fn parse_stream(
    body:      &mut ureq::Body,
    req_model: &str,
    verbose:   bool,
    on_chunk:  Option<&dyn Fn(&str)>,
) -> anyhow::Result<ProviderResponse> {
    let reader  = BufReader::new(body.as_reader());
    let mut content = String::new();
    let mut model   = req_model.to_string();
    let mut usage: Option<Usage> = None;

    for line in reader.lines() {
        let line = line.context("Stream read error")?;
        let line = line.trim();
        if verbose { eprintln!("< {line}"); }
        if line.is_empty() { continue; }
        let data = match line.strip_prefix(SSE_DATA_PREFIX) { Some(d) => d, None => continue };
        let event: AnthropicEvent = match serde_json::from_str(data) { Ok(e) => e, Err(_) => continue };

        match event.kind.as_str() {
            "message_start" => {
                if let Some(msg) = event.message {
                    if let Some(m) = msg.model { model = m; }
                    if let Some(u) = msg.usage {
                        usage = Some(Usage {
                            input_tokens:  u.input_tokens,
                            output_tokens: u.output_tokens,
                        });
                    }
                }
            }
            "content_block_delta" => {
                if let Some(delta) = event.delta {
                    if let Some(text) = delta.text {
                        if let Some(cb) = on_chunk { cb(&text); }
                        content.push_str(&text);
                    }
                }
            }
            "message_delta" => {
                if let Some(u) = event.usage {
                    usage = Some(Usage {
                        input_tokens:  usage.as_ref().and_then(|u| u.input_tokens),
                        output_tokens: u.output_tokens,
                    });
                }
            }
            "message_stop" => break,
            _ => {}
        }
    }
    Ok(ProviderResponse { content, model, usage })
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;
    use std::time::Duration;

    fn make_provider(server: &MockServer) -> AnthropicProvider {
        AnthropicProvider::new(&server.base_url(), "test-key", "claude-sonnet-4-6".into(), Duration::from_secs(5))
    }

    #[test]
    fn chat_non_streaming_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/messages");
            then.status(200).json_body(json!({
                "model": "claude-sonnet-4-6",
                "content": [{"type": "text", "text": "Hello!"}],
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }));
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("hi".into())], false, false, None).unwrap();
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.model, "claude-sonnet-4-6");
        assert_eq!(resp.usage.as_ref().and_then(|u| u.input_tokens), Some(10));
        assert_eq!(resp.usage.as_ref().and_then(|u| u.output_tokens), Some(5));
    }

    #[test]
    fn chat_extracts_system_message_into_system_field() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/messages")
                .json_body_partial(r#"{"system":"Be helpful"}"#);
            then.status(200).json_body(json!({
                "model": "claude-sonnet-4-6",
                "content": [{"type": "text", "text": "ok"}],
                "usage": null
            }));
        });
        let prov = make_provider(&server);
        let messages = vec![
            ChatMessage::system("Be helpful".into()),
            ChatMessage::user("hi".into()),
        ];
        prov.chat(&messages, false, false, None).unwrap();
        mock.assert();
    }

    #[test]
    fn chat_non_streaming_skips_non_text_blocks() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/messages");
            then.status(200).json_body(json!({
                "model": "claude-sonnet-4-6",
                "content": [
                    {"type": "tool_use", "id": "x"},
                    {"type": "text", "text": "ok"}
                ],
                "usage": null
            }));
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("hi".into())], false, false, None).unwrap();
        assert_eq!(resp.content, "ok");
    }

    #[test]
    fn chat_api_error_returns_err() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/messages");
            then.status(401).body("Unauthorized");
        });
        let prov = make_provider(&server);
        let result = prov.chat(&[ChatMessage::user("hi".into())], false, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn chat_streaming_accumulates_content() {
        let server = MockServer::start();
        let sse_body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-sonnet-4-6\",\"usage\":{\"input_tokens\":5}}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hel\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"lo\"}}\n\n",
            "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        server.mock(|when, then| {
            when.method(POST).path("/messages");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse_body);
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("hi".into())], true, false, None).unwrap();
        assert_eq!(resp.content, "Hello");
        assert_eq!(resp.usage.as_ref().and_then(|u| u.input_tokens), Some(5));
        assert_eq!(resp.usage.as_ref().and_then(|u| u.output_tokens), Some(2));
    }

    #[test]
    fn chat_streaming_calls_on_chunk_callback() {
        let server = MockServer::start();
        let sse_body = concat!(
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hel\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"lo\"}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        server.mock(|when, then| {
            when.method(POST).path("/messages");
            then.status(200).header("content-type", "text/event-stream").body(sse_body);
        });
        let prov = make_provider(&server);
        let chunks = std::sync::Mutex::new(Vec::<String>::new());
        prov.chat(&[ChatMessage::user("hi".into())], true, false, Some(&|chunk| {
            chunks.lock().unwrap().push(chunk.to_string());
        })).unwrap();
        let chunks = chunks.into_inner().unwrap();
        assert_eq!(chunks, vec!["Hel", "lo"]);
    }

    #[test]
    fn list_models_calls_api_and_sorts() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/models");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "claude-sonnet-4-6", "type": "model"},
                    {"id": "claude-opus-4-6",   "type": "model"},
                    {"id": "claude-haiku-4-5",  "type": "model"}
                ],
                "has_more": false
            }));
        });
        let prov = make_provider(&server);
        let models = prov.list_models().unwrap();
        assert_eq!(models, vec!["claude-haiku-4-5", "claude-opus-4-6", "claude-sonnet-4-6"]);
    }

    #[test]
    fn endpoint_returns_messages_url() {
        let server = MockServer::start();
        let prov = make_provider(&server);
        assert!(prov.endpoint().ends_with("/messages"));
    }

    #[test]
    fn request_json_excludes_system_from_messages() {
        let server = MockServer::start();
        let prov = make_provider(&server);
        let messages = vec![
            ChatMessage::system("sys".into()),
            ChatMessage::user("q".into()),
        ];
        let body = prov.request_json(&messages, false);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(body["system"], "sys");
    }
}
