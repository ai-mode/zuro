mod parse;
mod resolve;

pub use parse::parse_filename;
pub use resolve::{find_project_root, list_commands, resolve_command};

use serde::Deserialize;

pub struct CommandDef {
    pub name:        String,
    pub frontmatter: CommandFrontmatter,
    pub template:    String,
    pub location:    CommandLocation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandLocation {
    BuiltIn,
    Global,
    Local,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputDef {
    pub name:     String,
    /// Prompt shown to the user. Defaults to "name: " if empty.
    #[serde(default)]
    pub prompt:   String,
    #[serde(default)]
    pub required: bool,
}

impl InputDef {
    pub fn display_prompt(&self) -> String {
        if self.prompt.is_empty() {
            format!("{}: ", self.name)
        } else {
            self.prompt.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryMode {
    #[default]
    Full,
    Small,
    Large,
    None,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommandFrontmatter {
    pub description: Option<String>,
    #[serde(default)]
    pub inputs:      Vec<InputDef>,
    #[serde(default)]
    pub history:     HistoryMode,
}
