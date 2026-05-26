# Changelog
All notable changes to this project will be documented in this file.

## [unreleased](https://github.com/ai-mode/zuro/compare/v0.2.0...HEAD) - XXXX-XX-XX

### New Features
- Replace per-session context pool with a three-level hierarchy: global (`~/.zuro/pool.json`), project (`.zuro/pool.json`), and local (`.zuro/pool.local.json`); pool files live in the project tree, sessions store conversation history only
- Add `zuro context add --project` and `--global` flags to target a specific pool level; default writes to local pool (or global when outside a project)
- Add source tags `[G]`/`[P]`/`[L]` to `zuro context list` output
- Add `zuro context clear --project`/`--local`/`--global` flags to clear specific levels
- Add `.zuro/config.toml` project config with `[pool]` section: `use_global` (opt-in global pool) and `local_merge` (`append` or `replace`)

---

## [v0.2.0](https://github.com/ai-mode/zuro/compare/v0.1.0...v0.2.0) - 2026-05-25

### New Features
- Add `zuro repl` — interactive multi-turn chat mode with multiline input, Ctrl+Enter to send, profile and model displayed on startup; each invocation creates a new session automatically
- Add kitty keyboard protocol support via reedline: Ctrl+Enter is a distinct key in iTerm2, kitty, WezTerm, Ghostty and other modern terminals; Ctrl+J works as a universal fallback
- Add `--history` flag to `zuro repl` for limiting context depth per invocation
- Add `repl_submit_key` and `repl_history_limit` config options under `[default]`
- Add stdin-as-context: when both stdin and a prompt argument are given, stdin is injected as a context block; invoking with no input prints help
- Add `--no-session` flag for stateless one-off requests without session history or pool
- Switch context blocks to XML format: pool items, memory, and files are now wrapped in typed tags (`<context type="file" path="…">`, `<memory scope="global">`, etc.) for better model comprehension

### Fixes
- Include `providers.toml` in crate tarball

---

## [v0.1.0](https://github.com/ai-mode/zuro/releases/tag/v0.1.0) - 2026-05-24

### New Features
- Add streaming LLM conversations via `zuro run` with session persistence
- Add named commands: reusable Jinja2 prompt templates stored as Markdown files with YAML frontmatter
- Add `--input` flag (repeatable) for positional mapping of CLI values to command-declared inputs
- Add built-in commands: `fix`, `explain`, `review`, `document`
- Add command resolution order: local (`.zuro/commands/`) → global (`~/.zuro/commands/`) → built-in
- Add memory files: persistent Markdown instructions injected into every request, with global, project, and private scopes
- Add context pool: per-session persistent context items (files, dirs, globs, text notes, shell commands)
- Add session management: create, list, delete, fork, switch active session
- Add profile management: named provider/model configurations
- Add provider support: Anthropic (Claude), OpenAI (chat completions), OpenAI Responses API
- Add shell integration via `zuro shell init` for bash, zsh, and fish
- Add `--dry-run` flag: print assembled request payload without sending
- Add system message support via `.zuro/system.md` and `~/.zuro/system.md`
- Add interactive setup wizard via `zuro setup`
