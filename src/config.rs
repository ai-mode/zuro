use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::constants::{ZURO_DIR, PROJECT_CONFIG_FILE};
use crate::defaults;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub default:  DefaultConfig,
    #[serde(default, alias = "providers")]
    pub profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultConfig {
    #[serde(alias = "provider")]
    pub profile:    String,
    #[serde(default)]
    pub show_stats: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repl_submit_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repl_history_limit: Option<usize>,
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            profile:            default_type(),
            show_stats:         false,
            editor:             None,
            shell:              None,
            repl_submit_key:    None,
            repl_history_limit: None,
        }
    }
}

#[derive(Copy, Clone)]
pub enum SubmitKey { CtrlEnter, Enter }

pub fn resolve_submit_key(cfg: &DefaultConfig) -> SubmitKey {
    match cfg.repl_submit_key.as_deref().unwrap_or("ctrl+enter") {
        "enter" => SubmitKey::Enter,
        _       => SubmitKey::CtrlEnter,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    #[serde(default)]
    pub pool: PoolConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    #[serde(default)]
    pub use_global:  bool,
    #[serde(default)]
    pub local_merge: LocalMerge,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self { use_global: false, local_merge: LocalMerge::Append }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LocalMerge {
    #[default]
    Append,
    Replace,
}

pub fn load_project_config(project_root: Option<&Path>) -> ProjectConfig {
    let Some(root) = project_root else { return ProjectConfig::default(); };
    let path = root.join(ZURO_DIR).join(PROJECT_CONFIG_FILE);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn resolve_editor(cfg: &DefaultConfig) -> String {
    cfg.editor.clone()
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "vi".to_string())
}

pub fn resolve_shell(cfg: &DefaultConfig) -> String {
    cfg.shell.clone()
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "sh".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(rename = "type", default = "default_type")]
    pub provider_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key:       Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url:      Option<String>,
    pub model:         String,
}

fn default_type() -> String {
    defaults::all().first().map(|d| d.name.clone()).unwrap_or_else(|| "openai".into())
}

fn home() -> PathBuf {
    dirs::home_dir()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn config_path() -> PathBuf {
    home().join(".config").join("zuro").join("config.toml")
}

pub fn data_dir() -> PathBuf {
    home().join(".local").join("share").join("zuro")
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = config_path();
        if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("Cannot read {}", path.display()))?;
            return toml::from_str(&raw)
                .with_context(|| format!("Invalid TOML in {}", path.display()));
        }

        if io::stdin().is_terminal() {
            eprintln!("No config found at {}. Running setup wizard.", path.display());
            let cfg = run_wizard()?;
            cfg.save()?;
            eprintln!("\nConfig saved to {}", path.display());
            return Ok(cfg);
        }

        anyhow::bail!(
            "Config file not found: {}\n\nCreate it:\n\
             [default]\n\
             profile = \"openai\"\n\n\
             [profiles.openai]\n\
             type     = \"openai\"\n\
             api_key  = \"sk-...\"\n\
             base_url = \"https://api.openai.com/v1\"\n\
             model    = \"gpt-4o-mini\"",
            path.display()
        )
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        let s   = toml::to_string_pretty(self).context("Failed to serialize config")?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, s)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn active_profile(
        &self,
        name_override: Option<&str>,
    ) -> anyhow::Result<(String, &ProfileConfig)> {
        let name = name_override.unwrap_or(&self.default.profile);
        let cfg  = self.profiles.get(name)
            .with_context(|| format!("Profile '{}' not in config", name))?;
        Ok((name.to_string(), cfg))
    }
}

pub fn ask(label: &str, default: Option<&str>) -> anyhow::Result<String> {
    let mut stdout = io::stdout();
    match default {
        Some(d) => print!("{label} [{d}]: "),
        None    => print!("{label}: "),
    }
    stdout.flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let v = line.trim().to_string();
    if v.is_empty() {
        return default
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("{label} is required"));
    }
    Ok(v)
}

pub fn ask_opt(label: &str) -> anyhow::Result<Option<String>> {
    let mut stdout = io::stdout();
    print!("{label} (leave blank to skip): ");
    stdout.flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let v = line.trim().to_string();
    Ok(if v.is_empty() { None } else { Some(v) })
}

fn run_wizard() -> anyhow::Result<Config> {
    println!("\n=== zuro CLI Setup ===\n");

    let ptype = ask("Profile type (openai/anthropic)", Some(&default_type()))?;
    let d = defaults::find(&ptype)
        .or_else(|| defaults::all().first())
        .expect("at least one provider in providers.toml");

    let base_url = ask("API base URL", Some(&d.base_url))?;
    let api_key  = ask("API key", None)?;
    let model    = ask("Default model", Some(&d.default_model))?;

    let mut profiles = HashMap::new();
    profiles.insert(
        "default".to_string(),
        ProfileConfig {
            provider_type: ptype,
            api_key:       Some(api_key),
            base_url:      Some(base_url),
            model,
        },
    );

    Ok(Config {
        default: DefaultConfig {
            profile:            "default".into(),
            show_stats:         false,
            editor:             None,
            shell:              None,
            repl_submit_key:    None,
            repl_history_limit: None,
        },
        profiles,
    })
}
