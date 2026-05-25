use std::fs;
use std::path::Path;

use crate::constants::{ZURO_DIR, SYSTEM_FILE};
use crate::memory::MemoryContent;
use crate::pool::{ResolvedItem, ResolvedItemKind};
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

fn xml_block(tag: &str, attrs: &str, content: &str) -> String {
    if attrs.is_empty() {
        format!("<{tag}>\n{content}\n</{tag}>")
    } else {
        format!("<{tag} {attrs}>\n{content}\n</{tag}>")
    }
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
        parts.push(xml_block("memory", "scope=\"global\"", content));
    }
    if let Some(content) = &memory.local {
        if verbose { eprintln!("[provider] local_memory: .zuro/memory.md"); }
        parts.push(xml_block("memory", "scope=\"local\"", content));
    }
    if let Some(content) = &memory.local_private {
        if verbose { eprintln!("[provider] local_memory_private: .zuro/memory.local.md"); }
        parts.push(xml_block("memory", "scope=\"private\"", content));
    }

    for item in resolved_items {
        if verbose { eprintln!("[provider] pool: {}", item.kind.display()); }
        let block = match &item.kind {
            ResolvedItemKind::Note => {
                xml_block("context", "type=\"note\"", &item.content)
            }
            ResolvedItemKind::File { path } => {
                xml_block("context", &format!("type=\"file\" path=\"{path}\""), &item.content)
            }
            ResolvedItemKind::Command { cmd, exit_code } => {
                xml_block("context", &format!("type=\"cmd\" cmd=\"{cmd}\" exit=\"{exit_code}\""), &item.content)
            }
        };
        parts.push(block);
    }

    for f in files {
        if verbose { eprintln!("[provider] file: {}", f.path); }
        parts.push(xml_block("context", &format!("type=\"file\" path=\"{}\"", f.path), &f.content));
    }

    if let Some(content) = stdin {
        if verbose { eprintln!("[provider] stdin"); }
        parts.push(xml_block("context", "type=\"stdin\"", content));
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use crate::pool::{ResolvedItem, ResolvedItemKind};
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
        assert!(prefix.contains("<memory scope=\"global\">"));
        assert!(prefix.contains("global notes"));
        assert!(prefix.contains("</memory>"));
    }

    #[test]
    fn assemble_user_prefix_includes_pool_note() {
        let mem  = MemoryContent { global: None, local: None, local_private: None };
        let pool = vec![ResolvedItem { kind: ResolvedItemKind::Note, content: "pool content".into() }];
        let prefix = assemble_user_prefix(&mem, &pool, &[], None, false);
        assert!(prefix.contains("<context type=\"note\">"));
        assert!(prefix.contains("pool content"));
        assert!(prefix.contains("</context>"));
    }

    #[test]
    fn assemble_user_prefix_includes_pool_file() {
        let mem  = MemoryContent { global: None, local: None, local_private: None };
        let pool = vec![ResolvedItem {
            kind:    ResolvedItemKind::File { path: "/src/main.rs".into() },
            content: "fn main() {}".into(),
        }];
        let prefix = assemble_user_prefix(&mem, &pool, &[], None, false);
        assert!(prefix.contains("type=\"file\" path=\"/src/main.rs\""));
        assert!(prefix.contains("fn main() {}"));
    }

    #[test]
    fn assemble_user_prefix_includes_pool_command() {
        let mem  = MemoryContent { global: None, local: None, local_private: None };
        let pool = vec![ResolvedItem {
            kind:    ResolvedItemKind::Command { cmd: "echo hi".into(), exit_code: 0 },
            content: "hi\n".into(),
        }];
        let prefix = assemble_user_prefix(&mem, &pool, &[], None, false);
        assert!(prefix.contains("type=\"cmd\" cmd=\"echo hi\" exit=\"0\""));
        assert!(prefix.contains("hi\n"));
    }

    #[test]
    fn assemble_user_prefix_includes_files() {
        let mem   = MemoryContent { global: None, local: None, local_private: None };
        let files = vec![FileArg { path: "main.rs".into(), content: "fn main() {}".into() }];
        let prefix = assemble_user_prefix(&mem, &[], &files, None, false);
        assert!(prefix.contains("type=\"file\" path=\"main.rs\""));
        assert!(prefix.contains("fn main() {}"));
    }

    #[test]
    fn assemble_user_prefix_includes_stdin_context() {
        let mem = MemoryContent { global: None, local: None, local_private: None };
        let prefix = assemble_user_prefix(&mem, &[], &[], Some("piped input"), false);
        assert!(prefix.contains("<context type=\"stdin\">"));
        assert!(prefix.contains("piped input"));
    }

    #[test]
    fn assemble_user_prefix_ordering_memory_then_pool_then_files_then_stdin() {
        let mem  = MemoryContent { global: Some("mem".into()), local: None, local_private: None };
        let pool = vec![ResolvedItem { kind: ResolvedItemKind::Note, content: "pool".into() }];
        let files = vec![FileArg { path: "f.rs".into(), content: "file".into() }];
        let prefix = assemble_user_prefix(&mem, &pool, &files, Some("stdin data"), false);
        let mem_pos   = prefix.find("<memory scope=\"global\">").unwrap();
        let pool_pos  = prefix.find("<context type=\"note\">").unwrap();
        let file_pos  = prefix.find("path=\"f.rs\"").unwrap();
        let stdin_pos = prefix.find("<context type=\"stdin\">").unwrap();
        assert!(mem_pos < pool_pos);
        assert!(pool_pos < file_pos);
        assert!(file_pos < stdin_pos);
    }
}
