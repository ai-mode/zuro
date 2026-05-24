use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::Utc;
use fd_lock::RwLock;
use uuid::Uuid;

use crate::constants::MESSAGE_ID_PREFIX_LEN;
use super::{Exchange, SessionHeader, history_path, meta_path, session_dir};

pub struct Session {
    pub id:  String,
    pub dir: PathBuf,
}

impl Session {
    pub fn create(data_dir: &Path) -> anyhow::Result<Self> {
        let id  = Uuid::new_v4().to_string();
        let dir = session_dir(data_dir, &id);
        fs::create_dir_all(&dir)?;
        let header = SessionHeader {
            created_at:         Utc::now().to_rfc3339(),
            forked_from:        None,
            forked_at_exchange: None,
        };
        write_meta(&dir, &header)?;
        Ok(Self { id, dir })
    }

    pub fn open(data_dir: &Path, id: &str) -> anyhow::Result<Self> {
        let dir = session_dir(data_dir, id);
        anyhow::ensure!(dir.is_dir(), "Session '{id}' not found");
        Ok(Self { id: id.into(), dir })
    }

    pub fn load_header(&self) -> anyhow::Result<Option<SessionHeader>> {
        let path = meta_path(&self.dir);
        if !path.exists() { return Ok(None); }
        let s = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&s).ok())
    }

    pub fn load_exchanges(&self) -> anyhow::Result<Vec<Exchange>> {
        let path = history_path(&self.dir);
        if !path.exists() { return Ok(vec![]); }
        let file = fs::File::open(&path)
            .with_context(|| format!("Cannot open history: {}", path.display()))?;
        let mut out = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            let t = line.trim();
            if t.is_empty() { continue; }
            if let Ok(e) = serde_json::from_str::<Exchange>(t) { out.push(e); }
        }
        Ok(out)
    }

    pub fn append(&self, ex: &Exchange) -> anyhow::Result<()> {
        let path = history_path(&self.dir);
        let file = OpenOptions::new().create(true).append(true).open(&path)
            .with_context(|| format!("Cannot open history for writing: {}", path.display()))?;
        let mut lock  = RwLock::new(file);
        let mut guard = lock.write().context("Failed to acquire session lock")?;
        writeln!(*guard, "{}", serde_json::to_string(ex)?)?;
        Ok(())
    }

    pub fn fork(&self, data_dir: &Path) -> anyhow::Result<Session> {
        let new_id  = Uuid::new_v4().to_string();
        let new_dir = session_dir(data_dir, &new_id);
        fs::create_dir_all(&new_dir)?;
        let header = SessionHeader {
            created_at:         Utc::now().to_rfc3339(),
            forked_from:        Some(self.id.clone()),
            forked_at_exchange: None,
        };
        write_meta(&new_dir, &header)?;
        copy_history(&self.dir, &new_dir)?;
        Ok(Session { id: new_id, dir: new_dir })
    }

    pub fn fork_at(&self, data_dir: &Path, id_prefix: &str) -> anyhow::Result<Session> {
        let exchanges = self.load_exchanges()?;

        let pos = exchanges.iter().position(|e| e.message_id.starts_with(id_prefix))
            .or_else(|| exchanges.iter().rposition(|e| {
                e.meta.exchange_id.as_deref()
                    .map(|x| x.starts_with(id_prefix))
                    .unwrap_or(false)
            }))
            .ok_or_else(|| anyhow::anyhow!("No message or exchange matching '{id_prefix}'"))?;

        let new_id  = Uuid::new_v4().to_string();
        let new_dir = session_dir(data_dir, &new_id);
        fs::create_dir_all(&new_dir)?;

        let id_short = &id_prefix[..MESSAGE_ID_PREFIX_LEN.min(id_prefix.len())];
        let header = SessionHeader {
            created_at:         Utc::now().to_rfc3339(),
            forked_from:        Some(self.id.clone()),
            forked_at_exchange: Some(id_short.to_string()),
        };
        write_meta(&new_dir, &header)?;

        let hist_path = history_path(&new_dir);
        let mut file  = File::create(&hist_path)?;
        for ex in &exchanges[..=pos] {
            writeln!(file, "{}", serde_json::to_string(ex)?)?;
        }
        Ok(Session { id: new_id, dir: new_dir })
    }
}

