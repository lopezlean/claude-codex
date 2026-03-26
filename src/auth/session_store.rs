#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::FileSessionStore;
    use crate::auth::session::{CodexAuthFile, CodexTokens};

    fn sample_auth(access_token: &str, refresh_token: &str) -> CodexAuthFile {
        CodexAuthFile {
            auth_mode: Some("openai".to_string()),
            tokens: CodexTokens {
                id_token: Some("id-token".to_string()),
                access_token: Some(access_token.to_string()),
                refresh_token: Some(refresh_token.to_string()),
                account_id: Some("acct_123".to_string()),
            },
            last_refresh: Some("2026-03-26T12:00:00Z".to_string()),
        }
    }

    #[test]
    fn loads_existing_codex_auth_json() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("auth.json");
        fs::write(
            &path,
            r#"{
              "auth_mode":"openai",
              "tokens":{
                "id_token":"id-token",
                "access_token":"access-token",
                "refresh_token":"refresh-token",
                "account_id":"acct_123"
              },
              "last_refresh":"2026-03-26T12:00:00Z"
            }"#,
        )
        .expect("auth file should be written");

        let store = FileSessionStore::new(path.clone());
        let auth = store
            .load()
            .expect("auth should load")
            .expect("auth should exist");

        assert_eq!(auth.auth_mode.as_deref(), Some("openai"));
        assert_eq!(auth.tokens.access_token.as_deref(), Some("access-token"));
        assert_eq!(auth.tokens.refresh_token.as_deref(), Some("refresh-token"));
        assert_eq!(store.path(), path.as_path());
    }

    #[test]
    fn saves_the_same_shape_used_by_codex() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("auth.json");
        let store = FileSessionStore::new(path.clone());

        store
            .save(&sample_auth("access-token", "refresh-token"))
            .expect("auth should save");

        let raw = fs::read_to_string(path).expect("saved auth");
        assert!(raw.contains("\"auth_mode\":\"openai\""));
        assert!(raw.contains("\"access_token\":\"access-token\""));
        assert!(raw.contains("\"refresh_token\":\"refresh-token\""));
    }

    #[test]
    fn saving_twice_overwrites_existing_auth_file_contents_correctly() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("auth.json");
        let store = FileSessionStore::new(path.clone());

        store
            .save(&sample_auth("first-access-token", "first-refresh-token"))
            .expect("first auth save");
        store
            .save(&sample_auth("second-access-token", "second-refresh-token"))
            .expect("second auth save");

        let raw = fs::read_to_string(path).expect("saved auth");
        assert!(raw.contains("\"access_token\":\"second-access-token\""));
        assert!(raw.contains("\"refresh_token\":\"second-refresh-token\""));
        assert!(!raw.contains("\"access_token\":\"first-access-token\""));
        assert!(!raw.contains("\"refresh_token\":\"first-refresh-token\""));
    }

    #[test]
    fn save_ignores_an_occupied_default_temp_path() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("auth.json");
        let stale_tmp = path.with_extension("json.tmp");
        let store = FileSessionStore::new(path.clone());

        fs::create_dir(&stale_tmp).expect("stale temp directory");

        store
            .save(&sample_auth("access-token", "refresh-token"))
            .expect("auth save should not use the fixed temp path");

        let raw = fs::read_to_string(path).expect("saved auth");
        assert!(raw.contains("\"access_token\":\"access-token\""));
    }

    #[cfg(unix)]
    #[test]
    fn save_restricts_unix_permissions_for_auth_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("auth.json");
        let store = FileSessionStore::new(path.clone());

        store
            .save(&sample_auth("access-token", "refresh-token"))
            .expect("auth save");

        let mode = fs::metadata(path)
            .expect("auth metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode & 0o077,
            0,
            "auth file must not be group/world readable"
        );
    }
}

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use crate::auth::session::CodexAuthFile;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct FileSessionStore {
    path: PathBuf,
}

impl FileSessionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> io::Result<Option<CodexAuthFile>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&self.path)?;
        let parsed: CodexAuthFile = serde_json::from_str(&raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        Ok(Some(parsed))
    }

    pub fn save(&self, auth: &CodexAuthFile) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let body = serde_json::to_vec(auth)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let (mut temp_file, temp_path) = create_unique_temp_file(&self.path)?;
        if let Err(error) = temp_file
            .write_all(&body)
            .and_then(|_| temp_file.sync_all())
        {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
        drop(temp_file);

        if let Err(error) = replace_file(&temp_path, &self.path) {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn clear(&self) -> io::Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

fn create_unique_temp_file(path: &Path) -> io::Result<(File, PathBuf)> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("auth.json");

    for _ in 0..100 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            timestamp + u128::from(counter)
        ));

        match open_new_temp_file(&temp_path) {
            Ok(file) => return Ok((file, temp_path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to create a unique auth temp file",
    ))
}

#[cfg(unix)]
fn open_new_temp_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_new_temp_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::remove_file(destination) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    fs::rename(source, destination)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}
