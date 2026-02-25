use std::io::{BufRead, BufReader};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::{mask_key, ChatMessage, Provider, ProviderResponse, Usage};
use crate::constants::SSE_DATA_PREFIX;

pub struct OpenAIResponsesProvider {
    base_url: String,
    api_key:  String,
    model:    String,
    agent:    ureq::Agent,
}

impl OpenAIResponsesProvider {
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
struct ResponsesRequest<'a> {
    model: &'a str,
    input: Vec<ResponsesMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct ResponsesMessage {
    role:    String,
    content: String,
}

#[derive(Deserialize)]
struct ResponsesResponse {
    model:  Option<String>,
    output: Vec<OutputItem>,
    usage:  Option<ResponsesUsage>,
}

#[derive(Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    kind:    String,
    #[serde(default)]
    content: Vec<OutputContent>,
}

#[derive(Deserialize)]
struct OutputContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ResponsesUsage {
    input_tokens:  Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct DeltaEvent {
    delta: String,
}

#[derive(Deserialize)]
struct CompletedEvent {
    response: CompletedResponse,
}

#[derive(Deserialize)]
struct CompletedResponse {
    model: Option<String>,
    usage: Option<ResponsesUsage>,
}

#[derive(Deserialize)]
struct ModelsResponse { data: Vec<ModelEntry> }
#[derive(Deserialize)]
struct ModelEntry { id: String }

fn build_request<'a>(model: &'a str, messages: &[ChatMessage], stream: bool) -> ResponsesRequest<'a> {
    let instructions = messages.iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.clone())
        .collect::<Vec<_>>()
        .join("\n\n");
    ResponsesRequest {
        model,
        input: messages.iter()
            .filter(|m| m.role != "system")
            .map(|m| ResponsesMessage { role: m.role.clone(), content: m.content.clone() })
            .collect(),
        instructions: if instructions.is_empty() { None } else { Some(instructions) },
        stream: if stream { Some(true) } else { None },
    }
}

fn log_verbose_request(url: &str, api_key: &str, body: &ResponsesRequest<'_>) {
    let s = serde_json::to_string_pretty(body).unwrap_or_default();
    eprintln!("> POST {url}");
    eprintln!("> Authorization: Bearer {}", mask_key(api_key));
    for line in s.lines() { eprintln!("> {line}"); }
    eprintln!();
}

fn parse_response(resp: ResponsesResponse, fallback_model: &str) -> ProviderResponse {
    let content = resp.output.into_iter()
        .filter(|item| item.kind == "message")
        .flat_map(|item| item.content.into_iter())
        .filter(|c| c.kind == "output_text")
        .filter_map(|c| c.text)
        .collect::<Vec<_>>()
        .join("");
    let model = resp.model.unwrap_or_else(|| fallback_model.to_string());
    let usage = resp.usage.map(|u| Usage {
        input_tokens:  u.input_tokens,
        output_tokens: u.output_tokens,
    });
    ProviderResponse { content, model, usage }
}

fn parse_stream(
    body:      &mut ureq::Body,
    req_model: &str,
    verbose:   bool,
    on_chunk:  Option<&dyn Fn(&str)>,
) -> anyhow::Result<ProviderResponse> {
    let reader = BufReader::new(body.as_reader());
    let mut content    = String::new();
    let mut model      = req_model.to_string();
    let mut usage      = None;
    let mut event_type = String::new();

    for line in reader.lines() {
        let line = line.context("Stream read error")?;
        let line = line.trim();
        if verbose { eprintln!("< {line}"); }
        if line.is_empty() { continue; }

        if let Some(ev) = line.strip_prefix("event: ") {
            event_type = ev.to_string();
            continue;
        }

        let data = match line.strip_prefix(SSE_DATA_PREFIX) { Some(d) => d, None => continue };

        match event_type.as_str() {
            "response.output_text.delta" => {
                let ev: DeltaEvent = match serde_json::from_str(data) { Ok(e) => e, Err(_) => continue };
                if let Some(cb) = on_chunk { cb(&ev.delta); }
                content.push_str(&ev.delta);
            }
            "response.completed" => {
                let ev: CompletedEvent = match serde_json::from_str(data) { Ok(e) => e, Err(_) => break };
                if let Some(m) = ev.response.model { model = m; }
                if let Some(u) = ev.response.usage {
                    usage = Some(Usage {
                        input_tokens:  u.input_tokens,
                        output_tokens: u.output_tokens,
                    });
                }
                break;
            }
            _ => {}
        }
    }
    Ok(ProviderResponse { content, model, usage })
}

impl Provider for OpenAIResponsesProvider {
    fn chat(
        &self,
        messages: &[ChatMessage],
        stream:   bool,
        verbose:  bool,
        on_chunk: Option<&dyn Fn(&str)>,
    ) -> anyhow::Result<ProviderResponse> {
        let url  = format!("{}/responses", self.base_url);
        let body = build_request(&self.model, messages, stream);

        if verbose { log_verbose_request(&url, &self.api_key, &body); }

        let mut resp = self.agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("Responses API request failed: {e}"))?;

        if verbose { eprintln!("< {}", resp.status()); eprintln!(); }