fn write_meta(dir: &Path, header: &SessionHeader) -> anyhow::Result<()> {
    let path = meta_path(dir);
    let tmp  = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_string_pretty(header)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn copy_history(src_dir: &Path, dst_dir: &Path) -> anyhow::Result<()> {
    let src = history_path(src_dir);
    if src.exists() {
        fs::copy(&src, history_path(dst_dir))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::ExchangeMeta;
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
    fn create_returns_session_with_valid_id_and_dir() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        assert_eq!(session.id.len(), 36);
        assert!(session.dir.is_dir());
    }

    #[test]
    fn open_existing_session() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        let opened = Session::open(tmp.path(), &session.id).unwrap();
        assert_eq!(opened.id, session.id);
    }

    #[test]
    fn open_nonexistent_session_errors() {
        let tmp = make_data_dir();
        let result = Session::open(tmp.path(), "00000000-0000-0000-0000-000000000000");
        assert!(result.is_err());
    }

    #[test]
    fn append_and_load_exchanges_in_order() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        session.append(&make_exchange("user", "hello")).unwrap();
        session.append(&make_exchange("assistant", "world")).unwrap();
        session.append(&make_exchange("user", "again")).unwrap();
        let exchanges = session.load_exchanges().unwrap();
        assert_eq!(exchanges.len(), 3);
        assert_eq!(exchanges[0].role, "user");
        assert_eq!(exchanges[0].content, "hello");
        assert_eq!(exchanges[1].role, "assistant");
        assert_eq!(exchanges[2].content, "again");
    }

    #[test]
    fn load_exchanges_empty_when_no_history_file() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        let exchanges = session.load_exchanges().unwrap();
        assert!(exchanges.is_empty());
    }

    #[test]
    fn load_exchanges_skips_malformed_lines() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        session.append(&make_exchange("user", "good")).unwrap();
        let hist = history_path(&session.dir);
        let mut f = std::fs::OpenOptions::new().append(true).open(&hist).unwrap();
        use std::io::Write;
        writeln!(f, "{{bad json}}").unwrap();
        session.append(&make_exchange("assistant", "also good")).unwrap();
        let exchanges = session.load_exchanges().unwrap();
        assert_eq!(exchanges.len(), 2);
        assert_eq!(exchanges[0].content, "good");
        assert_eq!(exchanges[1].content, "also good");
    }

    #[test]
    fn fork_copies_history() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        session.append(&make_exchange("user", "a")).unwrap();
        session.append(&make_exchange("assistant", "b")).unwrap();
        let forked = session.fork(tmp.path()).unwrap();
        let exchanges = forked.load_exchanges().unwrap();
        assert_eq!(exchanges.len(), 2);
        assert_eq!(exchanges[0].content, "a");
    }

    #[test]
    fn fork_does_not_copy_pool() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        fs::write(session.dir.join("pool.json"), "[]").unwrap();
        let forked = session.fork(tmp.path()).unwrap();
        assert!(!forked.dir.join("pool.json").exists());
    }

    #[test]
    fn fork_sets_forked_from_in_header() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        let forked = session.fork(tmp.path()).unwrap();
        let header = forked.load_header().unwrap().unwrap();
        assert_eq!(header.forked_from, Some(session.id.clone()));
    }

    #[test]
    fn fork_at_truncates_at_exchange() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        session.append(&make_exchange("user", "q1")).unwrap();
        let ex = Exchange::now("assistant", "a1".to_string(), ExchangeMeta {
            exchange_id: Some("test-exchange-id".to_string()),
            ..Default::default()
        });
        session.append(&ex).unwrap();
        session.append(&make_exchange("user", "q2")).unwrap();
        session.append(&make_exchange("assistant", "a2")).unwrap();
        let forked = session.fork_at(tmp.path(), "test-exchange-id").unwrap();
        let exchanges = forked.load_exchanges().unwrap();
        assert_eq!(exchanges.len(), 2);
        assert_eq!(exchanges[1].content, "a1");
    }

    #[test]
    fn fork_at_unknown_prefix_errors() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        session.append(&make_exchange("user", "q")).unwrap();
        let result = session.fork_at(tmp.path(), "nonexistent-prefix");
        assert!(result.is_err());
    }
}
