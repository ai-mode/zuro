mod cli;
mod commands;
mod config;
mod constants;
mod defaults;
mod memory;
mod output;
mod pool;
mod provider;
mod providers;
mod session;
mod shell_init;
mod template;

use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use chrono::Local;
use clap::Parser;
use uuid::Uuid;

use std::collections::HashMap;

use crate::cli::{Cli, CmdAction, Commands, CtxAction, MemoryAction, ProfileAction, SessionAction, ShellAction};
use clap::CommandFactory;
use crate::commands::{CommandDef, CommandLocation, HistoryMode};
use crate::config::{Config, ProfileConfig};
use crate::constants::{SESSION_ID_PREFIX_LEN, TIMEOUT_CHAT_SECS, TIMEOUT_MODELS_SECS, ZURO_DIR};
use crate::memory::MemoryLocation;
use crate::output::{print_response, print_session_show, print_session_stats};
use crate::pool::PoolItem;
use crate::provider::{make_provider, mask_key, ChatMessage, Provider};
use crate::session::{Session, resolve_prefix, set_active};
use crate::template::{FileArg, TemplateContext};

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let mut cli  = Cli::parse();
    let config   = Config::load()?;
    let data_dir = config::data_dir();
    std::fs::create_dir_all(&data_dir)?;

    if let Some(cmd) = cli.command.take() {
        return match cmd {
            Commands::Session  { action }   => handle_session(action, &config, &data_dir),
            Commands::Profile  { action }   => handle_profile(action, &config),
            Commands::Shell    { action }   => handle_shell(action),
            Commands::Models   { profile }  => handle_models(profile.as_deref(), &config),
            Commands::Info                  => handle_info(&config, &data_dir),
            Commands::Run { command, input, files } => {
                let project_root = commands::find_project_root();
                handle_run(&cli, &command, &input, &files, &config, &data_dir, project_root.as_deref())
            }
            Commands::Commands { action } => {
                let project_root = commands::find_project_root();
                handle_commands(action, project_root.as_deref(), &config)
            }
            Commands::Memory { action } => {
                let project_root = commands::find_project_root();
                handle_memory(action, project_root.as_deref())
            }
            Commands::Context { action } => {
                let session_id = session::resolve(cli.session.as_deref(), &data_dir)?;
                let session = Session::open(&data_dir, &session_id)?;
                handle_context(action, &session, cli.verbose)
            }
        };
    }

    let stdin_input = read_stdin_if_piped()?;
    let (prompt, stdin_context) = match (cli.prompt.clone(), stdin_input) {
        (Some(p), Some(s)) => (p, Some(s)),
        (None,    Some(s)) => (s, None),
        (Some(p), None)    => (p, None),
        (None,    None)    => {
            Cli::command().print_help()?;
            println!();
            return Ok(());
        }
    };

    let (profile_name, profile_cfg) = config.active_profile(cli.profile.as_deref())?;
    let mut actual_cfg = profile_cfg.clone();
    if let Some(m) = &cli.model { actual_cfg.model = m.clone(); }

    anyhow::ensure!(
        !(cli.no_session && cli.session.is_some()),
        "--no-session and --session are mutually exclusive"
    );

    let project_root = commands::find_project_root();

    let (session_opt, resolved) = if cli.no_session {
        (None, vec![])
    } else {
        let session_id = session::resolve(cli.session.as_deref(), &data_dir)?;
        let session       = Session::open(&data_dir, &session_id)?;
        let pool_items = pool::load_pool(&session.dir)?;
        let shell      = config::resolve_shell(&config.default);
        let expanded   = pool::expand_pool(&pool_items, &shell, cli.verbose)?;
        (Some(session), expanded)
    };

    let memory      = memory::load_memory(project_root.as_deref());
    let system_msg  = providers::assemble_system_message(project_root.as_deref(), cli.verbose);
    let user_prefix = providers::assemble_user_prefix(&memory, &resolved, &[], stdin_context.as_deref(), cli.verbose);

    let final_user_content = combine_prefix_and_prompt(&user_prefix, &prompt);
    let exchange_id        = Uuid::new_v4().to_string();

    let mut messages = Vec::new();
    if !system_msg.is_empty() {
        messages.push(ChatMessage::system(system_msg));
    }
    if let Some(ref session) = session_opt {
        messages.extend(build_history(session, cli.no_log, None)?);
    }
    messages.push(ChatMessage::user(final_user_content));

    let prov = make_provider(&actual_cfg, Duration::from_secs(TIMEOUT_CHAT_SECS))?;

    if cli.dry_run {
        print_dry_run(&messages, &*prov, cli.stream);
        return Ok(());
    }

    if !cli.no_log {
        if let Some(ref session) = session_opt {
            session.append(&session::Exchange::now("user", prompt.clone(), session::ExchangeMeta {
                exchange_id: Some(exchange_id.clone()),
                ..Default::default()
            }))?;
        }
    }

    let show_stats = cli.stats || config.default.show_stats;
    execute_chat_and_log(&*prov, &messages, session_opt.as_ref(), exchange_id, profile_name, &cli, show_stats)
}

