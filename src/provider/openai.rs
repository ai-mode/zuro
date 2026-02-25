use std::io::{BufRead, BufReader};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::{mask_key, ChatMessage, Provider, ProviderResponse, Usage};
use crate::constants::{SSE_DATA_PREFIX, SSE_DONE};

pub struct OpenAIProvider {
    base_url: String,
    api_key:  String,
    model:    String,
    agent:    ureq::Agent,
}

impl OpenAIProvider {
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
struct OAIRequest<'a> {
    model:    &'a str,
    messages: Vec<OAIMessage>,
    stream:   bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Serialize)]
struct StreamOptions { include_usage: bool }

#[derive(Serialize, Deserialize)]
struct OAIMessage {
    role:    String,
    content: String,
}

#[derive(Deserialize)]
struct OAIResponse {
    model:   Option<String>,
    choices: Vec<OAIChoice>,
    usage:   Option<OAIUsage>,
}

#[derive(Deserialize)]
struct OAIChoice {
    message: OAIMessage,
}

#[derive(Deserialize)]
struct OAIUsage {
    prompt_tokens:     Option<u32>,
    completion_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct OAIChunk {
    model:   Option<String>,
    choices: Vec<OAIChunkChoice>,
    usage:   Option<OAIUsage>,
}

#[derive(Deserialize)]
struct OAIChunkChoice {
    delta: OAIDelta,
}

#[derive(Deserialize, Default)]
struct OAIDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct OAIModelsResponse { data: Vec<OAIModel> }
#[derive(Deserialize)]
struct OAIModel { id: String }

fn build_request<'a>(model: &'a str, messages: &[ChatMessage], stream: bool) -> OAIRequest<'a> {
    OAIRequest {
        model,
        messages: messages.iter().map(|m| OAIMessage {
            role:    m.role.clone(),
            content: m.content.clone(),
        }).collect(),
        stream,
        stream_options: if stream { Some(StreamOptions { include_usage: true }) } else { None },
    }
}

fn log_verbose_request(url: &str, api_key: &str, body: &OAIRequest<'_>) {
    let s = serde_json::to_string_pretty(body).unwrap_or_default();
    eprintln!("> POST {url}");
    eprintln!("> Authorization: Bearer {}", mask_key(api_key));
    for line in s.lines() { eprintln!("> {line}"); }
    eprintln!();
}

fn parse_response(resp: OAIResponse, fallback_model: &str) -> anyhow::Result<ProviderResponse> {
    let choice = resp.choices.into_iter().next()
        .ok_or_else(|| anyhow::anyhow!("Empty choices"))?;
    let model = resp.model.unwrap_or_else(|| fallback_model.to_string());
    let usage = resp.usage.map(|u| Usage {
        input_tokens:  u.prompt_tokens,
        output_tokens: u.completion_tokens,
    });
    Ok(ProviderResponse { content: choice.message.content, model, usage })
}

impl Provider for OpenAIProvider {
    fn chat(
        &self,
        messages: &[ChatMessage],
        stream:   bool,
        verbose:  bool,
        on_chunk: Option<&dyn Fn(&str)>,
    ) -> anyhow::Result<ProviderResponse> {
        let url  = format!("{}/chat/completions", self.base_url);
        let body = build_request(&self.model, messages, stream);

        if verbose { log_verbose_request(&url, &self.api_key, &body); }

        let mut resp = self.agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("OpenAI request failed: {e}"))?;

        if verbose { eprintln!("< {}", resp.status()); eprintln!(); }

        if stream {
            parse_stream(resp.body_mut(), &self.model, verbose, on_chunk)
        } else {
            let r: OAIResponse = resp.body_mut().read_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse OpenAI response: {e}"))?;
            parse_response(r, &self.model)
        }
    }

    fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        let r: OAIModelsResponse = self.agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.api_key))
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
        format!("{}/chat/completions", self.base_url)
    }

    fn request_json(&self, messages: &[ChatMessage], stream: bool) -> serde_json::Value {
        serde_json::to_value(build_request(&self.model, messages, stream)).unwrap_or_default()
    }
}

