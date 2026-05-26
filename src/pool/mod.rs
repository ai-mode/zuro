mod expand;
mod io;

pub use expand::{expand_pool, ResolvedItem, ResolvedItemKind};
pub use io::{
    add_items, clear_pool, remove_item,
    global_pool_path, project_pool_path, local_pool_path,
};

use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::config::{LocalMerge, ProjectConfig};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PoolItem {
    Text    { content: String },
    File    { path: std::path::PathBuf },
    Glob    { pattern: String, base: std::path::PathBuf },
    Dir     { path: std::path::PathBuf },
    Command { cmd: String },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoolSource { Global, Project, Local }

pub struct TaggedPoolItem {
    pub source: PoolSource,
    pub item:   PoolItem,
}

pub fn load_execution_pool(
    project_root: Option<&Path>,
    project_cfg:  &ProjectConfig,
) -> anyhow::Result<Vec<PoolItem>> {
    let mut items = Vec::new();

    if project_cfg.pool.use_global {
        items.extend(io::load_pool(&io::global_pool_path())?);
    }

    if let Some(root) = project_root {
        let project_items = io::load_pool(&io::project_pool_path(root))?;
        let local_items   = io::load_pool(&io::local_pool_path(root))?;
        match project_cfg.pool.local_merge {
            LocalMerge::Append  => { items.extend(project_items); items.extend(local_items); }
            LocalMerge::Replace => { items.extend(local_items); }
        }
    } else if !project_cfg.pool.use_global {
        items.extend(io::load_pool(&io::global_pool_path())?);
    }

    Ok(items)
}

pub fn load_tagged_pool(
    project_root: Option<&Path>,
    project_cfg:  &ProjectConfig,
) -> anyhow::Result<Vec<TaggedPoolItem>> {
    let mut tagged = Vec::new();

    if project_cfg.pool.use_global {
        for item in io::load_pool(&io::global_pool_path())? {
            tagged.push(TaggedPoolItem { source: PoolSource::Global, item });
        }
    }

    if let Some(root) = project_root {
        match project_cfg.pool.local_merge {
            LocalMerge::Append => {
                for item in io::load_pool(&io::project_pool_path(root))? {
                    tagged.push(TaggedPoolItem { source: PoolSource::Project, item });
                }
                for item in io::load_pool(&io::local_pool_path(root))? {
                    tagged.push(TaggedPoolItem { source: PoolSource::Local, item });
                }
            }
            LocalMerge::Replace => {
                for item in io::load_pool(&io::local_pool_path(root))? {
                    tagged.push(TaggedPoolItem { source: PoolSource::Local, item });
                }
            }
        }
    } else if !project_cfg.pool.use_global {
        for item in io::load_pool(&io::global_pool_path())? {
            tagged.push(TaggedPoolItem { source: PoolSource::Global, item });
        }
    }

    Ok(tagged)
}