fn execute_chat_and_log(
    prov:         &dyn Provider,
    messages:     &[ChatMessage],
    session:         Option<&Session>,
    exchange_id:  String,
    profile_name: String,
    cli:          &Cli,
    show_stats:   bool,
) -> anyhow::Result<()> {
    let json          = cli.format == "json";
    let on_chunk      = make_chunk_callback(cli.stream, json);
    let stop_spinner  = run_spinner(cli.progress, cli.stream, json, cli.verbose);
    let request_start = Instant::now();
    let chat_result   = prov.chat(messages, cli.stream, cli.verbose, on_chunk.as_deref());
    let duration_ms   = Some(request_start.elapsed().as_millis() as u64);
    if let Some(stop) = stop_spinner { stop(); }
    let resp = chat_result?;

    if cli.stream && !json { println!(); }

    if !cli.no_log {
        if let Some(session) = session {
            session.append(&session::Exchange::now(
                "assistant",
                resp.content.clone(),
                session::ExchangeMeta {
                    exchange_id: Some(exchange_id),
                    model:       Some(resp.model.clone()),
                    provider:    Some(profile_name),
                    duration_ms,
                    usage: resp.usage.as_ref().map(|u| session::TokenUsage {
                        input_tokens:  u.input_tokens,
                        output_tokens: u.output_tokens,
                    }),
                },
            ))?;
        }
    }

    let session_id = session.map(|session| session.id.as_str()).unwrap_or("-");
    print_response(&resp.content, &resp.model, resp.usage.as_ref(), json, session_id, show_stats);
    Ok(())
}

fn handle_run(
    cli:          &Cli,
    command:      &str,
    input:        &[String],
    files:        &[PathBuf],
    config:       &Config,
    data_dir:     &Path,
    project_root: Option<&Path>,
) -> anyhow::Result<()> {
    let cmd_def = commands::resolve_command(command, project_root)?;

    let collected_inputs = collect_inputs(&cmd_def.frontmatter.inputs, input)?;

    let stdin_content = read_stdin_if_piped()?;

    let file_args = read_file_args(files)?;

    let (profile_name, profile_cfg) = config.active_profile(cli.profile.as_deref())?;
    let mut actual_cfg = profile_cfg.clone();
    if let Some(m) = &cli.model { actual_cfg.model = m.clone(); }

    let (session_opt, resolved) = if cli.no_session {
        (None, vec![])
    } else {
        let session_id = session::resolve(cli.session.as_deref(), data_dir)?;
        let session       = Session::open(data_dir, &session_id)?;
        let pool_items = pool::load_pool(&session.dir)?;
        let shell      = config::resolve_shell(&config.default);
        let expanded   = pool::expand_pool(&pool_items, &shell, cli.verbose)?;
        (Some(session), expanded)
    };

    let memory      = memory::load_memory(project_root);
    let system_msg  = providers::assemble_system_message(project_root, cli.verbose);
    let user_prefix = providers::assemble_user_prefix(&memory, &resolved, &file_args, None, cli.verbose);

    let cwd        = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let date       = Local::now().format("%Y-%m-%d").to_string();
    let session_id = session_opt.as_ref().map(|s| s.id.clone()).unwrap_or_default();

    let ctx = TemplateContext {
        stdin:      stdin_content.as_deref(),
        inputs:     &collected_inputs,
        files:      &file_args,
        memory:     &memory,
        model:      &actual_cfg.model,
        profile:    &profile_name,
        session_id: &session_id,
        cwd:        &cwd,
        date:       &date,
    };

    let final_prompt       = template::render(&cmd_def.template, &ctx)?;
    let final_user_content = combine_prefix_and_prompt(&user_prefix, &final_prompt);
    let exchange_id        = Uuid::new_v4().to_string();

    let mut messages = Vec::new();
    if !system_msg.is_empty() {
        messages.push(ChatMessage::system(system_msg));
    }
    if let Some(ref session) = session_opt {
        messages.extend(build_history(session, cli.no_log, history_limit(&cmd_def))?);
    }
    messages.push(ChatMessage::user(final_user_content));

    let prov = make_provider(&actual_cfg, Duration::from_secs(TIMEOUT_CHAT_SECS))?;

    if cli.dry_run {
        print_dry_run(&messages, &*prov, cli.stream);
        return Ok(());
    }

    if !cli.no_log {
        if let Some(ref session) = session_opt {
            session.append(&session::Exchange::now("user", final_prompt.clone(), session::ExchangeMeta {
                exchange_id: Some(exchange_id.clone()),
                ..Default::default()
            }))?;
        }
    }

    let show_stats = cli.stats || config.default.show_stats;
    execute_chat_and_log(&*prov, &messages, session_opt.as_ref(), exchange_id, profile_name, cli, show_stats)
}

