use std::fs;
use std::path::Path;

use anyhow::Context;
use chrono::{DateTime, Utc};

use crate::constants::{DATETIME_DISPLAY_LEN, ENV_SESSION, HISTORY_FILE, MESSAGE_ID_PREFIX_LEN, SESSION_ID_PREFIX_LEN};
use super::{
    ExchangeStats, SessionInfo, SessionStats, Session,
    active_path, get_active, session_dir, sessions_dir,
};

pub fn list(data_dir: &Path) -> anyhow::Result<Vec<SessionInfo>> {
    let dir = sessions_dir(data_dir);
    if !dir.exists() { return Ok(vec![]); }

    let active = std::env::var(ENV_SESSION)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| get_active(data_dir))
        .unwrap_or_default();

    let mut entries: Vec<_> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    entries.sort_by(|a, b| {
        let ta = history_mtime(a);
        let tb = history_mtime(b);
        tb.cmp(&ta)
    });

    let mut result = Vec::new();
    for entry in entries {
        let id = entry.path()
            .file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let updated_at  = format_history_mtime(&entry);
        let session        = Session::open(data_dir, &id)?;
        let exchanges   = session.load_exchanges().unwrap_or_default();
        let (tokens_in, tokens_out, duration_ms) = sum_session_tokens(&exchanges);
        let (created_at, forked_from, forked_at_exchange) = session.load_header()
            .ok().flatten()
            .map(|h| (
                Some(format_session_timestamp(&h.created_at)),
                h.forked_from.map(|s| s[..SESSION_ID_PREFIX_LEN.min(s.len())].to_string()),
                h.forked_at_exchange,
            ))
            .unwrap_or((None, None, None));
        let is_active = id == active;
        result.push(SessionInfo {
            id, is_active, created_at, updated_at,
            forked_from, forked_at_exchange, tokens_in, tokens_out, duration_ms,
        });
    }
    Ok(result)
}

fn sum_session_tokens(exchanges: &[super::Exchange]) -> (u32, u32, u64) {
    let tokens_in   = exchanges.iter().filter_map(|e| e.meta.usage.as_ref()?.input_tokens).sum();
    let tokens_out  = exchanges.iter().filter_map(|e| e.meta.usage.as_ref()?.output_tokens).sum();
    let duration_ms = exchanges.iter().filter_map(|e| e.meta.duration_ms).sum();
    (tokens_in, tokens_out, duration_ms)
}

fn format_session_timestamp(ts: &str) -> String {
    ts[..DATETIME_DISPLAY_LEN.min(ts.len())].replace('T', " ")
}

pub fn stats(data_dir: &Path, id: &str) -> anyhow::Result<SessionStats> {
    let session      = Session::open(data_dir, id)?;
    let exchanges = session.load_exchanges()?;

    let mut result       = Vec::new();
    let mut total_input  = 0u32;
    let mut total_output = 0u32;
    let mut total_dur_ms = 0u64;

    for ex in exchanges.iter().filter(|e| e.role == "assistant") {
        if let Some(u) = &ex.meta.usage {
            let inp = u.input_tokens.unwrap_or(0);
            let out = u.output_tokens.unwrap_or(0);
            total_input  += inp;
            total_output += out;
            if let Some(d) = ex.meta.duration_ms { total_dur_ms += d; }
            result.push(ExchangeStats {
                exchange_id:   ex.meta.exchange_id.as_deref()
                    .map(|s| s[..MESSAGE_ID_PREFIX_LEN.min(s.len())].to_string()),
                ts:            ex.ts.clone(),
                model:         ex.meta.model.clone().unwrap_or_else(|| "?".into()),
                input_tokens:  inp,
                output_tokens: out,
                duration_ms:   ex.meta.duration_ms,
            });
        }
    }
    Ok(SessionStats { exchanges: result, total_input, total_output, total_dur_ms })
}

pub fn resolve_prefix(data_dir: &Path, prefix: &str) -> anyhow::Result<String> {
    let dir = sessions_dir(data_dir);
    if !dir.exists() { anyhow::bail!("No sessions found"); }
    let matches: Vec<String> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.path().file_name()?.to_str()?.to_string();
            if e.path().is_dir() && name.starts_with(prefix) { Some(name) } else { None }
        })
        .collect();
    match matches.len() {
        0 => anyhow::bail!("No session matching '{prefix}'"),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => anyhow::bail!("Ambiguous prefix '{prefix}' matches {} sessions", matches.len()),
    }
}

pub fn delete(data_dir: &Path, id: &str) -> anyhow::Result<()> {
    let dir = session_dir(data_dir, id);
    anyhow::ensure!(dir.is_dir(), "Session '{id}' not found");
    fs::remove_dir_all(&dir)
        .with_context(|| format!("Failed to delete session '{id}'"))?;
    if get_active(data_dir).as_deref() == Some(id) {
        let _ = fs::remove_file(active_path(data_dir));
    }
    Ok(())
}

