mod io;

pub use io::{append_to_memory, clear_memory, load_memory, show_memory};

#[derive(Default)]
pub struct MemoryContent {
    pub global:        Option<String>,
    pub local:         Option<String>,
    pub local_private: Option<String>,
}

pub enum MemoryLocation {
    Global,
    Local,
    LocalPrivate,
}