fn collect_inputs(
    defs:       &[commands::InputDef],
    cli_inputs: &[String],
) -> anyhow::Result<HashMap<String, Option<String>>> {
    let mut result = HashMap::new();
    for (idx, def) in defs.iter().enumerate() {
        let raw = if let Some(v) = cli_inputs.get(idx) {
            v.clone()
        } else {
            read_from_tty(&def.display_prompt())?
        };
        let value = if raw.trim().is_empty() { None } else { Some(raw.trim().to_string()) };
        if value.is_none() && def.required {
            anyhow::bail!("Input '{}' is required for this command", def.name);
        }
        result.insert(def.name.clone(), value);
    }
    Ok(result)
}

fn history_limit(cmd: &CommandDef) -> Option<usize> {
    match cmd.frontmatter.history {
        HistoryMode::Small => Some(3),
        HistoryMode::Large => Some(20),
        HistoryMode::None  => Some(0),
        HistoryMode::Full  => None,
    }
}

fn handle_commands(action: CmdAction, project_root: Option<&Path>, config: &Config) -> anyhow::Result<()> {
    match action {
        CmdAction::List => {
            let cmds = commands::list_commands(project_root);
            if cmds.is_empty() {
                println!("No commands found.");
                return Ok(());
            }
            for cmd in &cmds {
                let loc_tag = match cmd.location {
                    CommandLocation::BuiltIn => "[B]",
                    CommandLocation::Global  => "[G]",
                    CommandLocation::Local   => "[L]",
                };
                let desc = cmd.frontmatter.description.as_deref().unwrap_or(&cmd.name);
                println!("  {:<20} {:<6} {}", cmd.name, loc_tag, desc);
            }
        }
        CmdAction::Show { name } => {
            let cmd = commands::resolve_command(&name, project_root)?;
            print!("{}", cmd.template);
        }
        CmdAction::Edit { name, global } => {
            let dir = if global {
                dirs::home_dir()
                    .map(|h| h.join(ZURO_DIR).join("commands"))
                    .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
            } else {
                let root = project_root.ok_or_else(|| anyhow::anyhow!(
                    "No project root found. Use --global for ~/.zuro/commands/, \
                     or run from inside a project with a .zuro/ or .git/ directory."
                ))?;
                root.join(ZURO_DIR).join("commands")
            };

            let matches = find_command_files_by_name(&dir, &name);

            let file_to_edit = if matches.is_empty() {
                std::fs::create_dir_all(&dir)?;
                let stub = dir.join(format!("{name}.md"));
                if !stub.exists() {
                    std::fs::write(&stub, format!(
                        "---\ndescription: {name}\n---\n{{% if stdin %}}\n{{{{ stdin }}}}\n\n{{% endif %}}\n"
                    ))?;
                }
                stub
            } else if matches.len() == 1 {
                matches.into_iter().next().unwrap()
            } else {
                for (i, p) in matches.iter().enumerate() {
                    println!("{i}. {}", p.display());
                }
                print!("Choose file to edit (0–{}): ", matches.len() - 1);
                io::stdout().flush()?;
                let mut line = String::new();
                io::stdin().lock().read_line(&mut line)?;
                let idx: usize = line.trim().parse()
                    .map_err(|_| anyhow::anyhow!("Invalid selection"))?;
                anyhow::ensure!(idx < matches.len(), "Index out of range");
                matches.into_iter().nth(idx).unwrap()
            };

            open_in_editor(&file_to_edit, config)?;
        }
    }
    Ok(())
}