        if stream {
            parse_stream(resp.body_mut(), &self.model, verbose, on_chunk)
        } else {
            let r: ResponsesResponse = resp.body_mut().read_json()
                .map_err(|e| anyhow::anyhow!("Failed to parse Responses API response: {e}"))?;
            Ok(parse_response(r, &self.model))
        }
    }

    fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        let r: ModelsResponse = self.agent
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
        format!("{}/responses", self.base_url)
    }

    fn request_json(&self, messages: &[ChatMessage], stream: bool) -> serde_json::Value {
        serde_json::to_value(build_request(&self.model, messages, stream)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;
    use std::time::Duration;

    fn make_provider(server: &MockServer) -> OpenAIResponsesProvider {
        OpenAIResponsesProvider::new(&server.base_url(), "test-key", "o3".into(), Duration::from_secs(5))
    }

    #[test]
    fn chat_non_streaming_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/responses");
            then.status(200).json_body(json!({
                "model": "o3",
                "output": [{"type": "message", "content": [{"type": "output_text", "text": "Done"}]}],
                "usage": {"input_tokens": 4, "output_tokens": 1}
            }));
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("go".into())], false, false, None).unwrap();
        assert_eq!(resp.content, "Done");
        assert_eq!(resp.model, "o3");
        assert_eq!(resp.usage.as_ref().and_then(|u| u.input_tokens), Some(4));
    }

    #[test]
    fn chat_non_streaming_skips_non_message_output() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/responses");
            then.status(200).json_body(json!({
                "model": "o3",
                "output": [
                    {"type": "reasoning", "content": []},
                    {"type": "message", "content": [{"type": "output_text", "text": "ok"}]}
                ],
                "usage": null
            }));
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("go".into())], false, false, None).unwrap();
        assert_eq!(resp.content, "ok");
    }

    #[test]
    fn chat_non_streaming_skips_non_output_text_content() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/responses");
            then.status(200).json_body(json!({
                "model": "o3",
                "output": [{"type": "message", "content": [
                    {"type": "refusal", "text": "no"},
                    {"type": "output_text", "text": "yes"}
                ]}],
                "usage": null
            }));
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("go".into())], false, false, None).unwrap();
        assert_eq!(resp.content, "yes");
    }

    #[test]
    fn chat_extracts_system_into_instructions() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/responses")
                .json_body_partial(r#"{"instructions":"Be concise"}"#);
            then.status(200).json_body(json!({
                "model": "o3",
                "output": [{"type": "message", "content": [{"type": "output_text", "text": "ok"}]}],
                "usage": null
            }));
        });
        let prov = make_provider(&server);
        let messages = vec![
            ChatMessage::system("Be concise".into()),
            ChatMessage::user("hi".into()),
        ];
        prov.chat(&messages, false, false, None).unwrap();
        mock.assert();
    }

    #[test]
    fn chat_no_system_omits_instructions_field() {
        let server = MockServer::start();
        // If the provider sends an "instructions" field, this mock matches first and returns 500.
        server.mock(|when, then| {
            when.method(POST)
                .path("/responses")
                .body_contains("\"instructions\"");
            then.status(500).body("unexpected instructions field");
        });
        server.mock(|when, then| {
            when.method(POST).path("/responses");
            then.status(200).json_body(json!({
                "model": "o3",
                "output": [{"type": "message", "content": [{"type": "output_text", "text": "ok"}]}],
                "usage": null
            }));
        });
        let prov = make_provider(&server);
        prov.chat(&[ChatMessage::user("hi".into())], false, false, None).unwrap();
    }

    #[test]
    fn chat_streaming_accumulates_delta() {
        let server = MockServer::start();
        let sse_body = concat!(
            "event: response.output_text.delta\n",
            "data: {\"delta\":\"Hel\"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"delta\":\"lo\"}\n\n",
            "event: response.completed\n",
            "data: {\"response\":{\"model\":\"o3\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2}}}\n\n",
        );
        server.mock(|when, then| {
            when.method(POST).path("/responses");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse_body);
        });
        let prov = make_provider(&server);
        let resp = prov.chat(&[ChatMessage::user("go".into())], true, false, None).unwrap();
        assert_eq!(resp.content, "Hello");
        assert_eq!(resp.model, "o3");
        assert_eq!(resp.usage.as_ref().and_then(|u| u.output_tokens), Some(2));
    }

    #[test]
    fn chat_streaming_calls_on_chunk_callback() {
        let server = MockServer::start();
        let sse_body = concat!(
            "event: response.output_text.delta\n",
            "data: {\"delta\":\"Hel\"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"delta\":\"lo\"}\n\n",
            "event: response.completed\n",
            "data: {\"response\":{\"model\":\"o3\",\"usage\":null}}\n\n",
        );
        server.mock(|when, then| {
            when.method(POST).path("/responses");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse_body);
        });
        let prov = make_provider(&server);
        let chunks = std::sync::Mutex::new(Vec::<String>::new());
        prov.chat(&[ChatMessage::user("go".into())], true, false, Some(&|chunk| {
            chunks.lock().unwrap().push(chunk.to_string());
        })).unwrap();
        let chunks = chunks.into_inner().unwrap();
        assert_eq!(chunks, vec!["Hel", "lo"]);
    }

    #[test]
    fn chat_api_error_returns_err() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/responses");
            then.status(500).body("Internal Server Error");
        });
        let prov = make_provider(&server);
        let result = prov.chat(&[ChatMessage::user("go".into())], false, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn list_models_calls_api_and_sorts() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/models");
            then.status(200).json_body(json!({
                "data": [{"id": "o4-mini"}, {"id": "o3"}]
            }));
        });
        let prov = make_provider(&server);
        let models = prov.list_models().unwrap();
        assert_eq!(models, vec!["o3", "o4-mini"]);
    }

    #[test]
    fn endpoint_returns_responses_url() {
        let server = MockServer::start();
        let prov = make_provider(&server);
        assert!(prov.endpoint().ends_with("/responses"));
    }
}
