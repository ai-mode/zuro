use std::fs;
use std::path::Path;

use crate::constants::{ZURO_DIR, SYSTEM_FILE};
use crate::memory::MemoryContent;
use crate::pool::ResolvedItem;
use crate::template::FileArg;

pub fn assemble_system_message(project_root: Option<&Path>, verbose: bool) -> String {
    let mut parts = Vec::new();

    let global_path = dirs::home_dir().map(|h| h.join(ZURO_DIR).join(SYSTEM_FILE));
    if let Some(content) = global_path.and_then(|p| fs::read_to_string(p).ok()).filter(|s| !s.trim().is_empty()) {
        if verbose { eprintln!("[system] global_system: ~/.zuro/system.md"); }
        parts.push(content);
    }

    if let Some(root) = project_root {
        if let Ok(content) = fs::read_to_string(root.join(ZURO_DIR).join(SYSTEM_FILE)) {
            if !content.trim().is_empty() {
                if verbose { eprintln!("[system] local_system: .zuro/system.md"); }
                parts.push(content);
            }
        }
    }

    parts.join("\n\n")
}

fn format_context_block(label: &str, content: &str) -> String {
    format!("[{label}]\n{content}")
}

pub fn assemble_user_prefix(
    memory:         &MemoryContent,
    resolved_items: &[ResolvedItem],
    files:          &[FileArg],
    stdin:          Option<&str>,
    verbose:        bool,
) -> String {
    let mut parts = Vec::new();

    if let Some(content) = &memory.global {
        if verbose { eprintln!("[provider] global_memory: ~/.zuro/memory.md"); }
        parts.push(format_context_block("memory: global", content));
    }
    if let Some(content) = &memory.local {
        if verbose { eprintln!("[provider] local_memory: .zuro/memory.md"); }
        parts.push(format_context_block("memory: local", content));
    }
    if let Some(content) = &memory.local_private {
        if verbose { eprintln!("[provider] local_memory_private: .zuro/memory.local.md"); }
        parts.push(format_context_block("memory: private", content));
    }

    for item in resolved_items {
        if verbose { eprintln!("[provider] pool: {}", item.label); }
        parts.push(format_context_block(&format!("context: {}", item.label), &item.content));
    }

    for f in files {
        if verbose { eprintln!("[provider] file: {}", f.path); }
        parts.push(format_context_block(&format!("file: {}", f.path), &f.content));
    }

    if let Some(content) = stdin {
        if verbose { eprintln!("[provider] stdin"); }
        parts.push(format_context_block("stdin", content));
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use crate::pool::ResolvedItem;
    use crate::template::FileArg;

    fn make_root() -> TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn assemble_system_message_empty_when_no_files() {
        let tmp = make_root();
        let msg = assemble_system_message(Some(tmp.path()), false);
        assert!(msg.is_empty());
    }

    #[test]
    fn assemble_system_message_reads_local() {
        let tmp = make_root();
        let path = tmp.path().join(".zuro").join("system.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "You are a helpful assistant.").unwrap();
        let msg = assemble_system_message(Some(tmp.path()), false);
        assert_eq!(msg, "You are a helpful assistant.");
    }

    #[test]
    fn assemble_user_prefix_empty_when_nothing() {
        let mem = MemoryContent { global: None, local: None, local_private: None };
        let prefix = assemble_user_prefix(&mem, &[], &[], None, false);
        assert!(prefix.is_empty());
    }

    #[test]
    fn assemble_user_prefix_includes_global_memory() {
        let mem = MemoryContent { global: Some("global notes".into()), local: None, local_private: None };
        let prefix = assemble_user_prefix(&mem, &[], &[], None, false);
        assert!(prefix.contains("[memory: global]"));
        assert!(prefix.contains("global notes"));
    }

    #[test]
    fn assemble_user_prefix_includes_pool_item() {
        let mem = MemoryContent { global: None, local: None, local_private: None };
        let pool = vec![ResolvedItem { label: "note".into(), content: "pool content".into() }];
        let prefix = assemble_user_prefix(&mem, &pool, &[], None, false);
        assert!(prefix.contains("[context: note]"));
        assert!(prefix.contains("pool content"));
    }

    #[test]
    fn assemble_user_prefix_includes_files() {
        let mem = MemoryContent { global: None, local: None, local_private: None };
        let files = vec![FileArg { path: "main.rs".into(), content: "fn main() {}".into() }];
        let prefix = assemble_user_prefix(&mem, &[], &files, None, false);
        assert!(prefix.contains("[file: main.rs]"));
        assert!(prefix.contains("fn main() {}"));
    }

    #[test]
    fn assemble_user_prefix_includes_stdin_context() {
        let mem = MemoryContent { global: None, local: None, local_private: None };
        let prefix = assemble_user_prefix(&mem, &[], &[], Some("piped input"), false);
        assert!(prefix.contains("[stdin]"));
        assert!(prefix.contains("piped input"));
    }

    #[test]
    fn assemble_user_prefix_ordering_memory_then_pool_then_files_then_stdin() {
        let mem = MemoryContent { global: Some("mem".into()), local: None, local_private: None };
        let pool = vec![ResolvedItem { label: "ctx".into(), content: "pool".into() }];
        let files = vec![FileArg { path: "f.rs".into(), content: "file".into() }];
        let prefix = assemble_user_prefix(&mem, &pool, &files, Some("stdin data"), false);
        let mem_pos   = prefix.find("[memory: global]").unwrap();
        let pool_pos  = prefix.find("[context: ctx]").unwrap();
        let file_pos  = prefix.find("[file: f.rs]").unwrap();
        let stdin_pos = prefix.find("[stdin]").unwrap();
        assert!(mem_pos < pool_pos);
        assert!(pool_pos < file_pos);
        assert!(file_pos < stdin_pos);
    }
}
