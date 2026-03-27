use std::ffi::OsString;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCli {
    Run { claude_args: Vec<OsString> },
    Auth { command: AuthCommand },
    Models { command: ModelsCommand },
    ProxyServe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthCommand {
    Login,
    Status,
    Logout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelsCommand {
    List,
}

pub fn parse<I, T>(args: I) -> Result<ParsedCli, AppError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let collected: Vec<OsString> = args.into_iter().map(Into::into).collect();
    match collected.get(1).and_then(|value| value.to_str()) {
        Some("auth") => parse_auth(&collected),
        Some("models") => parse_models(&collected),
        Some("proxy") => parse_proxy(&collected),
        _ => Ok(ParsedCli::Run {
            claude_args: collected.into_iter().skip(1).collect(),
        }),
    }
}

fn parse_models(args: &[OsString]) -> Result<ParsedCli, AppError> {
    let subcommand = args
        .get(2)
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::Message("missing models subcommand".to_string()))?;

    if args.len() != 3 {
        return Err(AppError::Message(format!(
            "models {subcommand} does not accept trailing arguments"
        )));
    }

    match subcommand {
        "list" => Ok(ParsedCli::Models {
            command: ModelsCommand::List,
        }),
        _ => Err(AppError::Message(format!(
            "unknown models subcommand: {subcommand}"
        ))),
    }
}

fn parse_auth(args: &[OsString]) -> Result<ParsedCli, AppError> {
    let subcommand = args
        .get(2)
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::Message("missing auth subcommand".to_string()))?;

    if args.len() != 3 {
        return Err(AppError::Message(format!(
            "auth {subcommand} does not accept trailing arguments"
        )));
    }

    match subcommand {
        "login" => Ok(ParsedCli::Auth {
            command: AuthCommand::Login,
        }),
        "status" => Ok(ParsedCli::Auth {
            command: AuthCommand::Status,
        }),
        "logout" => Ok(ParsedCli::Auth {
            command: AuthCommand::Logout,
        }),
        _ => Err(AppError::Message(format!(
            "unknown auth subcommand: {subcommand}"
        ))),
    }
}

fn parse_proxy(args: &[OsString]) -> Result<ParsedCli, AppError> {
    let subcommand = args
        .get(2)
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::Message("missing proxy subcommand".to_string()))?;

    if args.len() != 3 {
        return Err(AppError::Message(format!(
            "proxy {subcommand} does not accept trailing arguments"
        )));
    }

    match subcommand {
        "serve" => Ok(ParsedCli::ProxyServe),
        _ => Err(AppError::Message(format!(
            "unknown proxy subcommand: {subcommand}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, AuthCommand, ModelsCommand, ParsedCli};

    #[test]
    fn parses_auth_login_command() {
        let parsed = parse(["claude-codex", "auth", "login"]).expect("auth login should parse");
        assert_eq!(
            parsed,
            ParsedCli::Auth {
                command: AuthCommand::Login,
            }
        );
    }

    #[test]
    fn treats_unknown_words_as_claude_arguments() {
        let parsed =
            parse(["claude-codex", "--model", "claude-3-5-sonnet-latest"]).expect("run mode");

        assert_eq!(
            parsed,
            ParsedCli::Run {
                claude_args: vec!["--model".into(), "claude-3-5-sonnet-latest".into()],
            }
        );
    }

    #[test]
    fn treats_effort_flag_as_a_run_mode_argument() {
        let parsed = parse(["claude-codex", "--effort", "low", "--print", "hello"])
            .expect("run mode with effort");

        assert_eq!(
            parsed,
            ParsedCli::Run {
                claude_args: vec![
                    "--effort".into(),
                    "low".into(),
                    "--print".into(),
                    "hello".into()
                ],
            }
        );
    }

    #[test]
    fn parses_proxy_serve_command() {
        let parsed = parse(["claude-codex", "proxy", "serve"]).expect("proxy serve should parse");
        assert_eq!(parsed, ParsedCli::ProxyServe);
    }

    #[test]
    fn parses_models_list_command() {
        let parsed = parse(["claude-codex", "models", "list"]).expect("models list should parse");
        assert_eq!(
            parsed,
            ParsedCli::Models {
                command: ModelsCommand::List,
            }
        );
    }

    #[test]
    fn rejects_bad_auth_commands() {
        for args in [
            vec!["claude-codex", "auth"],
            vec!["claude-codex", "auth", "bogus"],
            vec!["claude-codex", "auth", "login", "extra"],
        ] {
            assert!(
                parse(args.clone()).is_err(),
                "unexpectedly parsed auth args: {args:?}"
            );
        }
    }

    #[test]
    fn rejects_bad_proxy_commands() {
        for args in [
            vec!["claude-codex", "proxy"],
            vec!["claude-codex", "proxy", "bogus"],
            vec!["claude-codex", "proxy", "serve", "extra"],
        ] {
            assert!(
                parse(args.clone()).is_err(),
                "unexpectedly parsed proxy args: {args:?}"
            );
        }
    }

    #[test]
    fn rejects_bad_models_commands() {
        for args in [
            vec!["claude-codex", "models"],
            vec!["claude-codex", "models", "bogus"],
            vec!["claude-codex", "models", "list", "extra"],
        ] {
            assert!(
                parse(args.clone()).is_err(),
                "unexpectedly parsed models args: {args:?}"
            );
        }
    }
}
