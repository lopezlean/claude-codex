#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::FileSessionStore;
    use crate::auth::session::{CodexAuthFile, CodexTokens};

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
            .save(&CodexAuthFile {
                auth_mode: Some("openai".to_string()),
                tokens: CodexTokens {
                    id_token: Some("id-token".to_string()),
                    access_token: Some("access-token".to_string()),
                    refresh_token: Some("refresh-token".to_string()),
                    account_id: Some("acct_123".to_string()),
                },
                last_refresh: Some("2026-03-26T12:00:00Z".to_string()),
            })
            .expect("auth should save");

        let raw = fs::read_to_string(path).expect("saved auth");
        assert!(raw.contains("\"auth_mode\":\"openai\""));
        assert!(raw.contains("\"access_token\":\"access-token\""));
        assert!(raw.contains("\"refresh_token\":\"refresh-token\""));
    }
}

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::auth::session::CodexAuthFile;

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

        let tmp_path = self.path.with_extension("json.tmp");
        let body = serde_json::to_vec(auth)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        fs::write(&tmp_path, body)?;
        fs::rename(tmp_path, &self.path)?;
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
