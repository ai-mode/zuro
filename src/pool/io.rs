use std::fs;
use std::path::Path;

use anyhow::Context;

use crate::constants::POOL_FILE;
use super::PoolItem;

fn pool_file_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join(POOL_FILE)
}

pub fn load_pool(session_dir: &Path) -> anyhow::Result<Vec<PoolItem>> {
    let path = pool_file_path(session_dir);
    if !path.exists() { return Ok(vec![]); }
    let s = fs::read_to_string(&path)
        .with_context(|| format!("Cannot read pool: {}", path.display()))?;
    serde_json::from_str::<Vec<PoolItem>>(&s)
        .with_context(|| format!("Failed to parse pool file: {}", path.display()))
}

pub fn save_pool(session_dir: &Path, pool: &[PoolItem]) -> anyhow::Result<()> {
    let path = pool_file_path(session_dir);
    let tmp  = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(pool)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn add_items(session_dir: &Path, items: Vec<PoolItem>) -> anyhow::Result<()> {
    let mut pool = load_pool(session_dir)?;
    pool.extend(items);
    save_pool(session_dir, &pool)
}

pub fn clear_pool(session_dir: &Path) -> anyhow::Result<()> {
    let path = pool_file_path(session_dir);
    if path.exists() { fs::remove_file(&path)?; }
    Ok(())
}

pub fn remove_item(session_dir: &Path, index: usize) -> anyhow::Result<()> {
    let mut pool = load_pool(session_dir)?;
    anyhow::ensure!(
        index < pool.len(),
        "Index {index} out of range (pool has {} items)", pool.len()
    );
    pool.remove(index);
    save_pool(session_dir, &pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_dir() -> TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn load_pool_returns_empty_when_no_file() {
        let tmp = make_dir();
        let pool = load_pool(tmp.path()).unwrap();
        assert!(pool.is_empty());
    }

    #[test]
    fn add_items_creates_pool_file() {
        let tmp = make_dir();
        add_items(tmp.path(), vec![PoolItem::Text { content: "hello".into() }]).unwrap();
        assert!(tmp.path().join(POOL_FILE).exists());
        let pool = load_pool(tmp.path()).unwrap();
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn add_items_appends_to_existing() {
        let tmp = make_dir();
        add_items(tmp.path(), vec![PoolItem::Text { content: "first".into() }]).unwrap();
        add_items(tmp.path(), vec![PoolItem::Text { content: "second".into() }]).unwrap();
        let pool = load_pool(tmp.path()).unwrap();
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn remove_item_removes_by_index() {
        let tmp = make_dir();
        add_items(tmp.path(), vec![
            PoolItem::Text { content: "a".into() },
            PoolItem::Text { content: "b".into() },
            PoolItem::Text { content: "c".into() },
        ]).unwrap();
        remove_item(tmp.path(), 1).unwrap();
        let pool = load_pool(tmp.path()).unwrap();
        assert_eq!(pool.len(), 2);
        let labels: Vec<_> = pool.iter().map(|p| match p {
            PoolItem::Text { content } => content.as_str(),
            _ => "",
        }).collect();
        assert_eq!(labels, vec!["a", "c"]);
    }

    #[test]
    fn remove_item_out_of_range_errors() {
        let tmp = make_dir();
        add_items(tmp.path(), vec![PoolItem::Text { content: "x".into() }]).unwrap();
        assert!(remove_item(tmp.path(), 5).is_err());
    }

    #[test]
    fn clear_pool_empties_the_pool() {
        let tmp = make_dir();
        add_items(tmp.path(), vec![
            PoolItem::Text { content: "a".into() },
            PoolItem::Text { content: "b".into() },
        ]).unwrap();
        clear_pool(tmp.path()).unwrap();
        let pool = load_pool(tmp.path()).unwrap();
        assert!(pool.is_empty());
    }

    #[test]
    fn roundtrip_all_pool_item_types() {
        let tmp = make_dir();
        let items = vec![
            PoolItem::Text    { content: "note".into() },
            PoolItem::File    { path: PathBuf::from("/tmp/foo.rs") },
            PoolItem::Glob    { pattern: "**/*.rs".into(), base: PathBuf::from("/tmp") },
            PoolItem::Dir     { path: PathBuf::from("/tmp/src") },
            PoolItem::Command { cmd: "echo hi".into() },
        ];
        add_items(tmp.path(), items).unwrap();
        let loaded = load_pool(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 5);
        assert!(matches!(loaded[0], PoolItem::Text { .. }));
        assert!(matches!(loaded[1], PoolItem::File { .. }));
        assert!(matches!(loaded[2], PoolItem::Glob { .. }));
        assert!(matches!(loaded[3], PoolItem::Dir { .. }));
        assert!(matches!(loaded[4], PoolItem::Command { .. }));
    }
}
