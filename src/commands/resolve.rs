use std::collections::HashSet;
use std::path::{Path, PathBuf};

use include_dir::{include_dir, Dir};

use crate::constants::{ZURO_DIR, COMMANDS_SUBDIR};
use super::{CommandDef, CommandLocation};
use super::parse::{parse_filename, parse_frontmatter};

static BUILTIN_COMMANDS: Dir = include_dir!("$CARGO_MANIFEST_DIR/.zuro/commands");

pub fn find_project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut dir = cwd.as_path();
    loop {
        if dir.join(ZURO_DIR).is_dir() || dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

pub fn resolve_command(name: &str, project_root: Option<&Path>) -> anyhow::Result<CommandDef> {
    if let Some(root) = project_root {
        let dir = root.join(ZURO_DIR).join(COMMANDS_SUBDIR);
        if let Some(cmd) = dir_command(name, &dir, CommandLocation::Local)? {
            return Ok(cmd);
        }
    }

    if let Some(global_dir) = global_commands_dir() {
        if let Some(cmd) = dir_command(name, &global_dir, CommandLocation::Global)? {
            return Ok(cmd);
        }
    }

    if let Some(cmd) = builtin_command(name)? {
        return Ok(cmd);
    }

    anyhow::bail!("Command '{name}' not found. Run 'zuro commands list' to see available commands.")
}

pub fn list_commands(project_root: Option<&Path>) -> Vec<CommandDef> {
    let builtins = all_builtin_commands();
    let globals  = global_commands_dir()
        .map(|d| all_dir_commands(&d, CommandLocation::Global))
        .unwrap_or_default();
    let locals = project_root
        .map(|r| all_dir_commands(&r.join(ZURO_DIR).join(COMMANDS_SUBDIR), CommandLocation::Local))
        .unwrap_or_default();

    let local_names:  HashSet<String> = locals.iter().map(|c| c.name.clone()).collect();
    let global_names: HashSet<String> = globals.iter().map(|c| c.name.clone()).collect();

    let mut result = Vec::new();
    result.extend(locals);
    for cmd in globals {
        if !local_names.contains(&cmd.name) { result.push(cmd); }
    }
    for cmd in builtins {
        if !local_names.contains(&cmd.name) && !global_names.contains(&cmd.name) {
            result.push(cmd);
        }
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

fn global_commands_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(ZURO_DIR).join(COMMANDS_SUBDIR))
}

fn dir_command(name: &str, dir: &Path, location: CommandLocation) -> anyhow::Result<Option<CommandDef>> {
    if !dir.is_dir() { return Ok(None); }
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(None); };

    let mut found: Option<CommandDef> = None;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let Some(ext) = path.extension() else { continue };
        if ext.to_str() != Some("md") { continue }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        if parse_filename(stem) != name { continue }

        if found.is_some() {
            anyhow::bail!("Duplicate command '{name}' in {:?}", dir);
        }
        let src = std::fs::read_to_string(&path)?;
        let (frontmatter, template) = parse_frontmatter(&src)?;
        found = Some(CommandDef { name: name.to_string(), frontmatter, template, location });
    }

    Ok(found)
}

fn all_dir_commands(dir: &Path, location: CommandLocation) -> Vec<CommandDef> {
    if !dir.is_dir() { return vec![]; }
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![]; };

    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension()?.to_str() != Some("md") { return None; }
            let stem = path.file_stem()?.to_str()?;
            let name = parse_filename(stem);
            if name.is_empty() { return None; }
            let src = std::fs::read_to_string(&path).ok()?;
            let (frontmatter, template) = parse_frontmatter(&src).ok()?;
            Some(CommandDef { name, frontmatter, template, location })
        })
        .collect()
}

fn builtin_command(name: &str) -> anyhow::Result<Option<CommandDef>> {
    let mut found: Option<CommandDef> = None;
    for file in BUILTIN_COMMANDS.files() {
        let Some(ext) = file.path().extension() else { continue };
        if ext.to_str() != Some("md") { continue }
        let Some(stem) = file.path().file_stem().and_then(|s| s.to_str()) else { continue };
        if parse_filename(stem) != name { continue }

        if found.is_some() {
            anyhow::bail!("Duplicate built-in command '{name}'");
        }
        let src = file.contents_utf8()
            .ok_or_else(|| anyhow::anyhow!("Built-in command '{name}' is not valid UTF-8"))?;
        let (frontmatter, template) = parse_frontmatter(src)?;
        found = Some(CommandDef { name: name.to_string(), frontmatter, template, location: CommandLocation::BuiltIn });
    }
    Ok(found)
}

fn all_builtin_commands() -> Vec<CommandDef> {
    BUILTIN_COMMANDS.files()
        .filter_map(|f| {
            if f.path().extension()?.to_str() != Some("md") { return None; }
            let stem = f.path().file_stem()?.to_str()?;
            let name = parse_filename(stem);
            if name.is_empty() { return None; }
            let src = f.contents_utf8()?;
            let (frontmatter, template) = parse_frontmatter(src).ok()?;
            Some(CommandDef { name, frontmatter, template, location: CommandLocation::BuiltIn })
        })
        .collect()
}
