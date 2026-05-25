use std::path::Path;
use std::process::Command;

use anyhow::Context;
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;

use super::PoolItem;

pub enum ResolvedItemKind {
    Note,
    File    { path: String },
    Command { cmd: String, exit_code: i32 },
}

impl ResolvedItemKind {
    pub fn display(&self) -> String {
        match self {
            Self::Note                      => "note".to_string(),
            Self::File    { path }          => path.clone(),
            Self::Command { cmd, exit_code } => format!("cmd: {cmd} (exit {exit_code})"),
        }
    }
}

pub struct ResolvedItem {
    pub kind:    ResolvedItemKind,
    pub content: String,
}

fn collect_walked_files(
    walker:       ignore::Walk,
    error_context: impl Fn(&Path) -> String,
) -> anyhow::Result<Vec<(std::path::PathBuf, String)>> {
    let mut files = Vec::new();
    for entry in walker {
        let entry = entry?;
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
        let path = entry.into_path();
        let content = std::fs::read_to_string(&path)
            .with_context(|| error_context(&path))?;
        files.push((path, content));
    }
    Ok(files)
}

fn make_walker(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    if let Some(global_ignore) = dirs::home_dir()
        .map(|h| h.join(".zuro").join(".zuro-ignore"))
        .filter(|p| p.exists())
    {
        builder.add_ignore(global_ignore);
    }
    builder.add_custom_ignore_filename(".zuro-ignore");
    builder
}

pub fn expand_pool(pool: &[PoolItem], shell: &str, verbose: bool) -> anyhow::Result<Vec<ResolvedItem>> {
    let mut result = Vec::new();
    for item in pool {
        match item {
            PoolItem::Text { content } => {
                if verbose { eprintln!("[pool] text note"); }
                result.push(ResolvedItem { kind: ResolvedItemKind::Note, content: content.clone() });
            }
            PoolItem::File { path } => {
                if verbose { eprintln!("[pool] file: {}", path.display()); }
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Cannot read pool file: {}", path.display()))?;
                result.push(ResolvedItem {
                    kind: ResolvedItemKind::File { path: path.to_string_lossy().into_owned() },
                    content,
                });
            }
            PoolItem::Glob { pattern, base } => {
                if verbose { eprintln!("[pool] glob: {pattern} in {}", base.display()); }
                let mut ob = OverrideBuilder::new(base);
                ob.add(pattern)?;
                let overrides = ob.build()?;
                let walker = make_walker(base).overrides(overrides).build();
                for (path, content) in collect_walked_files(walker, |p| {
                    format!("Cannot read glob match '{}' (pattern '{}' in '{}')", p.display(), pattern, base.display())
                })? {
                    if verbose { eprintln!("[pool]   → {}", path.display()); }
                    result.push(ResolvedItem {
                        kind: ResolvedItemKind::File { path: path.to_string_lossy().into_owned() },
                        content,
                    });
                }
            }
            PoolItem::Dir { path } => {
                if verbose { eprintln!("[pool] dir: {}", path.display()); }
                let walker = make_walker(path).build();
                for (file_path, content) in collect_walked_files(walker, |p| {
                    format!("Cannot read file '{}' from directory '{}'", p.display(), path.display())
                })? {
                    if verbose { eprintln!("[pool]   → {}", file_path.display()); }
                    result.push(ResolvedItem {
                        kind: ResolvedItemKind::File { path: file_path.to_string_lossy().into_owned() },
                        content,
                    });
                }
            }
            PoolItem::Command { cmd } => {
                if verbose { eprintln!("[pool] command: {cmd}"); }
                let output = Command::new(shell)
                    .arg("-c")
                    .arg(cmd)
                    .output()
                    .with_context(|| format!("Failed to run pool command: {cmd}"))?;
                let exit_code = output.status.code().unwrap_or(-1);
                let content   = String::from_utf8_lossy(&output.stdout).into_owned();
                if verbose { eprintln!("[pool]   exit_code={exit_code}"); }
                result.push(ResolvedItem {
                    kind: ResolvedItemKind::Command { cmd: cmd.clone(), exit_code },
                    content,
                });
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_dir() -> TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn expand_text_item() {
        let items = vec![PoolItem::Text { content: "hello".into() }];
        let resolved = expand_pool(&items, "sh", false).unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(matches!(resolved[0].kind, ResolvedItemKind::Note));
        assert_eq!(resolved[0].content, "hello");
    }

    #[test]
    fn expand_file_item() {
        let tmp = make_dir();
        let path = tmp.path().join("foo.txt");
        fs::write(&path, "bar content").unwrap();
        let items = vec![PoolItem::File { path: path.clone() }];
        let resolved = expand_pool(&items, "sh", false).unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(matches!(&resolved[0].kind, ResolvedItemKind::File { path } if path.contains("foo.txt")));
        assert_eq!(resolved[0].content, "bar content");
    }

    #[test]
    fn expand_file_item_missing_errors() {
        let items = vec![PoolItem::File { path: "/nonexistent/path/file.rs".into() }];
        assert!(expand_pool(&items, "sh", false).is_err());
    }

    #[test]
    fn expand_glob_item_matches_files() {
        let tmp = make_dir();
        fs::write(tmp.path().join("a.rs"), "a").unwrap();
        fs::write(tmp.path().join("b.rs"), "b").unwrap();
        fs::write(tmp.path().join("c.txt"), "c").unwrap();
        let items = vec![PoolItem::Glob {
            pattern: "*.rs".into(),
            base: tmp.path().to_path_buf(),
        }];
        let resolved = expand_pool(&items, "sh", false).unwrap();
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn expand_dir_item_reads_all_files() {
        let tmp = make_dir();
        fs::write(tmp.path().join("x.rs"), "x").unwrap();
        fs::write(tmp.path().join("y.rs"), "y").unwrap();
        let items = vec![PoolItem::Dir { path: tmp.path().to_path_buf() }];
        let resolved = expand_pool(&items, "sh", false).unwrap();
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn expand_command_item_captures_stdout() {
        let items = vec![PoolItem::Command { cmd: "echo hello".into() }];
        let resolved = expand_pool(&items, "sh", false).unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(matches!(&resolved[0].kind, ResolvedItemKind::Command { exit_code: 0, .. }));
        assert!(resolved[0].content.contains("hello"));
    }

    #[test]
    fn expand_command_item_nonzero_exit_still_returns_output() {
        let items = vec![PoolItem::Command { cmd: "echo err; exit 1".into() }];
        let resolved = expand_pool(&items, "sh", false).unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(matches!(&resolved[0].kind, ResolvedItemKind::Command { exit_code: 1, .. }));
        assert!(resolved[0].content.contains("err"));
    }
}