fn parse_stream(
    body:      &mut ureq::Body,
    req_model: &str,
    verbose:   bool,
    on_chunk:  Option<&dyn Fn(&str)>,
) -> anyhow::Result<ProviderResponse> {
    let reader = BufReader::new(body.as_reader());
    let mut content = String::new();
    let mut model   = req_model.to_string();
    let mut usage: Option<Usage> = None;

    for line in reader.lines() {
        let line = line.context("Stream read error")?;
        let line = line.trim();
        if verbose { eprintln!("< {line}"); }
        if line.is_empty() { continue; }
        let data = match line.strip_prefix(SSE_DATA_PREFIX) { Some(d) => d, None => continue };
        if data == SSE_DONE { break; }
        let chunk: OAIChunk = match serde_json::from_str(data) { Ok(c) => c, Err(_) => continue };
        if let Some(m) = &chunk.model { model = m.clone(); }
        if let Some(u) = chunk.usage {
            usage = Some(Usage {
                input_tokens:  u.prompt_tokens,
                output_tokens: u.completion_tokens,
            });
        }
        for choice in &chunk.choices {
            if let Some(text) = &choice.delta.content {
                if let Some(cb) = on_chunk { cb(text); }
                content.push_str(text);
            }
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

    fn make_provider(server: &MockServer) -> OpenAIProvider {
        OpenAIProvider::new(&server.base_url(), "test-key", "gpt-4.1-mini".into(), Duration::from_secs(5))
    }

    #[test]
    fn chat_non_streaming_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body(json!({
                "model": "gpt-4.1-mini",
                "choices": [{"message": {"role": "assistant", "content": "Hi"}}],
                "usage": {"prompt_tokens": 3, "completion_tokens": 1}
            }));
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("hello".into())], false, false, None).unwrap();
        assert_eq!(resp.content, "Hi");
        assert_eq!(resp.model, "gpt-4.1-mini");
        assert_eq!(resp.usage.as_ref().and_then(|u| u.input_tokens), Some(3));
        assert_eq!(resp.usage.as_ref().and_then(|u| u.output_tokens), Some(1));
    }

    #[test]
    fn chat_non_streaming_empty_choices_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body(json!({
                "model": "gpt-4.1-mini",
                "choices": [],
                "usage": null
            }));
        });
        let prov = make_provider(&server);
        let result = prov.chat(&[ChatMessage::user("hello".into())], false, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn chat_passes_all_messages_including_system() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .json_body_partial(r#"{"messages":[{"role":"system","content":"sys"},{"role":"user","content":"q"}]}"#);
            then.status(200).json_body(json!({
                "model": "gpt-4.1-mini",
                "choices": [{"message": {"role": "assistant", "content": "ok"}}],
                "usage": null
            }));
        });
        let prov = make_provider(&server);
        let messages = vec![
            ChatMessage::system("sys".into()),
            ChatMessage::user("q".into()),
        ];
        prov.chat(&messages, false, false, None).unwrap();
        mock.assert();
    }

    #[test]
    fn chat_streaming_includes_stream_options() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .json_body_partial(r#"{"stream":true,"stream_options":{"include_usage":true}}"#);
            then.status(200)
                .header("content-type", "text/event-stream")
                .body("data: [DONE]\n\n");
        });
        let prov = make_provider(&server);
        prov.chat(&[ChatMessage::user("hi".into())], true, false, None).unwrap();
        mock.assert();
    }

    #[test]
    fn chat_streaming_accumulates_content() {
        let server = MockServer::start();
        let sse_body = concat!(
            "data: {\"model\":\"gpt-4.1-mini\",\"choices\":[{\"delta\":{\"content\":\"Hel\"}}],\"usage\":null}\n\n",
            "data: {\"model\":\"gpt-4.1-mini\",\"choices\":[{\"delta\":{\"content\":\"lo\"}}],\"usage\":null}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n",
        );
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse_body);
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("hi".into())], true, false, None).unwrap();
        assert_eq!(resp.content, "Hello");
        assert_eq!(resp.usage.as_ref().and_then(|u| u.output_tokens), Some(2));
    }

    #[test]
    fn chat_streaming_calls_on_chunk_callback() {
        let server = MockServer::start();
        let sse_body = concat!(
            "data: {\"model\":\"gpt-4.1-mini\",\"choices\":[{\"delta\":{\"content\":\"Hel\"}}],\"usage\":null}\n\n",
            "data: {\"model\":\"gpt-4.1-mini\",\"choices\":[{\"delta\":{\"content\":\"lo\"}}],\"usage\":null}\n\n",
            "data: [DONE]\n\n",
        );
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse_body);
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
    fn chat_api_error_returns_err() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(429).body("Rate limited");
        });
        let prov = make_provider(&server);
        let result = prov.chat(&[ChatMessage::user("hi".into())], false, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn list_models_calls_api_and_sorts() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/models");
            then.status(200).json_body(json!({
                "data": [{"id": "gpt-4.1"}, {"id": "gpt-4o"}, {"id": "gpt-4.1-mini"}]
            }));
        });
        let prov = make_provider(&server);
        let models = prov.list_models().unwrap();
        assert_eq!(models, vec!["gpt-4.1", "gpt-4.1-mini", "gpt-4o"]);
    }

    #[test]
    fn endpoint_returns_chat_completions_url() {
        let server = MockServer::start();
        let prov = make_provider(&server);
        assert!(prov.endpoint().ends_with("/chat/completions"));
    }

    #[test]
    fn request_json_non_streaming_omits_stream_options() {
        let server = MockServer::start();
        let prov = make_provider(&server);
        let body = prov.request_json(&[ChatMessage::user("q".into())], false);
        assert!(body.get("stream_options").is_none());
        assert_eq!(body["stream"], json!(false));
    }
}
