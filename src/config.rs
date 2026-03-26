use std::path::PathBuf;

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub auth_file: PathBuf,
    pub callback_port: u16,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let home = resolve_home_dir()?;
        Ok(Self {
            auth_file: home.join(".codex").join("auth.json"),
            callback_port: 1455,
        })
    }
}

fn resolve_home_dir() -> Result<PathBuf> {
    for variable in ["HOME", "USERPROFILE"] {
        if let Some(value) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(value));
        }
    }

    bail!("HOME or USERPROFILE must be set to resolve ~/.codex/auth.json");
}

#[cfg(test)]
mod tests {
    use super::AppConfig;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn falls_back_to_userprofile_when_home_is_missing() {
        let _guard = ENV_LOCK.lock().expect("lock env mutex");
        let original_home = std::env::var_os("HOME");
        let original_userprofile = std::env::var_os("USERPROFILE");
        std::env::remove_var("HOME");
        std::env::set_var("USERPROFILE", "/tmp/windows-home");

        let result = AppConfig::from_env();

        if let Some(value) = original_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(value) = original_userprofile {
            std::env::set_var("USERPROFILE", value);
        } else {
            std::env::remove_var("USERPROFILE");
        }

        let config = result.expect("USERPROFILE should be accepted");
        assert_eq!(
            config.auth_file,
            std::path::PathBuf::from("/tmp/windows-home/.codex/auth.json")
        );
    }

    #[test]
    fn returns_an_error_when_no_supported_home_variable_exists() {
        let _guard = ENV_LOCK.lock().expect("lock env mutex");
        let original_home = std::env::var_os("HOME");
        let original_userprofile = std::env::var_os("USERPROFILE");
        std::env::remove_var("HOME");
        std::env::remove_var("USERPROFILE");

        let result = AppConfig::from_env();

        if let Some(value) = original_home {
            std::env::set_var("HOME", value);
        }
        if let Some(value) = original_userprofile {
            std::env::set_var("USERPROFILE", value);
        }

        let error = result.expect_err("from_env should fail without HOME or USERPROFILE");
        assert!(
            error.to_string().contains("HOME or USERPROFILE"),
            "unexpected error: {error}"
        );
    }
}
