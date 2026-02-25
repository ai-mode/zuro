use super::CommandFrontmatter;

pub fn parse_filename(stem: &str) -> String {
    stem.to_string()
}

pub fn parse_frontmatter(src: &str) -> anyhow::Result<(CommandFrontmatter, String)> {
    if !src.starts_with("---\n") {
        return Ok((CommandFrontmatter::default(), src.to_string()));
    }
    let Some(end) = src[4..].find("\n---\n") else {
        return Ok((CommandFrontmatter::default(), src.to_string()));
    };
    let fm_str = &src[4..4 + end];
    let body   = src[4 + end + 5..].to_string();
    let fm: CommandFrontmatter = serde_yaml::from_str(fm_str)
        .map_err(|e| anyhow::anyhow!("Invalid frontmatter: {e}"))?;
    Ok((fm, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{HistoryMode, InputDef};

    #[test]
    fn parse_filename_returns_stem_as_is() {
        assert_eq!(parse_filename("fix"), "fix");
        assert_eq!(parse_filename("user__fix"), "user__fix");
        assert_eq!(parse_filename(""), "");
    }

    #[test]
    fn no_frontmatter_returns_defaults() {
        let (fm, body) = parse_frontmatter("Hello world").unwrap();
        assert_eq!(fm.description, None);
        assert!(fm.inputs.is_empty());
        assert_eq!(fm.history, HistoryMode::Full);
        assert_eq!(body, "Hello world");
    }

    #[test]
    fn description_only() {
        let src = "---\ndescription: Fix bugs\n---\nbody";
        let (fm, body) = parse_frontmatter(src).unwrap();
        assert_eq!(fm.description, Some("Fix bugs".to_string()));
        assert_eq!(body, "body");
    }

    #[test]
    fn history_modes() {
        for (mode, expected) in [
            ("small", HistoryMode::Small),
            ("large", HistoryMode::Large),
            ("none",  HistoryMode::None),
            ("full",  HistoryMode::Full),
        ] {
            let src = format!("---\nhistory: {mode}\n---\nbody");
            let (fm, _) = parse_frontmatter(&src).unwrap();
            assert_eq!(fm.history, expected);
        }
    }

    #[test]
    fn invalid_history_is_error() {
        let src = "---\nhistory: mega\n---\nbody";
        assert!(parse_frontmatter(src).is_err());
    }

    #[test]
    fn single_input_with_prompt() {
        let src = "---\ninputs:\n  - name: focus\n    prompt: \"Focus on:\"\n    required: true\n---\nbody";
        let (fm, _) = parse_frontmatter(src).unwrap();
        assert_eq!(fm.inputs.len(), 1);
        assert_eq!(fm.inputs[0].name, "focus");
        assert_eq!(fm.inputs[0].prompt, "Focus on:");
        assert!(fm.inputs[0].required);
    }

    #[test]
    fn multiple_inputs() {
        let src = "---\ninputs:\n  - name: a\n  - name: b\n---\nbody";
        let (fm, _) = parse_frontmatter(src).unwrap();
        assert_eq!(fm.inputs.len(), 2);
        assert_eq!(fm.inputs[0].name, "a");
        assert_eq!(fm.inputs[1].name, "b");
    }

    #[test]
    fn input_default_prompt_is_empty() {
        let src = "---\ninputs:\n  - name: topic\n---\nbody";
        let (fm, _) = parse_frontmatter(src).unwrap();
        assert_eq!(fm.inputs[0].prompt, "");
        assert_eq!(fm.inputs[0].display_prompt(), "topic: ");
    }

    #[test]
    fn unclosed_frontmatter_treated_as_no_frontmatter() {
        let src = "---\ndescription = \"no close\"\nbody";
        let (fm, body) = parse_frontmatter(src).unwrap();
        assert_eq!(fm.description, None);
        assert_eq!(body, src);
    }

    #[test]
    fn empty_body_after_close() {
        let src = "---\ndescription = \"x\"\n---\n";
        let (fm, body) = parse_frontmatter(src).unwrap();
        assert_eq!(fm.description, Some("x".to_string()));
        assert_eq!(body, "");
    }
}
