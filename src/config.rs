use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub auth_file: PathBuf,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Self {
            auth_file: home.join(".codex").join("auth.json"),
        }
    }
}