pub fn clear_all(data_dir: &Path) -> anyhow::Result<usize> {
    let dir = sessions_dir(data_dir);
    if !dir.exists() { return Ok(0); }
    let entries: Vec<_> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    let count = entries.len();
    for entry in entries {
        fs::remove_dir_all(entry.path())?;
    }
    let _ = fs::remove_file(active_path(data_dir));
    Ok(count)
}

fn history_mtime(entry: &fs::DirEntry) -> Option<std::time::SystemTime> {
    let result = entry.path().join(HISTORY_FILE).metadata()
        .and_then(|m| m.modified())
        .or_else(|_| entry.metadata().and_then(|m| m.modified()));
    if result.is_err() {
        eprintln!("[session] cannot read mtime for '{}'", entry.path().display());
    }
    result.ok()
}

fn format_history_mtime(entry: &fs::DirEntry) -> String {
    history_mtime(entry)
        .map(|t| {
            let dt: DateTime<Utc> = t.into();
            dt.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_else(|| "?".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Exchange, ExchangeMeta, Session, TokenUsage};
    use tempfile::TempDir;

    fn make_data_dir() -> TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("sessions")).unwrap();
        tmp
    }

    fn make_exchange(role: &str, content: &str) -> Exchange {
        Exchange::now(role, content.to_string(), Default::default())
    }

    #[test]
    fn list_empty_data_dir() {
        let tmp = make_data_dir();
        let items = list(tmp.path()).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn list_returns_all_sessions() {
        let tmp = make_data_dir();
        Session::create(tmp.path()).unwrap();
        Session::create(tmp.path()).unwrap();
        Session::create(tmp.path()).unwrap();
        let items = list(tmp.path()).unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn list_marks_active_session() {
        let tmp = make_data_dir();
        let s1 = Session::create(tmp.path()).unwrap();
        Session::create(tmp.path()).unwrap();
        crate::session::set_active(tmp.path(), &s1.id).unwrap();
        let items = list(tmp.path()).unwrap();
        let active_items: Vec<_> = items.iter().filter(|i| i.is_active).collect();
        assert_eq!(active_items.len(), 1);
        assert_eq!(active_items[0].id, s1.id);
    }

    #[test]
    fn resolve_prefix_finds_unique_match() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        let prefix = &session.id[..8];
        let found = resolve_prefix(tmp.path(), prefix).unwrap();
        assert_eq!(found, session.id);
    }

    #[test]
    fn resolve_prefix_errors_on_no_match() {
        let tmp = make_data_dir();
        Session::create(tmp.path()).unwrap();
        let result = resolve_prefix(tmp.path(), "xxxxxxxx");
        assert!(result.is_err());
    }

    #[test]
    fn delete_removes_session_dir() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        assert!(session.dir.is_dir());
        delete(tmp.path(), &session.id).unwrap();
        assert!(!session.dir.exists());
    }

    #[test]
    fn delete_nonexistent_errors() {
        let tmp = make_data_dir();
        let result = delete(tmp.path(), "00000000-0000-0000-0000-000000000000");
        assert!(result.is_err());
    }

    #[test]
    fn clear_all_removes_all_sessions() {
        let tmp = make_data_dir();
        Session::create(tmp.path()).unwrap();
        Session::create(tmp.path()).unwrap();
        Session::create(tmp.path()).unwrap();
        let n = clear_all(tmp.path()).unwrap();
        assert_eq!(n, 3);
        let items = list(tmp.path()).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn stats_returns_token_totals() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        session.append(&make_exchange("user", "q1")).unwrap();
        session.append(&Exchange::now("assistant", "a1".to_string(), ExchangeMeta {
            usage: Some(TokenUsage { input_tokens: Some(10), output_tokens: Some(5) }),
            duration_ms: Some(200),
            model: Some("gpt-4.1-mini".into()),
            ..Default::default()
        })).unwrap();
        session.append(&make_exchange("user", "q2")).unwrap();
        session.append(&Exchange::now("assistant", "a2".to_string(), ExchangeMeta {
            usage: Some(TokenUsage { input_tokens: Some(8), output_tokens: Some(3) }),
            duration_ms: Some(150),
            model: Some("gpt-4.1-mini".into()),
            ..Default::default()
        })).unwrap();
        let s = stats(tmp.path(), &session.id).unwrap();
        assert_eq!(s.total_input, 18);
        assert_eq!(s.total_output, 8);
        assert_eq!(s.total_dur_ms, 350);
        assert_eq!(s.exchanges.len(), 2);
    }
}
