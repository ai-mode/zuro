use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;

const MARKER: &str = "# zuro shell integration";

const BASH_ZSH_SNIPPET: &str = r#"# zuro shell integration
zuro() {
    case "$1 $2" in
        "session use"|"session new"|"session fork")
            for _zuro_arg in "$@"; do
                case "$_zuro_arg" in
                    --help|-h) command zuro "$@"; return $?;;
                esac
            done
            eval "$(command zuro "$@")"
            return $?
            ;;
    esac
    command zuro "$@"
}
"#;

const FISH_SNIPPET: &str = r#"# zuro shell integration
function zuro
    switch "$argv[1] $argv[2]"
        case "session use" "session new" "session fork"
            if contains -- --help $argv; or contains -- -h $argv
                command zuro $argv
            else
                eval (command zuro $argv)
            end
        case '*'
            command zuro $argv
    end
end
"#;

pub enum Shell { Bash, Zsh, Fish }

pub fn detect_shell() -> Shell {
    std::env::var("SHELL").ok()
        .and_then(|s| {
            if      s.ends_with("zsh")  { Some(Shell::Zsh)  }
            else if s.ends_with("bash") { Some(Shell::Bash) }
            else if s.ends_with("fish") { Some(Shell::Fish) }
            else { None }
        })
        .unwrap_or(Shell::Bash)
}

pub fn parse_shell(s: &str) -> anyhow::Result<Shell> {
    match s {
        "bash" => Ok(Shell::Bash),
        "zsh"  => Ok(Shell::Zsh),
        "fish" => Ok(Shell::Fish),
        other  => anyhow::bail!("Unknown shell '{}'. Supported: bash, zsh, fish", other),
    }
}

pub fn shell_name(shell: &Shell) -> &'static str {
    match shell {
        Shell::Bash => "bash",
        Shell::Zsh  => "zsh",
        Shell::Fish => "fish",
    }
}

pub fn snippet(shell: &Shell) -> &'static str {
    match shell {
        Shell::Bash | Shell::Zsh => BASH_ZSH_SNIPPET,
        Shell::Fish              => FISH_SNIPPET,
    }
}

pub fn config_path(shell: &Shell) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    match shell {
        Shell::Bash => home.join(".bashrc"),
        Shell::Zsh  => home.join(".zshrc"),
        Shell::Fish => home.join(".config").join("fish").join("functions").join("zuro.fish"),
    }
}

pub fn is_installed(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|s| s.contains(MARKER))
        .unwrap_or(false)
}

pub fn print_snippet(shell: &Shell) {
    let path = config_path(shell);
    let name = shell_name(shell);
    eprintln!("# Shell: {name}  →  {}", path.display());
    eprintln!("# -----------------------------------------------");
    print!("{}", snippet(shell));
    eprintln!("# -----------------------------------------------");
    eprintln!("# Option 1 — append manually:");
    eprintln!("#   zuro shell init >> {}", path.display());
    eprintln!("# Option 2 — install automatically:");
    eprintln!("#   zuro shell init --install");
}

pub fn install(shell: &Shell) -> anyhow::Result<()> {
    let path = config_path(shell);
    let name = shell_name(shell);

    println!("Detected shell: {name}");

    if is_installed(&path) {
        println!("Shell integration already present in {} — nothing to do.", path.display());
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create directory {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Cannot open {} for writing", path.display()))?;

    write!(file, "\n{}", snippet(shell))
        .with_context(|| format!("Cannot write to {}", path.display()))?;

    println!("Appending to {} ... done", path.display());
    println!("Restart your shell or run: source {}", path.display());
    Ok(())
}
