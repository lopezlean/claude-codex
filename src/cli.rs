use std::ffi::OsString;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCli {
    Run { claude_args: Vec<OsString> },
    Auth { command: AuthCommand },
    ProxyServe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthCommand {
    Login,
    Status,
    Logout,
}

pub fn parse<I, T>(args: I) -> Result<ParsedCli, AppError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let collected: Vec<OsString> = args.into_iter().map(Into::into).collect();
    match collected.get(1).and_then(|value| value.to_str()) {
        Some("auth") => parse_auth(&collected),
        Some("proxy") => parse_proxy(&collected),
        _ => Ok(ParsedCli::Run {
            claude_args: collected.into_iter().skip(1).collect(),
        }),
    }
}

fn parse_auth(args: &[OsString]) -> Result<ParsedCli, AppError> {
    match args.get(2).and_then(|value| value.to_str()) {
        Some("login") => Ok(ParsedCli::Auth {
            command: AuthCommand::Login,
        }),
        Some("status") => Ok(ParsedCli::Auth {
            command: AuthCommand::Status,
        }),
        Some("logout") => Ok(ParsedCli::Auth {
            command: AuthCommand::Logout,
        }),
        _ => Ok(ParsedCli::Run {
            claude_args: args.iter().skip(1).cloned().collect(),
        }),
    }
}

fn parse_proxy(args: &[OsString]) -> Result<ParsedCli, AppError> {
    match args.get(2).and_then(|value| value.to_str()) {
        Some("serve") => Ok(ParsedCli::ProxyServe),
        _ => Ok(ParsedCli::Run {
            claude_args: args.iter().skip(1).cloned().collect(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, AuthCommand, ParsedCli};

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
    fn parses_proxy_serve_command() {
        let parsed = parse(["claude-codex", "proxy", "serve"]).expect("proxy serve should parse");
        assert_eq!(parsed, ParsedCli::ProxyServe);
    }
}
