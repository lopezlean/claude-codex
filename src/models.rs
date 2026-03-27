use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Codex,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortLevel {
    Low,
    Medium,
    High,
}

const CODEX_MODELS: &[&str] = &[
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.3-codex",
    "gpt-5.2-codex",
    "gpt-5.2",
    "gpt-5.1-codex-max",
    "gpt-5.1-codex-mini",
];

const CHAT_COMPLETIONS_MODELS: &[&str] = &["gpt-4o", "gpt-4o-mini"];

pub fn backend_kind_for_token(access_token: &str) -> Result<BackendKind, AppError> {
    if access_token.starts_with("ey") {
        Ok(BackendKind::Codex)
    } else if access_token.starts_with("sk-") {
        Ok(BackendKind::ChatCompletions)
    } else {
        Err(AppError::Message(format!(
            "could not determine backend kind from access token format"
        )))
    }
}

pub fn default_model_for(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Codex => "gpt-5.4",
        BackendKind::ChatCompletions => "gpt-4o",
    }
}

pub fn available_models_for(backend: BackendKind) -> &'static [&'static str] {
    match backend {
        BackendKind::Codex => CODEX_MODELS,
        BackendKind::ChatCompletions => CHAT_COMPLETIONS_MODELS,
    }
}

pub fn is_supported_model(backend: BackendKind, model: &str) -> bool {
    available_models_for(backend).contains(&model)
}

pub fn resolve_model(
    backend: BackendKind,
    requested_model: Option<&str>,
) -> Result<String, AppError> {
    let model = requested_model.unwrap_or_else(|| default_model_for(backend));
    if is_supported_model(backend, model) {
        Ok(model.to_string())
    } else {
        let available = available_models_for(backend).join(", ");
        Err(AppError::Message(format!(
            "unsupported model '{model}' for {} backend. Available models: {available}",
            backend_name(backend)
        )))
    }
}

pub fn backend_name(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Codex => "codex",
        BackendKind::ChatCompletions => "chat-completions",
    }
}

pub fn default_effort() -> EffortLevel {
    EffortLevel::Medium
}

pub fn resolve_effort(
    backend: BackendKind,
    requested_effort: Option<&str>,
) -> Result<EffortLevel, AppError> {
    let Some(requested_effort) = requested_effort else {
        return Ok(default_effort());
    };

    if backend != BackendKind::Codex {
        return Err(AppError::Message(
            "--effort is only supported on the codex backend".to_string(),
        ));
    }

    match requested_effort {
        "low" => Ok(EffortLevel::Low),
        "medium" => Ok(EffortLevel::Medium),
        "high" => Ok(EffortLevel::High),
        other => Err(AppError::Message(format!(
            "unsupported effort '{other}'. Available levels: low, medium, high"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        available_models_for, backend_kind_for_token, default_effort, default_model_for,
        is_supported_model, resolve_effort, BackendKind, EffortLevel,
    };

    #[test]
    fn detects_codex_backend_from_jwt_like_tokens() {
        assert_eq!(
            backend_kind_for_token("ey.test.token").expect("codex token"),
            BackendKind::Codex
        );
    }

    #[test]
    fn detects_chat_completions_backend_from_api_keys() {
        assert_eq!(
            backend_kind_for_token("sk-test").expect("api key"),
            BackendKind::ChatCompletions
        );
    }

    #[test]
    fn codex_default_model_is_gpt_5_4() {
        assert_eq!(default_model_for(BackendKind::Codex), "gpt-5.4");
    }

    #[test]
    fn codex_catalog_contains_the_approved_models() {
        assert_eq!(
            available_models_for(BackendKind::Codex),
            &[
                "gpt-5.4",
                "gpt-5.4-mini",
                "gpt-5.3-codex",
                "gpt-5.2-codex",
                "gpt-5.2",
                "gpt-5.1-codex-max",
                "gpt-5.1-codex-mini",
            ]
        );
    }

    #[test]
    fn rejects_unsupported_codex_models() {
        assert!(!is_supported_model(BackendKind::Codex, "gpt-4o"));
    }

    #[test]
    fn default_effort_is_medium() {
        assert_eq!(default_effort(), EffortLevel::Medium);
    }

    #[test]
    fn resolves_supported_codex_effort_levels() {
        assert_eq!(
            resolve_effort(BackendKind::Codex, Some("low")).expect("low effort"),
            EffortLevel::Low
        );
        assert_eq!(
            resolve_effort(BackendKind::Codex, Some("medium")).expect("medium effort"),
            EffortLevel::Medium
        );
        assert_eq!(
            resolve_effort(BackendKind::Codex, Some("high")).expect("high effort"),
            EffortLevel::High
        );
    }

    #[test]
    fn rejects_effort_for_chat_completions() {
        let error = resolve_effort(BackendKind::ChatCompletions, Some("high"))
            .expect_err("chat completions should reject effort");
        assert!(
            error
                .to_string()
                .contains("--effort is only supported on the codex backend"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_unknown_effort_levels() {
        let error = resolve_effort(BackendKind::Codex, Some("extreme"))
            .expect_err("unknown effort should fail");
        assert!(
            error
                .to_string()
                .contains("unsupported effort 'extreme'. Available levels: low, medium, high"),
            "unexpected error: {error}"
        );
    }
}
