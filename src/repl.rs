use std::path::Path;
use std::time::Duration;

use std::borrow::Cow;

use reedline::{
    default_emacs_keybindings, EditCommand, Emacs, KeyCode, KeyModifiers,
    Prompt, PromptEditMode, PromptHistorySearch, Reedline, ReedlineEvent, Signal,
};

use uuid::Uuid;

struct ReplPrompt;

impl Prompt for ReplPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str>             { Cow::Borrowed("") }
    fn render_prompt_right(&self) -> Cow<'_, str>            { Cow::Borrowed("") }
    fn render_prompt_indicator(&self, _: PromptEditMode) -> Cow<'_, str> { Cow::Borrowed("> ") }
    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str>          { Cow::Borrowed("  ") }
    fn render_prompt_history_search_indicator(&self, _: PromptHistorySearch) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }
}

use crate::cli::Cli;
use crate::config::{Config, SubmitKey, resolve_submit_key};
use crate::constants::TIMEOUT_CHAT_SECS;
use crate::provider::{make_provider, ChatMessage, Provider};
use crate::providers;
use crate::session::{self, Session};
use crate::{pool, memory, config};

pub fn run_repl(
    cli:          &Cli,
    config:       &Config,
    data_dir:     &Path,
    project_root: Option<&Path>,
    history_flag: Option<usize>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !(cli.no_session && cli.session.is_some()),
        "--no-session and --session are mutually exclusive"
    );

    let (profile_name, profile_cfg) = config.active_profile(cli.profile.as_deref())?;
    let mut actual_cfg = profile_cfg.clone();
    if let Some(m) = &cli.model { actual_cfg.model = m.clone(); }

    let (session_opt, resolved) = if cli.no_session {
        (None, vec![])
    } else if let Some(ref session_id) = cli.session {
        let session    = Session::open(data_dir, session_id)?;
        let pool_items = pool::load_pool(&session.dir)?;
        let shell      = config::resolve_shell(&config.default);
        let expanded   = pool::expand_pool(&pool_items, &shell, cli.verbose)?;
        (Some(session), expanded)
    } else {
        let session = Session::create(data_dir)?;
        if cli.verbose { eprintln!("[repl] new session: {}", session.id); }
        (Some(session), vec![])
    };

    let memory        = memory::load_memory(project_root);
    let system_msg    = providers::assemble_system_message(project_root, cli.verbose);
    let provider      = make_provider(&actual_cfg, Duration::from_secs(TIMEOUT_CHAT_SECS))?;
    let show_stats    = cli.stats || config.default.show_stats;
    let history_limit = history_flag.or(config.default.repl_history_limit);
    let submit_key    = resolve_submit_key(&config.default);

    let mut editor = build_editor(submit_key);
    let prompt = ReplPrompt;

    let submit_hint = match submit_key {
        SubmitKey::CtrlEnter => "Ctrl+Enter / Ctrl+J to send  |  Enter for newline",
        SubmitKey::Enter     => "Enter to send",
    };
    eprintln!("  {} · {}  |  {}  |  Ctrl+C to quit", profile_name, actual_cfg.model, submit_hint);

    loop {
        match editor.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                send_repl_turn(
                    input,
                    session_opt.as_ref(),
                    &*provider,
                    cli,
                    &memory,
                    &resolved,
                    &system_msg,
                    &profile_name,
                    show_stats,
                    history_limit,
                )?;
            }
            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => break,
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

fn build_editor(submit_key: SubmitKey) -> Reedline {
    let mut keybindings = default_emacs_keybindings();

    if let SubmitKey::CtrlEnter = submit_key {
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Enter,
            ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
        );
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Enter,
            ReedlineEvent::Submit,
        );
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('j'),
            ReedlineEvent::Submit,
        );
    }

    Reedline::create()
        .use_kitty_keyboard_enhancement(true)
        .with_edit_mode(Box::new(Emacs::new(keybindings)))
}

fn send_repl_turn(
    prompt:        &str,
    session_opt:   Option<&Session>,
    provider:      &dyn Provider,
    cli:           &Cli,
    memory:        &crate::memory::MemoryContent,
    resolved:      &[crate::pool::ResolvedItem],
    system_msg:    &str,
    profile_name:  &str,
    show_stats:    bool,
    history_limit: Option<usize>,
) -> anyhow::Result<()> {
    let user_prefix  = providers::assemble_user_prefix(memory, resolved, &[], None, cli.verbose);
    let user_content = crate::combine_prefix_and_prompt(&user_prefix, prompt);
    let exchange_id  = Uuid::new_v4().to_string();

    let mut messages = Vec::new();
    if !system_msg.is_empty() {
        messages.push(ChatMessage::system(system_msg.to_string()));
    }
    if let Some(session) = session_opt {
        messages.extend(crate::build_history(session, cli.no_log, history_limit)?);
    }
    messages.push(ChatMessage::user(user_content));

    if cli.dry_run {
        crate::print_dry_run(&messages, provider, cli.stream);
        return Ok(());
    }

    if !cli.no_log {
        if let Some(session) = session_opt {
            session.append(&session::Exchange::now(
                "user",
                prompt.to_string(),
                session::ExchangeMeta {
                    exchange_id: Some(exchange_id.clone()),
                    ..Default::default()
                },
            ))?;
        }
    }

    crate::execute_chat_and_log(
        provider,
        &messages,
        session_opt,
        exchange_id,
        profile_name.to_string(),
        cli,
        show_stats,
    )
}
