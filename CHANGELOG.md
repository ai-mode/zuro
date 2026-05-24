# Changelog
All notable changes to this project will be documented in this file.

## [unreleased](https://github.com/ai-mode/zuro/compare/v0.1.0...HEAD) - XXXX-XX-XX

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
