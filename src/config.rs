use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub auth_file: PathBuf,
    pub callback_port: u16,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .expect("HOME must be set to resolve ~/.codex/auth.json");
        Self {
            auth_file: home.join(".codex").join("auth.json"),
            callback_port: 1455,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn panics_when_home_is_missing() {
        let _guard = ENV_LOCK.lock().expect("lock env mutex");
        let original_home = std::env::var_os("HOME");
        std::env::remove_var("HOME");

        let result = std::panic::catch_unwind(|| AppConfig::from_env());

        if let Some(value) = original_home {
            std::env::set_var("HOME", value);
        }

        assert!(result.is_err(), "from_env should not accept a missing HOME");
    }
}