fn handle_memory(action: MemoryAction, project_root: Option<&Path>) -> anyhow::Result<()> {
    match action {
        MemoryAction::Add { text, file, local, private } => {
            let loc = if private { MemoryLocation::LocalPrivate }
                      else if local { MemoryLocation::Local }
                      else { MemoryLocation::Global };

            let content = if let Some(path) = file {
                std::fs::read_to_string(&path)
                    .with_context(|| format!("Cannot read {}", path.display()))?
            } else if let Some(t) = text {
                t
            } else {
                let mut s = String::new();
                io::stdin().read_to_string(&mut s)?;
                s.trim().to_string()
            };

            memory::append_to_memory(loc, &content, project_root)?;
            eprintln!("Memory updated.");
        }
        MemoryAction::Show { global, local, private } => {
            let loc = if global { Some(MemoryLocation::Global) }
                      else if private { Some(MemoryLocation::LocalPrivate) }
                      else if local { Some(MemoryLocation::Local) }
                      else { None };
            let s = memory::show_memory(loc, project_root)?;
            print!("{s}");
        }
        MemoryAction::Clear { global, local, private, yes } => {
            let locs: Vec<MemoryLocation> = [
                if global  { Some(MemoryLocation::Global) }        else { None },
                if local   { Some(MemoryLocation::Local) }         else { None },
                if private { Some(MemoryLocation::LocalPrivate) }  else { None },
            ].into_iter().flatten().collect();

            anyhow::ensure!(
                !locs.is_empty(),
                "Specify at least one of --global, --local, --private"
            );

            if !yes {
                if !io::stdin().is_terminal() {
                    anyhow::bail!("Non-interactive mode: use --yes to confirm");
                }
                print!("Clear memory? [y/N] ");
                io::stdout().flush()?;
                let mut answer = String::new();
                io::stdin().read_line(&mut answer)?;
                if answer.trim().to_lowercase() != "y" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            for loc in locs {
                memory::clear_memory(loc, project_root)?;
            }
            eprintln!("Memory cleared.");
        }
    }
    Ok(())
}

fn handle_context(action: CtxAction, session: &Session, verbose: bool) -> anyhow::Result<()> {
    match action {
        CtxAction::Add { paths, text, cmd } => {
            let cwd = std::env::current_dir()?;
            let mut items: Vec<PoolItem> = Vec::new();

            for path in paths {
                let s = path.to_string_lossy();
                if s.contains('*') || s.contains('?') || s.contains('{') {
                    items.push(PoolItem::Glob { pattern: s.into_owned(), base: cwd.clone() });
                } else {
                    let abs = if path.is_absolute() { path } else { cwd.join(path) };
                    if abs.is_dir() {
                        items.push(PoolItem::Dir { path: abs });
                    } else {
                        items.push(PoolItem::File { path: abs });
                    }
                }
            }

            if let Some(t) = text {
                let content = if t.is_empty() {
                    let mut s = String::new();
                    io::stdin().read_to_string(&mut s)?;
                    s
                } else {
                    t
                };
                items.push(PoolItem::Text { content });
            }

            if let Some(c) = cmd {
                items.push(PoolItem::Command { cmd: c });
            }

            pool::add_items(&session.dir, items)?;
            if verbose { eprintln!("[context] pool updated"); }
        }
        CtxAction::List => {
            let items = pool::load_pool(&session.dir)?;
            if items.is_empty() {
                println!("Pool is empty.");
            } else {
                for (i, item) in items.iter().enumerate() {
                    println!("{i}. {}", pool_item_label(item));
                }
            }
        }
        CtxAction::Remove => {
            let items = pool::load_pool(&session.dir)?;
            if items.is_empty() {
                println!("Pool is empty.");
                return Ok(());
            }
            for (i, item) in items.iter().enumerate() {
                println!("{i}. {}", pool_item_label(item));
            }
            print!("Enter index to remove: ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            let idx: usize = line.trim().parse()
                .map_err(|_| anyhow::anyhow!("Invalid index"))?;
            pool::remove_item(&session.dir, idx)?;
            eprintln!("Item {idx} removed.");
        }
        CtxAction::Clear => {
            pool::clear_pool(&session.dir)?;
            eprintln!("Pool cleared.");
        }
    }
    Ok(())
}

fn print_dry_run(messages: &[ChatMessage], provider: &dyn Provider, stream: bool) {
    eprint!("{}", provider.dry_run_output(messages, stream));
}

fn pool_item_label(item: &PoolItem) -> String {
    match item {
        PoolItem::Text    { content } => format!("text: {}", content.lines().next().unwrap_or("(empty)")),
        PoolItem::File    { path }    => format!("file: {}", path.display()),
        PoolItem::Glob    { pattern, base } => format!("glob: {pattern} in {}", base.display()),
        PoolItem::Dir     { path }    => format!("dir: {}", path.display()),
        PoolItem::Command { cmd }     => format!("cmd: {cmd}"),
    }
}

fn find_command_files_by_name(dir: &Path, name: &str) -> Vec<PathBuf> {
    if !dir.is_dir() { return vec![]; }
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![]; };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension()?.to_str() != Some("md") { return None; }
            let stem = path.file_stem()?.to_str()?;
            if commands::parse_filename(stem) == name { Some(path) } else { None }
        })
        .collect()
}

