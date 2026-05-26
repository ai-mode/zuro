use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::constants::{ZURO_DIR, POOL_PROJECT_FILE, POOL_LOCAL_FILE};
use super::PoolItem;

pub fn global_pool_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".zuro")
        .join(POOL_PROJECT_FILE)
}

pub fn project_pool_path(project_root: &Path) -> PathBuf {
    project_root.join(ZURO_DIR).join(POOL_PROJECT_FILE)
}

pub fn local_pool_path(project_root: &Path) -> PathBuf {
    project_root.join(ZURO_DIR).join(POOL_LOCAL_FILE)
}

pub fn load_pool(path: &Path) -> anyhow::Result<Vec<PoolItem>> {
    if !path.exists() { return Ok(vec![]); }
    let s = fs::read_to_string(path)
        .with_context(|| format!("Cannot read pool: {}", path.display()))?;
    serde_json::from_str::<Vec<PoolItem>>(&s)
        .with_context(|| format!("Failed to parse pool file: {}", path.display()))
}

pub fn save_pool(path: &Path, pool: &[PoolItem]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(pool)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn add_items(path: &Path, items: Vec<PoolItem>) -> anyhow::Result<()> {
    let mut pool = load_pool(path)?;
    pool.extend(items);
    save_pool(path, &pool)
}

pub fn clear_pool(path: &Path) -> anyhow::Result<()> {
    if path.exists() { fs::remove_file(path)?; }
    Ok(())
}

pub fn remove_item(path: &Path, index: usize) -> anyhow::Result<()> {
    let mut pool = load_pool(path)?;
    anyhow::ensure!(
        index < pool.len(),
        "Index {index} out of range (pool has {} items)", pool.len()
    );
    pool.remove(index);
    save_pool(path, &pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_dir() -> TempDir { tempfile::tempdir().unwrap() }
    fn pool_path(dir: &TempDir) -> PathBuf { dir.path().join("pool.json") }

    #[test]
    fn load_pool_returns_empty_when_no_file() {
        let tmp = make_dir();
        let pool = load_pool(&pool_path(&tmp)).unwrap();
        assert!(pool.is_empty());
    }

    #[test]
    fn add_items_creates_pool_file() {
        let tmp = make_dir();
        let path = pool_path(&tmp);
        add_items(&path, vec![PoolItem::Text { content: "hello".into() }]).unwrap();
        assert!(path.exists());
        let pool = load_pool(&path).unwrap();
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn add_items_appends_to_existing() {
        let tmp = make_dir();
        let path = pool_path(&tmp);
        add_items(&path, vec![PoolItem::Text { content: "first".into() }]).unwrap();
        add_items(&path, vec![PoolItem::Text { content: "second".into() }]).unwrap();
        let pool = load_pool(&path).unwrap();
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn remove_item_removes_by_index() {
        let tmp  = make_dir();
        let path = pool_path(&tmp);
        add_items(&path, vec![
            PoolItem::Text { content: "a".into() },
            PoolItem::Text { content: "b".into() },
            PoolItem::Text { content: "c".into() },
        ]).unwrap();
        remove_item(&path, 1).unwrap();
        let pool = load_pool(&path).unwrap();
        assert_eq!(pool.len(), 2);
        let labels: Vec<_> = pool.iter().map(|p| match p {
            PoolItem::Text { content } => content.as_str(),
            _ => "",
        }).collect();
        assert_eq!(labels, vec!["a", "c"]);
    }

    #[test]
    fn remove_item_out_of_range_errors() {
        let tmp  = make_dir();
        let path = pool_path(&tmp);
        add_items(&path, vec![PoolItem::Text { content: "x".into() }]).unwrap();
        assert!(remove_item(&path, 5).is_err());
    }

    #[test]
    fn clear_pool_empties_the_pool() {
        let tmp  = make_dir();
        let path = pool_path(&tmp);
        add_items(&path, vec![
            PoolItem::Text { content: "a".into() },
            PoolItem::Text { content: "b".into() },
        ]).unwrap();
        clear_pool(&path).unwrap();
        let pool = load_pool(&path).unwrap();
        assert!(pool.is_empty());
    }

    #[test]
    fn roundtrip_all_pool_item_types() {
        let tmp  = make_dir();
        let path = pool_path(&tmp);
        let items = vec![
            PoolItem::Text    { content: "note".into() },
            PoolItem::File    { path: PathBuf::from("/tmp/foo.rs") },
            PoolItem::Glob    { pattern: "**/*.rs".into(), base: PathBuf::from("/tmp") },
            PoolItem::Dir     { path: PathBuf::from("/tmp/src") },
            PoolItem::Command { cmd: "echo hi".into() },
        ];
        add_items(&path, items).unwrap();
        let loaded = load_pool(&path).unwrap();
        assert_eq!(loaded.len(), 5);
        assert!(matches!(loaded[0], PoolItem::Text { .. }));
        assert!(matches!(loaded[1], PoolItem::File { .. }));
        assert!(matches!(loaded[2], PoolItem::Glob { .. }));
        assert!(matches!(loaded[3], PoolItem::Dir { .. }));
        assert!(matches!(loaded[4], PoolItem::Command { .. }));
    }

    #[test]
    fn add_items_creates_parent_dirs() {
        let tmp  = make_dir();
        let path = tmp.path().join(".zuro").join("pool.json");
        add_items(&path, vec![PoolItem::Text { content: "hi".into() }]).unwrap();
        assert!(path.exists());
    }
}
