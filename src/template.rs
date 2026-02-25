use std::collections::HashMap;

use anyhow::Context;
use serde_json::json;

use crate::memory::MemoryContent;

pub struct FileArg {
    pub path:    String,
    pub content: String,
}

pub struct TemplateContext<'a> {
    pub stdin:      Option<&'a str>,
    pub inputs:     &'a HashMap<String, Option<String>>,
    pub files:      &'a [FileArg],
    pub memory:     &'a MemoryContent,
    pub model:      &'a str,
    pub profile:    &'a str,
    pub session_id: &'a str,
    pub cwd:        &'a str,
    pub date:       &'a str,
}

pub fn render(template: &str, ctx: &TemplateContext<'_>) -> anyhow::Result<String> {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.add_template("cmd", template).context("Failed to parse command template")?;

    let files: Vec<_> = ctx.files.iter()
        .map(|f| json!({ "path": f.path, "content": f.content }))
        .collect();

    let inputs: serde_json::Map<String, serde_json::Value> = ctx.inputs.iter()
        .map(|(k, v)| (k.clone(), match v {
            Some(s) => serde_json::Value::String(s.clone()),
            None    => serde_json::Value::Null,
        }))
        .collect();

    let ctx_val = json!({
        "stdin":      ctx.stdin,
        "inputs":     inputs,
        "files":      files,
        "memory": {
            "global":        ctx.memory.global,
            "local":         ctx.memory.local,
            "local_private": ctx.memory.local_private,
        },
        "model":      ctx.model,
        "profile":    ctx.profile,
        "session_id": ctx.session_id,
        "cwd":        ctx.cwd,
        "date":       ctx.date,
    });

    env.get_template("cmd")?
        .render(ctx_val)
        .context("Failed to render command template")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_ctx<'a>(
        stdin:  Option<&'a str>,
        inputs: &'a HashMap<String, Option<String>>,
        memory: &'a MemoryContent,
    ) -> TemplateContext<'a> {
        TemplateContext {
            stdin,
            inputs,
            files: &[],
            memory,
            model: "gpt-4.1-mini",
            profile: "openai",
            session_id: "abc123",
            cwd: "/tmp",
            date: "2026-04-26",
        }
    }

    #[test]
    fn render_plain_string() {
        let mem    = MemoryContent::default();
        let inputs = HashMap::new();
        let ctx    = empty_ctx(None, &inputs, &mem);
        assert_eq!(render("Hello world", &ctx).unwrap(), "Hello world");
    }

    #[test]
    fn render_stdin_variable() {
        let mem    = MemoryContent::default();
        let inputs = HashMap::new();
        let ctx    = empty_ctx(Some("piped content"), &inputs, &mem);
        assert_eq!(render("{{ stdin }}", &ctx).unwrap(), "piped content");
    }

    #[test]
    fn render_stdin_conditional_true() {
        let mem    = MemoryContent::default();
        let inputs = HashMap::new();
        let ctx    = empty_ctx(Some("x"), &inputs, &mem);
        assert_eq!(render("{% if stdin %}yes{% endif %}", &ctx).unwrap(), "yes");
    }

    #[test]
    fn render_stdin_conditional_false() {
        let mem    = MemoryContent::default();
        let inputs = HashMap::new();
        let ctx    = empty_ctx(None, &inputs, &mem);
        assert_eq!(render("{% if stdin %}yes{% endif %}", &ctx).unwrap(), "");
    }

    #[test]
    fn render_named_input() {
        let mem    = MemoryContent::default();
        let mut inputs = HashMap::new();
        inputs.insert("topic".to_string(), Some("security".to_string()));
        let ctx = empty_ctx(None, &inputs, &mem);
        assert_eq!(render("Focus: {{ inputs.topic }}", &ctx).unwrap(), "Focus: security");
    }

    #[test]
    fn render_named_input_conditional_true() {
        let mem    = MemoryContent::default();
        let mut inputs = HashMap::new();
        inputs.insert("focus".to_string(), Some("bugs".to_string()));
        let ctx = empty_ctx(None, &inputs, &mem);
        assert_eq!(render("{% if inputs.focus %}yes{% endif %}", &ctx).unwrap(), "yes");
    }

    #[test]
    fn render_named_input_conditional_false_when_none() {
        let mem    = MemoryContent::default();
        let mut inputs = HashMap::new();
        inputs.insert("focus".to_string(), None);
        let ctx = empty_ctx(None, &inputs, &mem);
        assert_eq!(render("{% if inputs.focus %}yes{% endif %}", &ctx).unwrap(), "");
    }

    #[test]
    fn render_named_input_conditional_false_when_missing() {
        let mem    = MemoryContent::default();
        let inputs = HashMap::new();
        let ctx    = empty_ctx(None, &inputs, &mem);
        assert_eq!(render("{% if inputs.focus %}yes{% endif %}", &ctx).unwrap(), "");
    }

    #[test]
    fn render_date_and_model() {
        let mem    = MemoryContent::default();
        let inputs = HashMap::new();
        let ctx    = empty_ctx(None, &inputs, &mem);
        assert_eq!(render("{{ date }} {{ model }}", &ctx).unwrap(), "2026-04-26 gpt-4.1-mini");
    }

    #[test]
    fn render_files_list() {
        let mem    = MemoryContent::default();
        let inputs = HashMap::new();
        let files  = vec![
            FileArg { path: "a.rs".into(), content: "fn main() {}".into() },
            FileArg { path: "b.rs".into(), content: "fn foo() {}".into() },
        ];
        let ctx = TemplateContext {
            stdin: None, inputs: &inputs, files: &files,
            memory: &mem, model: "gpt-4.1-mini", profile: "openai",
            session_id: "abc", cwd: "/tmp", date: "2026-04-26",
        };
        let result = render("{% for f in files %}{{ f.path }} {% endfor %}", &ctx).unwrap();
        assert_eq!(result.trim(), "a.rs b.rs");
    }

    #[test]
    fn render_memory_global() {
        let mem    = MemoryContent { global: Some("my notes".into()), local: None, local_private: None };
        let inputs = HashMap::new();
        let ctx    = empty_ctx(None, &inputs, &mem);
        assert_eq!(render("{{ memory.global }}", &ctx).unwrap(), "my notes");
    }

    #[test]
    fn render_invalid_template_errors() {
        let mem    = MemoryContent::default();
        let inputs = HashMap::new();
        let ctx    = empty_ctx(None, &inputs, &mem);
        assert!(render("{{ unclosed", &ctx).is_err());
    }
}