fn open_in_editor(path: &Path, config: &Config) -> anyhow::Result<()> {
    let editor_str = config::resolve_editor(&config.default);
    let parts = shell_words::split(&editor_str)
        .with_context(|| format!("Invalid editor command: {editor_str}"))?;
    let (bin, args) = parts.split_first()
        .ok_or_else(|| anyhow::anyhow!("Editor command is empty"))?;
    let status = std::process::Command::new(bin)
        .args(args)
        .arg(path)
        .status()
        .with_context(|| format!("Failed to launch editor: {bin}"))?;
    anyhow::ensure!(status.success(), "Editor exited with non-zero status: {status}");
    Ok(())
}

fn combine_prefix_and_prompt(prefix: &str, prompt: &str) -> String {
    if prefix.is_empty() {
        prompt.to_string()
    } else {
        format!("{prefix}\n\n{prompt}")
    }
}

fn read_from_tty(prompt_str: &str) -> anyhow::Result<String> {
    eprint!("{prompt_str}");
    let _ = io::stderr().flush();
    let tty = std::fs::File::open("/dev/tty")
        .context("Cannot open /dev/tty for interactive input")?;
    let mut line = String::new();
    BufReader::new(tty).read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn read_file_args(paths: &[PathBuf]) -> anyhow::Result<Vec<FileArg>> {
    paths.iter().map(|p| {
        let content = std::fs::read_to_string(p)
            .with_context(|| format!("Cannot read {}", p.display()))?;
        Ok(FileArg { path: p.to_string_lossy().into_owned(), content })
    }).collect()
}

fn read_stdin_if_piped() -> anyhow::Result<Option<String>> {
    if io::stdin().is_terminal() {
        return Ok(None);
    }
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    let s = s.trim().to_string();
    Ok(if s.is_empty() { None } else { Some(s) })
}

fn build_history(session: &Session, no_log: bool, limit: Option<usize>) -> anyhow::Result<Vec<ChatMessage>> {
    if no_log { return Ok(vec![]); }
    let all = session.load_exchanges()?;
    let exchanges = match limit {
        Some(n) => {
            let skip = all.len().saturating_sub(n);
            &all[skip..]
        }
        None => &all[..],
    };
    let messages = exchanges.iter().filter_map(|ex| {
        match ex.role.as_str() {
            "user"      => Some(ChatMessage::user(ex.content.clone())),
            "assistant" => Some(ChatMessage::assistant(ex.content.clone())),
            _           => None,
        }
    }).collect();
    Ok(messages)
}

fn run_spinner(progress: bool, stream: bool, json: bool, verbose: bool) -> Option<impl FnOnce()> {
    if !progress || stream || json || verbose || !io::stderr().is_terminal() {
        return None;
    }
    let running = Arc::new(AtomicBool::new(true));
    let r       = Arc::clone(&running);
    let handle  = std::thread::spawn(move || {
        let start = Instant::now();
        let mut animation_tick = 0usize;
        const MAX_POS: usize = 7;
        const CYCLE:   usize = MAX_POS * 2;
        while r.load(Ordering::Relaxed) {
            let phase = animation_tick % CYCLE;
            let pos   = if phase <= MAX_POS { phase } else { CYCLE - phase };
            let bar   = format!("{}{}{}", " ".repeat(pos), "==>", " ".repeat(MAX_POS - pos));
            eprint!("\r[{bar}] {}s ", start.elapsed().as_secs());
            let _ = io::stderr().flush();
            animation_tick += 1;
            std::thread::sleep(Duration::from_millis(80));
        }
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
    });
    Some(move || { running.store(false, Ordering::Relaxed); handle.join().ok(); })
}

fn make_chunk_callback(stream: bool, json: bool) -> Option<Box<dyn Fn(&str)>> {
    if stream && !json {
        Some(Box::new(|chunk: &str| {
            print!("{chunk}");
            let _ = io::stdout().flush();
        }))
    } else {
        None
    }
}

fn default_model_for(provider_type: &str) -> String {
    defaults::find(provider_type)
        .map(|d| d.default_model.clone())
        .unwrap_or_else(|| defaults::all()[0].default_model.clone())
}

fn save_profile(
    config:      &Config,
    name:        &str,
    entry:       ProfileConfig,
    set_default: bool,
) -> anyhow::Result<bool> {
    let mut updated = config.clone();
    let is_new = !updated.profiles.contains_key(name);
    updated.profiles.insert(name.to_string(), entry);
    if set_default || updated.profiles.len() == 1 {
        updated.default.profile = name.to_string();
    }
    updated.save()?;
    Ok(is_new)
}

fn format_duration(ms: u64) -> String {
    output::format_duration(ms)
}

fn handle_session(
    action:   SessionAction,
    _config:  &Config,
    data_dir: &Path,
) -> anyhow::Result<()> {
    match action {
        SessionAction::New => {
            let session = Session::create(data_dir)?;
            println!("export ZURO_SESSION={}", session.id);
            eprintln!("# New session: {}", &session.id[..SESSION_ID_PREFIX_LEN]);
            eprintln!("# Run: eval $(zuro session new)  — or add shell integration (see README)");
        }
        SessionAction::Fork { at } => {
            let current_id = session::resolve(None, data_dir)?;
            let current    = Session::open(data_dir, &current_id)?;
            let forked = match at {
                Some(ref exchange_id) => current.fork_at(data_dir, exchange_id)?,
                None                  => current.fork(data_dir)?,
            };
            println!("export ZURO_SESSION={}", forked.id);
            eprintln!("# Forked {} → {}", &current_id[..SESSION_ID_PREFIX_LEN], &forked.id[..SESSION_ID_PREFIX_LEN]);
            eprintln!("# Run: eval $(zuro session fork)  — or add shell integration (see README)");
        }
        SessionAction::Use { id } => {
            Session::open(data_dir, &id)
                .with_context(|| format!("Session '{id}' not found"))?;
            println!("export ZURO_SESSION={id}");
            eprintln!("# Session: {}", &id[..SESSION_ID_PREFIX_LEN.min(id.len())]);
            eprintln!("# Run: eval $(zuro session use <id>)  — or add shell integration (see README)");
        }
        SessionAction::SetGlobal { id } => {
            let id = match id {
                Some(id) => {
                    Session::open(data_dir, &id)
                        .with_context(|| format!("Session '{id}' not found"))?;
                    id
                }
                None => session::resolve(None, data_dir)?,
            };
            set_active(data_dir, &id)?;
            println!("Global session set to: {id}");
        }
        SessionAction::List => {
            let items = session::list(data_dir)?;
            if items.is_empty() {
                println!("No sessions.");
                return Ok(());
            }
            println!("  {:<36} {:<17} {:<16} {:<16} {:>6} {:>6}  {}", "ID", "Fork", "Created", "Updated", "In", "Out", "Duration");
            println!("{}", "─".repeat(108));
            for item in &items {
                let m        = if item.is_active { "*" } else { " " };
                let created  = item.created_at.as_deref().unwrap_or("-");
                let fork_col = match (&item.forked_from, &item.forked_at_exchange) {
                    (Some(s), Some(e)) => format!("{s}@{e}"),
                    (Some(s), None)    => s.clone(),
                    _                  => "-".to_string(),
                };
                let dur = format_duration(item.duration_ms);
                println!("{m} {} {:<17} {:<16} {:<16} {:>6} {:>6}  {}", item.id, fork_col, created, item.updated_at, item.tokens_in, item.tokens_out, dur);
            }
        }
        SessionAction::Show { id, format } => {
            let id        = resolve_session_arg(id, data_dir)?;
            let session      = Session::open(data_dir, &id)?;
            let exchanges = session.load_exchanges()?;
            print_session_show(&exchanges, &format);
        }
        SessionAction::Stats { id } => {
            let id    = resolve_session_arg(id, data_dir)?;
            let stats = session::stats(data_dir, &id)?;
            print_session_stats(&stats);
        }
        SessionAction::Delete { id } => {
            let full_id = if id.len() == 36 {
                id.clone()
            } else {
                resolve_prefix(data_dir, &id)?
            };
            session::delete(data_dir, &full_id)?;
            println!("Deleted session: {}", &full_id[..SESSION_ID_PREFIX_LEN.min(full_id.len())]);
        }
        SessionAction::Clear { yes } => {
            if !yes {
                if !io::stdin().is_terminal() {
                    anyhow::bail!("Non-interactive mode: use --yes to confirm");
                }
                print!("Are you sure you want to delete all sessions? [y/N] ");
                io::stdout().flush()?;
                let mut answer = String::new();
                io::stdin().read_line(&mut answer)?;
                if answer.trim().to_lowercase() != "y" {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            let n = session::clear_all(data_dir)?;
            println!("Deleted {n} session{}.", if n == 1 { "" } else { "s" });
        }
    }
    Ok(())
}

fn resolve_session_arg(arg: Option<String>, data_dir: &Path) -> anyhow::Result<String> {
    match arg {
        Some(id) => Ok(id),
        None     => session::resolve(None, data_dir),
    }
}

fn handle_profile(action: ProfileAction, config: &Config) -> anyhow::Result<()> {
    match action {
        ProfileAction::List => {
            let mut names: Vec<&String> = config.profiles.keys().collect();
            names.sort();
            let col_name = names.iter().map(|n| n.chars().count() + 2).max().unwrap_or(0);
            let col_type = names.iter()
                .map(|n| config.profiles[*n].provider_type.chars().count())
                .max().unwrap_or(0);
            for name in names {
                let marker = if name == &config.default.profile { "*" } else { " " };
                let cfg = &config.profiles[name];
                let tag  = format!("[{name}]");
                println!("{marker} {tag:<col_name$} {:<col_type$} {}", cfg.provider_type, cfg.model);
            }
        }
        ProfileAction::Show { name } => {
            let name = name.as_deref().unwrap_or(&config.default.profile);
            let cfg  = config.profiles.get(name)
                .with_context(|| format!("Profile '{name}' not found"))?;
            println!("name:     {name}");
            println!("type:     {}", cfg.provider_type);
            println!("model:    {}", cfg.model);
            println!("base_url: {}", cfg.base_url.as_deref().unwrap_or("(default)"));
            if let Some(k) = &cfg.api_key { println!("api_key:  {}", mask_key(k)); }
        }
        ProfileAction::Use { name } => {
            anyhow::ensure!(config.profiles.contains_key(&name), "Profile '{name}' not in config");
            let mut updated = config.clone();
            updated.default.profile = name.clone();
            updated.save()?;
            println!("Default profile set to: {name}");
        }
        ProfileAction::Add { name, r#type, key, model, url, set_default } => {
            let (name, entry, set_default) = if name.is_none()
                && key.is_none() && model.is_none() && url.is_none() && !set_default
            {
                if !io::stdin().is_terminal() {
                    anyhow::bail!("Interactive mode requires a TTY. Use flags: zuro profile add <name> --key ...");
                }
                run_profile_wizard()?
            } else {
                let name = name.ok_or_else(|| anyhow::anyhow!(
                    "Profile name is required. Usage: zuro profile add <name> [--key ...] [--model ...] [--url ...]"
                ))?;
                let existing = config.profiles.get(&name).cloned();
                let resolved_type = r#type.clone()
                    .or_else(|| existing.as_ref().map(|e| e.provider_type.clone()))
                    .unwrap_or_else(|| "openai".to_string());
                let base = existing.unwrap_or_else(|| ProfileConfig {
                    provider_type: resolved_type.clone(),
                    api_key:       None,
                    base_url:      None,
                    model:         default_model_for(&resolved_type),
                });
                let entry = ProfileConfig {
                    provider_type: r#type.unwrap_or(base.provider_type),
                    api_key:       key.or(base.api_key),
                    base_url:      url.or(base.base_url),
                    model:         model.unwrap_or(base.model),
                };
                (name, entry, set_default)
            };

            let is_new = save_profile(config, &name, entry, set_default)?;
            let action = if is_new { "Added" } else { "Updated" };
            println!("{action} profile: {name}");

            let updated = Config::load()?;
            if updated.default.profile == name {
                println!("Set as default.");
            }
        }
    }
    Ok(())
}

fn run_profile_wizard() -> anyhow::Result<(String, ProfileConfig, bool)> {
    println!("\n=== Add Profile ===\n");

    let name  = config::ask("Profile name", None)?;
    let ptype = config::ask("Type (openai/openai-responses/anthropic)", Some("openai"))?;

    let d = defaults::find(&ptype)
        .or_else(|| defaults::all().first())
        .expect("at least one provider in providers.toml");

    let url         = config::ask("API base URL", Some(&d.base_url))?;
    let key         = config::ask_opt("API key")?;
    let model       = config::ask("Model", Some(&d.default_model))?;
    let set_default = config::ask("Set as default?", Some("N"))?.to_lowercase() == "y";

    let entry = ProfileConfig {
        provider_type: ptype,
        api_key:       key,
        base_url:      Some(url),
        model,
    };

    Ok((name, entry, set_default))
}

fn handle_models(profile: Option<&str>, config: &Config) -> anyhow::Result<()> {
    let (_, cfg) = config.active_profile(profile)?;
    let prov     = make_provider(cfg, Duration::from_secs(TIMEOUT_MODELS_SECS))?;
    let models   = match prov.list_models() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Warning: list_models failed ({e}); falling back to built-in list");
            defaults::find(&cfg.provider_type)
                .map(|d| d.models.clone())
                .unwrap_or_default()
        }
    };
    if models.is_empty() {
        eprintln!("No models available.");
    }
    for m in models { println!("{m}"); }
    Ok(())
}

fn handle_info(config: &Config, data_dir: &Path) -> anyhow::Result<()> {
    use crate::constants::{ENV_SESSION, SESSIONS_SUBDIR};

    let cfg_path   = config::config_path();
    let cfg_exists = if cfg_path.exists() { "[exists]" } else { "[missing]" };
    println!("Config");
    println!("  file:     {}  {}", cfg_path.display(), cfg_exists);
    println!("  profiles: {}  (default: {})", config.profiles.len(), config.default.profile);

    let sessions_dir  = data_dir.join(SESSIONS_SUBDIR);
    let session_count = std::fs::read_dir(&sessions_dir)
        .map(|d| d.filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .count())
        .unwrap_or(0);
    println!("\nData");
    println!("  dir:      {}", data_dir.display());
    println!("  sessions: {}  ({} sessions)", sessions_dir.display(), session_count);

    let (active_id, source) = if let Ok(id) = std::env::var(ENV_SESSION) {
        if !id.is_empty() { (id, "from $ZURO_SESSION") } else { (String::new(), "") }
    } else {
        (String::new(), "")
    };
    let (active_id, source) = if active_id.is_empty() {
        match session::get_active(data_dir) {
            Some(id) => (id, "from active_session file"),
            None     => ("none".to_string(), ""),
        }
    } else {
        (active_id, source)
    };
    println!("\nSession");
    if source.is_empty() {
        println!("  active:   {active_id}");
    } else {
        println!("  active:   {active_id}  ({source})");
    }

    let shell     = shell_init::detect_shell();
    let shell_cfg = shell_init::config_path(&shell);
    let installed = if shell_cfg.exists() {
        if shell_init::is_installed(&shell_cfg) { "yes" } else { "no" }
    } else {
        "no config file"
    };
    println!("\nShell");
    println!("  shell:     {}", shell_init::shell_name(&shell));
    println!("  config:    {}", shell_cfg.display());
    println!("  installed: {installed}");

    Ok(())
}

fn handle_shell(action: ShellAction) -> anyhow::Result<()> {
    match action {
        ShellAction::Init { install, shell } => {
            let sh = match shell.as_deref() {
                Some(s) => shell_init::parse_shell(s)?,
                None    => shell_init::detect_shell(),
            };
            if install {
                shell_init::install(&sh)?;
            } else {
                shell_init::print_snippet(&sh);
            }
        }
    }
    Ok(())
}
