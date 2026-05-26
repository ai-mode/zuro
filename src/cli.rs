use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "zuro", about = "CLI for LLM conversations")]
pub struct Cli {
    /// Prompt to send (omit to read from stdin pipe)
    pub prompt: Option<String>,

    /// Profile name (overrides config default)
    #[arg(long)]
    pub profile: Option<String>,

    /// Model name (overrides profile config)
    #[arg(long)]
    pub model: Option<String>,

    /// Session ID (overrides active session)
    #[arg(long)]
    pub session: Option<String>,

    /// Do not log this exchange to the session
    #[arg(long)]
    pub no_log: bool,

    /// Send without any session: no history, no pool, nothing saved
    #[arg(long)]
    pub no_session: bool,

    /// Enable streaming output
    #[arg(long)]
    pub stream: bool,

    /// Output format: text | json
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Show progress bar while waiting for response
    #[arg(long)]
    pub progress: bool,

    /// Print token usage and model info after response
    #[arg(long)]
    pub stats: bool,

    /// Verbose curl-like request/response dump to stderr
    #[arg(short, long)]
    pub verbose: bool,

    /// Assemble the full request and print it to stderr without sending
    #[arg(long)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Session management
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Profile management
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
    /// Shell integration
    Shell {
        #[command(subcommand)]
        action: ShellAction,
    },
    /// List available models
    Models {
        #[arg(long)]
        profile: Option<String>,
    },
    /// Show configuration and environment info
    Info,
    /// Run a named command
    Run {
        /// Command name (e.g. fix, explain, review)
        command: String,
        /// Input values passed positionally to declared inputs (repeatable)
        #[arg(long)]
        input: Vec<String>,
        /// Files to include in the command context
        #[arg(long = "file")]
        files: Vec<PathBuf>,
    },
    /// Manage named commands
    Commands {
        #[command(subcommand)]
        action: CmdAction,
    },
    /// Manage memory files
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Manage the session context pool
    Context {
        #[command(subcommand)]
        action: CtxAction,
    },
    /// Start an interactive chat session
    Repl {
        /// Limit how many recent exchanges are included in context (overrides config)
        #[arg(long)]
        history: Option<usize>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// Create a new session; prints export command for shell eval
    New,
    /// Fork the active session; prints export command for shell eval
    Fork {
        /// Fork from a specific exchange ID prefix (defaults to end of session)
        #[arg(long)]
        at: Option<String>,
    },
    /// Switch to an existing session; prints export command for shell eval
    Use { id: String },
    /// Set the global active session (affects all terminals without $ZURO_SESSION set)
    SetGlobal {
        /// Session ID; defaults to current active session
        id: Option<String>,
    },
    /// List all sessions
    List,
    /// Show history of a session
    Show {
        /// Session ID (defaults to active session)
        id: Option<String>,
        /// Output format: text | chat | table | json
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Show token usage statistics
    Stats {
        /// Session ID (defaults to active session)
        id: Option<String>,
    },
    /// Delete a session by ID or prefix
    Delete {
        /// Session ID or 8-char prefix
        id: String,
    },
    /// Delete all sessions
    Clear {
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ShellAction {
    /// Print or install the shell wrapper function for session commands
    Init {
        /// Write to shell config instead of printing
        #[arg(long)]
        install: bool,
        /// Shell to target: bash, zsh, fish (default: auto-detect)
        #[arg(long)]
        shell: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProfileAction {
    /// List configured profiles
    List,
    /// Show details of a profile
    Show { name: Option<String> },
    /// Set the default profile
    Use { name: String },
    /// Add or update a profile
    Add {
        /// Profile name — if omitted, runs interactive wizard
        name: Option<String>,
        /// Provider type: openai or anthropic
        #[arg(long)]
        r#type: Option<String>,
        /// API key
        #[arg(long)]
        key: Option<String>,
        /// Model name
        #[arg(long)]
        model: Option<String>,
        /// API base URL
        #[arg(long)]
        url: Option<String>,
        /// Make this the default profile
        #[arg(long)]
        set_default: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum CmdAction {
    /// List all available commands
    List,
    /// Show the template for a command
    Show {
        /// Command name
        name: String,
    },
    /// Open a command file in the configured editor
    Edit {
        /// Command name (e.g. "fix", "review")
        name: String,
        /// Target ~/.zuro/commands/ instead of .zuro/commands/
        #[arg(long)]
        global: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum MemoryAction {
    /// Add text or file contents to memory
    Add {
        /// Inline text to append (reads from stdin if omitted)
        text: Option<String>,
        /// Read content from a file instead
        #[arg(long)]
        file: Option<PathBuf>,
        /// Write to project memory (.zuro/memory.md)
        #[arg(long)]
        local: bool,
        /// Write to private project memory (.zuro/memory.local.md)
        #[arg(long)]
        private: bool,
    },
    /// Show memory contents
    Show {
        #[arg(long)]
        global: bool,
        #[arg(long)]
        local: bool,
        #[arg(long)]
        private: bool,
    },
    /// Clear memory
    Clear {
        #[arg(long)]
        global: bool,
        #[arg(long)]
        local: bool,
        #[arg(long)]
        private: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum CtxAction {
    /// Add files, directories, glob patterns, text notes, or commands to the pool
    Add {
        /// Files, directories, or glob patterns to add
        paths: Vec<PathBuf>,
        /// Inline text note (reads from stdin if flag is given without a value)
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        text: Option<String>,
        /// Shell command whose stdout is included in context (re-run each request)
        #[arg(long)]
        cmd: Option<String>,
        /// Write to project pool (.zuro/pool.json)
        #[arg(long)]
        project: bool,
        /// Write to global pool (~/.zuro/pool.json)
        #[arg(long)]
        global: bool,
    },
    /// List pool items across all active levels
    List,
    /// Remove a pool item interactively
    Remove,
    /// Clear pool items
    Clear {
        /// Clear project pool (.zuro/pool.json)
        #[arg(long)]
        project: bool,
        /// Clear local pool (.zuro/pool.local.json)
        #[arg(long)]
        local: bool,
        /// Clear global pool (~/.zuro/pool.json)
        #[arg(long)]
        global: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}
