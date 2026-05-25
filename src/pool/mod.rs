mod expand;
mod io;

pub use expand::{expand_pool, ResolvedItem, ResolvedItemKind};
pub use io::{add_items, clear_pool, load_pool, remove_item};

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PoolItem {
    Text    { content: String },
    File    { path: PathBuf },
    Glob    { pattern: String, base: PathBuf },
    Dir     { path: PathBuf },
    Command { cmd: String },
}
