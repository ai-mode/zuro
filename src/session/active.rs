use std::fs;
use std::path::Path;

use anyhow::Context;

use crate::constants::{ACTIVE_SESSION_FILE, ENV_SESSION};
use super::{Session, active_path};

pub fn get_active(data_dir: &Path) -> Option<String> {
    fs::read_to_string(active_path(data_dir))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn set_active(data_dir: &Path, id: &str) -> anyhow::Result<()> {
    fs::create_dir_all(data_dir)?;
    let path = active_path(data_dir);
    let tmp  = data_dir.join(format!("{ACTIVE_SESSION_FILE}.tmp"));
    fs::write(&tmp, id)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn resolve(flag: Option<&str>, data_dir: &Path) -> anyhow::Result<String> {
    if let Some(id) = flag {
        Session::open(data_dir, id)
            .with_context(|| format!("Session '{id}' not found (passed via --session)"))?;
        return Ok(id.into());
    }
    if let Ok(id) = std::env::var(ENV_SESSION) {
        if !id.is_empty() {
            Session::open(data_dir, &id)
                .with_context(|| format!("Session '{id}' not found (from $ZURO_SESSION)"))?;
            return Ok(id);
        }
    }
    if let Some(id) = get_active(data_dir) {
        if Session::open(data_dir, &id).is_ok() {
            return Ok(id);
        }
    }
    let session = Session::create(data_dir)?;
    set_active(data_dir, &session.id)?;
    Ok(session.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_data_dir() -> TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("sessions")).unwrap();
        tmp
    }

    #[test]
    fn get_active_returns_none_when_no_file() {
        let tmp = make_data_dir();
        assert_eq!(get_active(tmp.path()), None);
    }

    #[test]
    fn set_and_get_active() {
        let tmp = make_data_dir();
        set_active(tmp.path(), "test-id").unwrap();
        assert_eq!(get_active(tmp.path()), Some("test-id".to_string()));
    }

    #[test]
    fn resolve_uses_flag_first() {
        let tmp = make_data_dir();
        let flag_sess = Session::create(tmp.path()).unwrap();
        let file_sess = Session::create(tmp.path()).unwrap();
        set_active(tmp.path(), &file_sess.id).unwrap();
        let id = resolve(Some(&flag_sess.id), tmp.path()).unwrap();
        assert_eq!(id, flag_sess.id);
    }

    #[test]
    fn resolve_flag_nonexistent_session_is_error() {
        let tmp = make_data_dir();
        let result = resolve(Some("00000000-0000-0000-0000-000000000000"), tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn resolve_uses_active_file() {
        let tmp = make_data_dir();
        let session = Session::create(tmp.path()).unwrap();
        set_active(tmp.path(), &session.id).unwrap();
        let id = resolve(None, tmp.path()).unwrap();
        assert_eq!(id, session.id);
    }

    #[test]
    fn resolve_creates_new_session_when_nothing_set() {
        let tmp = make_data_dir();
        let id = resolve(None, tmp.path()).unwrap();
        assert_eq!(id.len(), 36);
        assert!(Session::open(tmp.path(), &id).is_ok());
    }

    #[test]
    fn resolve_creates_new_session_when_active_file_points_to_deleted_session() {
        let tmp = make_data_dir();
        set_active(tmp.path(), "00000000-0000-0000-0000-000000000000").unwrap();
        let id = resolve(None, tmp.path()).unwrap();
        assert_ne!(id, "00000000-0000-0000-0000-000000000000");
        assert!(Session::open(tmp.path(), &id).is_ok());
    }
}
