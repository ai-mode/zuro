use std::fs;
use std::path::{Path, PathBuf};

use crate::constants::{ZURO_DIR, MEMORY_FILE, MEMORY_LOCAL_FILE};
use super::{MemoryContent, MemoryLocation};

fn global_memory_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(ZURO_DIR).join(MEMORY_FILE)
}

fn local_memory_path(root: &Path) -> PathBuf {
    root.join(ZURO_DIR).join(MEMORY_FILE)
}

fn local_private_memory_path(root: &Path) -> PathBuf {
    root.join(ZURO_DIR).join(MEMORY_LOCAL_FILE)
}

fn resolve_path(loc: &MemoryLocation, project_root: Option<&Path>) -> anyhow::Result<PathBuf> {
    match loc {
        MemoryLocation::Global => Ok(global_memory_path()),
        MemoryLocation::Local => {
            let root = project_root.ok_or_else(|| anyhow::anyhow!("No project root found for local memory"))?;
            Ok(local_memory_path(root))
        }
        MemoryLocation::LocalPrivate => {
            let root = project_root.ok_or_else(|| anyhow::anyhow!("No project root found for local private memory"))?;
            Ok(local_private_memory_path(root))
        }
    }
}

pub fn load_memory(project_root: Option<&Path>) -> MemoryContent {
    let global = fs::read_to_string(global_memory_path()).ok().filter(|s| !s.is_empty());
    let (local, local_private) = project_root
        .map(|root| {
            let local   = fs::read_to_string(local_memory_path(root)).ok().filter(|s| !s.is_empty());
            let private = fs::read_to_string(local_private_memory_path(root)).ok().filter(|s| !s.is_empty());
            (local, private)
        })
        .unwrap_or((None, None));
    MemoryContent { global, local, local_private }
}

pub fn append_to_memory(loc: MemoryLocation, text: &str, project_root: Option<&Path>) -> anyhow::Result<()> {
    let path = resolve_path(&loc, project_root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let content = if existing.trim().is_empty() {
        text.to_string()
    } else {
        format!("{}\n---\n{}", existing.trim_end(), text)
    };
    fs::write(&path, content)?;
    Ok(())
}

pub fn show_memory(loc: Option<MemoryLocation>, project_root: Option<&Path>) -> anyhow::Result<String> {
    match loc {
        None => {
            let mut parts = Vec::new();
            if let Ok(s) = fs::read_to_string(global_memory_path()) {
                if !s.trim().is_empty() {
                    parts.push(format!("[global: ~/.zuro/memory.md]\n{s}"));
                }
            }
            if let Some(root) = project_root {
                if let Ok(s) = fs::read_to_string(local_memory_path(root)) {
                    if !s.trim().is_empty() {
                        parts.push(format!("[local: .zuro/memory.md]\n{s}"));
                    }
                }
                if let Ok(s) = fs::read_to_string(local_private_memory_path(root)) {
                    if !s.trim().is_empty() {
                        parts.push(format!("[private: .zuro/memory.local.md]\n{s}"));
                    }
                }
            }
            Ok(parts.join("\n\n"))
        }
        Some(loc) => {
            let path = resolve_path(&loc, project_root)?;
            Ok(fs::read_to_string(&path).unwrap_or_default())
        }
    }
}

pub fn clear_memory(loc: MemoryLocation, project_root: Option<&Path>) -> anyhow::Result<()> {
    let path = resolve_path(&loc, project_root)?;
    if path.exists() {
        fs::write(&path, "")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_root() -> TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn load_memory_all_none_when_no_files() {
        let tmp = make_root();
        let mem = load_memory(Some(tmp.path()));
        assert!(mem.local.is_none());
        assert!(mem.local_private.is_none());
    }

    #[test]
    fn load_memory_reads_local() {
        let tmp = make_root();
        let path = local_memory_path(tmp.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "local notes").unwrap();
        let mem = load_memory(Some(tmp.path()));
        assert_eq!(mem.local, Some("local notes".to_string()));
    }

    #[test]
    fn load_memory_reads_local_private() {
        let tmp = make_root();
        let path = local_private_memory_path(tmp.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "private notes").unwrap();
        let mem = load_memory(Some(tmp.path()));
        assert_eq!(mem.local_private, Some("private notes".to_string()));
    }

    #[test]
    fn append_creates_file_when_absent() {
        let tmp = make_root();
        append_to_memory(MemoryLocation::Local, "first entry", Some(tmp.path())).unwrap();
        let path = local_memory_path(tmp.path());
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "first entry");
    }

    #[test]
    fn append_adds_separator() {
        let tmp = make_root();
        append_to_memory(MemoryLocation::Local, "first", Some(tmp.path())).unwrap();
        append_to_memory(MemoryLocation::Local, "second", Some(tmp.path())).unwrap();
        let path = local_memory_path(tmp.path());
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "first\n---\nsecond");
    }

    #[test]
    fn clear_memory_empties_file() {
        let tmp = make_root();
        append_to_memory(MemoryLocation::Local, "something", Some(tmp.path())).unwrap();
        clear_memory(MemoryLocation::Local, Some(tmp.path())).unwrap();
        let path = local_memory_path(tmp.path());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn clear_memory_ok_when_no_file() {
        let tmp = make_root();
        let result = clear_memory(MemoryLocation::Local, Some(tmp.path()));
        assert!(result.is_ok());
    }

    #[test]
    fn show_memory_with_location_returns_specific_content() {
        let tmp = make_root();
        append_to_memory(MemoryLocation::Local, "local content", Some(tmp.path())).unwrap();
        let s = show_memory(Some(MemoryLocation::Local), Some(tmp.path())).unwrap();
        assert_eq!(s, "local content");
    }

    #[test]
    fn show_memory_no_location_combines_all() {
        let tmp = make_root();
        append_to_memory(MemoryLocation::Local, "local", Some(tmp.path())).unwrap();
        append_to_memory(MemoryLocation::LocalPrivate, "private", Some(tmp.path())).unwrap();
        let s = show_memory(None, Some(tmp.path())).unwrap();
        assert!(s.contains("local"));
        assert!(s.contains("private"));
    }
}
