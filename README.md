# zuro

Minimalist CLI for LLM conversations directly from the terminal.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [First run](#first-run)
- [Usage](#usage)
  - [Flags](#flags)
- [REPL](#repl)
- [Sessions](#sessions)
  - [Shell integration](#shell-integration)
  - [Global session](#global-session)
  - [Session commands](#session-commands)
  - [Viewing history](#viewing-history)
  - [Forking](#forking)
  - [Cleanup](#cleanup)
- [Named commands](#named-commands)
  - [Running commands](#running-commands)
  - [Command files](#command-files)
  - [Frontmatter fields](#frontmatter-fields)
  - [Template variables](#template-variables)
  - [Managing commands](#managing-commands)
- [Memory](#memory)
- [Context pool](#context-pool)
- [Profiles](#profiles)
- [Config](#config)
- [Files](#files)
- [Diagnostics](#diagnostics)
- [JSON output](#json-output)
- [Legal Notice](#legal-notice)

## Features

- Single-shot prompts and pipe support
- Interactive REPL (`zuro repl`) with multiline input and Ctrl+Enter to send
- Streaming output (`--stream`)
- Sessions: conversation history persists across invocations
- Named commands: reusable prompt templates with frontmatter-based behaviour
- Context pool: three-level (global / project / local) persistent context attached to every request
- Memory files: persistent global and per-project instructions for the model
- Multiple profiles in one config (OpenAI-compatible APIs and Anthropic)
- JSON output for scripting
- Progress indicator, token usage stats
- `--dry-run` to inspect the full request without sending it

## Installation

**From crates.io:**

```bash
cargo install zuro
```

**From source:**

```bash
git clone <repo> ~/zuro
cd ~/zuro
cargo build --release
cp target/release/zuro /usr/local/bin/zuro
```

Requires Rust 1.75+.

## First run

On first run, a setup wizard is launched:

```
No config found at ~/.config/zuro/config.toml. Running setup wizard.

=== zuro CLI Setup ===

Profile type (openai/anthropic) [openai]:
API base URL [https://api.openai.com/v1]:
API key: sk-...
Default model [gpt-4o-mini]:

Config saved to ~/.config/zuro/config.toml
```

## Usage

```bash
# Simple prompt
zuro "what is a monad?"

# Via pipe
echo "explain this" | zuro
cat file.rs | zuro "find bugs"
git diff | zuro "write a commit message"

# Streaming output
zuro --stream "tell me a story"

# Specific profile or model
zuro --profile anthropic "translate to French: hello"
zuro --model gpt-4o "complex task"

# JSON output
zuro --format json "answer briefly" | jq .answer

# Without logging to session
zuro --no-log "quick question"

# Show token usage after response
zuro --stats "what is a monad?"

# Show progress bar while waiting
zuro --progress "write a long essay"

# Inspect the full request without sending it
zuro --dry-run "hello"
cat file.rs | zuro --dry-run run fix
```

### Flags

| Flag | Description |
|------|-------------|
| `--stream` | Stream tokens as they arrive |
| `--profile <name>` | Use a profile from config |
| `--model <name>` | Override the model |
| `--session <id>` | Use a specific session |
| `--no-log` | Do not record this exchange to the session |
| `--format json` | Output as JSON |
| `--stats` | Print token usage and model info after response |
| `--progress` | Show animated progress bar while waiting for response |
| `--dry-run` | Print the assembled request to stderr without sending |
| `-v, --verbose` | Dump requests/responses to stderr |

## REPL

`zuro repl` opens an interactive multi-turn chat session in the terminal.

```bash
zuro repl                        # new session, Ctrl+Enter to send
zuro repl --session <id>         # resume an existing session
zuro repl --no-session           # stateless, nothing saved
zuro repl --history 10           # limit context to last 10 exchanges
zuro repl --profile anthropic    # use a specific profile
```

**Key bindings (default):**

| Key | Action |
|-----|--------|
| Ctrl+Enter | Send message |
| Enter | Insert newline (multiline input) |
| Ctrl+C / Ctrl+D | Quit |
| ↑ / ↓ | Navigate input history |

> Ctrl+Enter requires a terminal with kitty keyboard protocol support (iTerm2, kitty, WezTerm, Ghostty, Alacritty). In other terminals use **Ctrl+J** as an equivalent.
>
> Set `repl_submit_key = "enter"` in config to send with plain Enter instead.

**Session behaviour:**

Each `zuro repl` invocation automatically creates a new session. Pass `--session <id>` to resume an existing one, or `--no-session` for a stateless run.

**Config options** (under `[default]`):

| Key | Default | Description |
|-----|---------|-------------|
| `repl_submit_key` | `"ctrl+enter"` | Submit key: `ctrl+enter` or `enter` |
| `repl_history_limit` | *(all)* | Max exchanges included in context |

## Sessions

Conversation is automatically saved to the active session. On the next invocation the history is loaded and context is preserved.

Resolution order: `--session flag` → `$ZURO_SESSION` → global active_session file → create new.

### Shell integration

`session new`, `session use`, and `session fork` set the session for the **current terminal only**
by printing an `export` command. A shell wrapper makes this work transparently without any `eval`
boilerplate.

**Option 1 — automatic install** (auto-detects bash / zsh / fish):

```bash
zuro shell init --install
```

Appends the wrapper to the appropriate config file (`~/.zshrc`, `~/.bashrc`, or
`~/.config/fish/functions/zuro.fish`) and prints instructions to reload the shell.

**Option 2 — manual install**:

```bash
zuro shell init              # print snippet + target file path
zuro shell init --shell fish # same, for a specific shell
```

The snippet goes to stdout so you can redirect it directly:

```bash
zuro shell init >> ~/.zshrc
```

After reloading your shell:

```bash
source ~/.zshrc   # or restart the terminal
```

Session commands work without `eval`:

```bash
zuro session new              # creates session, sets $ZURO_SESSION in this terminal only
zuro session use <uuid>       # switches session in this terminal only
zuro session fork             # forks and switches in this terminal only
```

Without shell integration, use `eval` explicitly:

```bash
eval $(zuro session new)
eval $(zuro session use <uuid>)
```

### Global session

To set the session shared by all terminals that have no `$ZURO_SESSION` set:

```bash
zuro session set-global <uuid>   # set specific session as global
zuro session set-global          # promote current $ZURO_SESSION to global
```

The global session is stored in `~/.local/share/zuro/active_session`.

### Session commands

```bash
# List sessions with creation date, fork origin, token counts
zuro session list

# Create a new session (TTY-local)
zuro session new

# Switch to an existing session (TTY-local)
zuro session use <id>
```

`session list` output:

```
   ID                                    Created           Updated           Fork            In     Out   Duration
   ───────────────────────────────────────────────────────────────────────────────────────────────────────────────
*  550e8400-e29b-41d4-a716-446655440000  2026-02-28 10:00  2026-02-28 13:05  -               1234   5678   5.123s
   a3f1c209-e29b-41d4-a716-446655440001  2026-02-28 09:30  2026-02-28 11:30  550e8400         500   1200   2.456s
```

`*` marks the active session.

### Viewing history

```bash
# Show exchanges in the current session (default: text format)
zuro session show
zuro session show <id>

# Output formats
zuro session show --format text     # compact one-liner per exchange (default)
zuro session show --format chat     # human-readable blocks
zuro session show --format table    # side-by-side two-column layout
zuro session show --format json     # JSON array

# Token usage statistics
zuro session stats
zuro session stats <id>
```

### Forking

Fork creates a copy of a session. The fork is TTY-local (sets `$ZURO_SESSION`).

```bash
# Fork the current session from its end
zuro session fork

# Fork up to and including a specific exchange (use 8-char ID prefix from session show)
zuro session fork --at 550e8400
```

The forked session records which session it was forked from, shown in `session list` under the `Fork` column.

### Cleanup

```bash
# Delete a single session by ID or 8-char prefix
zuro session delete 550e8400

# Delete all sessions (prompts for confirmation)
zuro session clear

# Skip confirmation (for scripts)
zuro session clear --yes
```

## Named commands

Commands are reusable prompt templates stored as Markdown files. They can be piped stdin, accept user input, and include session context.

### Running commands

```bash
# Pipe code into a built-in command
cat src/main.rs | zuro run fix
git diff        | zuro run review
cat lib.rs      | zuro run document

# Pass inputs via --input (positionally mapped to declared inputs, repeatable)
cat src/main.rs | zuro run fix --input "focus on error handling"

# Multiple --input flags map to inputs in declaration order
zuro run commit --input "auth" --input "yes"

# Inputs not covered by --input are prompted interactively in order
cat src/main.rs | zuro run fix
# Focus on (optional): <typed here>

# Include files via --file instead of stdin
zuro run review --file src/main.rs --file src/lib.rs
```

### Command files

Command files are resolved in order: local (`.zuro/commands/`) → global (`~/.zuro/commands/`) → built-in.
The most specific location wins entirely.

Built-in commands: `fix`, `explain`, `document`, `review`.

To add a project-specific command, create `.zuro/commands/<name>.md`:

```markdown
---
description: Validate deployment scripts for our infra
history: small
inputs:
  - name: focus
    prompt: "What to check for? "
---
{% if stdin %}
{{ stdin }}

{% endif %}
Check this deployment script for common mistakes.{% if inputs.focus %} Pay attention to: {{ inputs.focus }}.{% endif %}
```

### Frontmatter fields

Frontmatter is YAML and optional. Omitting a field uses the default value.

| Field | Type | Default | Effect |
|-------|------|---------|--------|
| `description` | string | — | Shown in `zuro commands list` |
| `history` | `full` \| `small` \| `large` \| `none` | `full` | How many session exchanges to include |
| `inputs` | list of input definitions | `[]` | Interactive fields prompted before the command runs |

**`history` values:**
- `full` — include all session history
- `small` — include the last 3 exchanges
- `large` — include the last 20 exchanges
- `none` — send no history

**`inputs` — input definition fields:**

| Field | Type | Default | Effect |
|-------|------|---------|--------|
| `name` | string | — | Variable name, accessed as `{{ inputs.name }}` in the template |
| `prompt` | string | `"name: "` | Text shown to the user at the prompt |
| `required` | bool | `false` | Empty input is an error when `true`; silently skipped when `false` |

Example with multiple inputs:

```markdown
---
description: Generate a commit message
inputs:
  - name: scope
    prompt: "Scope of change: "
    required: true
  - name: breaking
    prompt: "Breaking change? (yes/no): "
---
Write a conventional commit message for the following diff.
Scope: {{ inputs.scope }}{% if inputs.breaking %}
Breaking: {{ inputs.breaking }}{% endif %}

{{ stdin }}
```

`--input` maps to the first input; subsequent inputs are always prompted interactively.

### Template variables

| Variable | Source |
|----------|--------|
| `{{ stdin }}` | Piped stdin |
| `{{ inputs.<name> }}` | Named input value (`null` if skipped) |
| `{{ files }}` | List of `{path, content}` from `--file` flags |
| `{{ memory.global }}` | `~/.zuro/memory.md` |
| `{{ memory.local }}` | `.zuro/memory.md` |
| `{{ memory.local_private }}` | `.zuro/memory.local.md` |
| `{{ model }}` | Active model name |
| `{{ profile }}` | Active profile name |
| `{{ session_id }}` | Active session ID (8-char prefix) |
| `{{ cwd }}` | Current working directory |
| `{{ date }}` | Current date (`YYYY-MM-DD`) |

### Managing commands

```bash
# List all available commands (built-in, global, local)
zuro commands list

# Show the template of a command
zuro commands show fix

# Open a command file in $EDITOR (creates it if it doesn't exist)
zuro commands edit fix              # .zuro/commands/
zuro commands edit fix --global     # ~/.zuro/commands/
```

The editor is resolved from `config.default.editor` → `$EDITOR` → `vi`.

## Memory

Memory files are plain Markdown appended to the context of every request.
They let you give the model persistent instructions without repeating them each time.

| File | Scope |
|------|-------|
| `~/.zuro/memory.md` | Global — applies to all projects |
| `.zuro/memory.md` | Project — committed to VCS, shared with team |
| `.zuro/memory.local.md` | Project private — add to `.gitignore` |

```bash
# Append text to global memory
zuro memory add "Always prefer iterators over explicit loops in Rust"

# Append to project memory
zuro memory add --local "This project targets Rust 1.75+, uses Tokio"

# Append to private project memory
zuro memory add --private "My staging endpoint: http://..."

# Read content from a file
zuro memory add --local --file ARCHITECTURE.md

# Read content from stdin
cat notes.md | zuro memory add --local

# Show memory contents
zuro memory show           # all
zuro memory show --global
zuro memory show --local
zuro memory show --private

# Clear memory (prompts for confirmation)
zuro memory clear --global
zuro memory clear --local --yes
```

## Context pool

The context pool injects persistent items into every request automatically — no need to repeat `--file` flags or re-paste content. It has three levels that are merged on each invocation:

| Level | File | Typical use |
|-------|------|-------------|
| Global | `~/.zuro/pool.json` | Tools and snippets useful everywhere |
| Project | `.zuro/pool.json` | Shared team context — commit to VCS |
| Local | `.zuro/pool.local.json` | Personal additions — add to `.gitignore` |

```bash
# Add to local pool (default when inside a project)
zuro context add src/session.rs src/main.rs
zuro context add src/                        # directory (respects .gitignore and .zuro-ignore)
zuro context add "src/**/*.rs"               # glob
zuro context add --text "The bug is in fork_at"
git diff | zuro context add --text           # text from stdin
zuro context add --cmd "git log --oneline -20"   # shell command, re-run on each request
zuro context add --cmd "cargo check 2>&1"

# Write to a specific level
zuro context add --project src/main.rs      # → .zuro/pool.json
zuro context add --global ~/.zuro/snippets/ # → ~/.zuro/pool.json

# List all active pool items (shows source level)
zuro context list
# 0. [P] file: src/main.rs
# 1. [L] note: The bug is in fork_at
# 2. [G] cmd: git log --oneline -20 (exit 0)

# Remove an item interactively (select by index, correct file is updated automatically)
zuro context remove

# Clear pool items by level
zuro context clear --local
zuro context clear --project
zuro context clear --global
zuro context clear --local --project   # multiple levels at once
```

File contents are read at request time, not when added. Pool items always reflect the current state of files.

### Project pool config

Control merge behaviour in `.zuro/config.toml`:

```toml
[pool]
use_global  = false    # include ~/.zuro/pool.json (default: false)
local_merge = "append" # "append": project + local (default)
                       # "replace": local pool only, project pool is ignored
```

When `use_global` is false (the default), the global pool is excluded unless the current directory has no project root, in which case the global pool is used as fallback.

### Ignore rules

Resolved in order (last match wins):
1. `~/.zuro/.zuro-ignore` — global
2. `<project>/.gitignore`
3. `<project>/.zuro-ignore`

## Profiles

A profile bundles an API endpoint, key, type, and model under a name.

```bash
# List profiles
zuro profile list

# Show profile details
zuro profile show
zuro profile show anthropic

# Add a new profile (interactive wizard)
zuro profile add

# Add a profile with flags
zuro profile add openai-pro --type openai --key sk-... --model gpt-4o

# Update a single field of an existing profile (type is preserved)
zuro profile add openai --model gpt-4o

# Set the default profile
zuro profile use anthropic

# List available models for the active (or named) profile
zuro models
zuro models --profile anthropic
```

### `profile add` flags

| Flag | Description |
|------|-------------|
| `--type` | Provider type: `openai` (default for new profiles), `openai-responses`, or `anthropic` |
| `--key` | API key |
| `--model` | Model name |
| `--url` | API base URL |
| `--set-default` | Make this the default profile |

## Config

`~/.config/zuro/config.toml`:

```toml
[default]
profile    = "openai"
show_stats = false          # always print token usage after each response
editor     = "code --wait"  # used by `zuro commands edit`; falls back to $EDITOR, then vi
shell      = "zsh"          # used for pool command items; falls back to $SHELL, then sh

# REPL settings
repl_submit_key    = "ctrl+enter"  # or "enter"
repl_history_limit = 20            # max exchanges included in context (omit for unlimited)

[profiles.openai]
type     = "openai"
api_key  = "sk-..."
base_url = "https://api.openai.com/v1"
model    = "gpt-4o-mini"

[profiles.anthropic]
type    = "anthropic"
api_key = "sk-ant-..."
model   = "claude-sonnet-4-6"

# Local Ollama
[profiles.local]
type     = "openai"
base_url = "http://localhost:11434/v1"
model    = "llama3.2"
```

The `openai` type works with any OpenAI-compatible API (Ollama, vLLM, LM Studio, etc.).

### Project config

`.zuro/config.toml` (per-project, commit to VCS):

```toml
[pool]
use_global  = false    # include ~/.zuro/pool.json in every request
local_merge = "append" # "append" (default) or "replace"
```

## Files

| Path | Description |
|------|-------------|
| `~/.config/zuro/config.toml` | Global configuration |
| `~/.local/share/zuro/sessions/<uuid>/meta.json` | Session metadata |
| `~/.local/share/zuro/sessions/<uuid>/history.jsonl` | Session exchange history |
| `~/.local/share/zuro/active_session` | Global active session ID |
| `~/.zuro/pool.json` | Global context pool |
| `~/.zuro/memory.md` | Global memory |
| `~/.zuro/commands/` | Global named commands |
| `.zuro/config.toml` | Project config (commit to VCS) |
| `.zuro/pool.json` | Project context pool (commit to VCS) |
| `.zuro/pool.local.json` | Personal context pool (.gitignore this) |
| `.zuro/memory.md` | Project memory (commit to VCS) |
| `.zuro/memory.local.md` | Personal project memory (.gitignore this) |
| `.zuro/commands/` | Project named commands |
| `.zuro/system.md` | Project system instructions |
| `.zuro-ignore` | Ignore rules for context pool file expansion |

## Diagnostics

```bash
zuro info
```

Shows config path, data directory, session count, active session source, and shell integration status:

```
Config
  file:     /home/user/.config/zuro/config.toml  [exists]
  profiles: 2  (default: openai)

Data
  dir:      /home/user/.local/share/zuro
  sessions: /home/user/.local/share/zuro/sessions  (12 sessions)

Session
  active:   550e8400-...  (from $ZURO_SESSION)

Shell
  shell:     zsh
  config:    /home/user/.zshrc
  installed: yes
```

## JSON output

```bash
zuro --format json "capital of France?"
```

```json
{
  "answer": "Paris.",
  "session_id": "...",
  "model": "gpt-4o-mini",
  "usage": { "input_tokens": 12, "output_tokens": 4 }
}
```

Useful in scripts:

```bash
ANSWER=$(zuro --format json --no-log "one-liner: list files" | jq -r .answer)
```

## Legal Notice

This project is an independent open-source initiative and is not affiliated with, endorsed by, or sponsored by Anthropic, PBC, OpenAI, Inc., DeepSeek, Hugging Face, Inc., or Google LLC.

Claude is a trademark of Anthropic, PBC. OpenAI, GPT, and ChatGPT are trademarks or registered trademarks of OpenAI, Inc. DeepSeek is a trademark of DeepSeek. Hugging Face and the Hugging Face logo are trademarks or registered trademarks of Hugging Face, Inc. Google and Gemini are trademarks of Google LLC. All other trademarks mentioned in this documentation are the property of their respective owners.

The use of AI APIs (such as Anthropic's, OpenAI's, Google's, DeepSeek's, and Hugging Face's) is subject to their respective terms of service and usage policies. Users are responsible for ensuring their usage complies with all applicable terms and regulations.
