//! Host-declared, exact-model generation policy. Unknown support is not permission.
use manch_protocol::{Error, Result};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default)]
pub struct GenerationOptions {
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub reasoning: Option<ReasoningOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningKind {
    GeminiBudget,
    GeminiLevel(ThinkingLevel),
    OpenAiEffort(ReasoningEffort),
    CompatibleBudget,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningOptions {
    /// -1 selects Gemini's dynamic budget; zero requests no thinking where supported.
    GeminiBudget(i32),
    GeminiLevel(ThinkingLevel),
    OpenAiEffort(ReasoningEffort),
    /// Fireworks' numeric reasoning_effort extension, never sent to OpenAI.
    CompatibleBudget(u32),
}
impl ReasoningOptions {
    pub fn kind(self) -> ReasoningKind {
        match self {
            Self::GeminiBudget(_) => ReasoningKind::GeminiBudget,
            Self::GeminiLevel(level) => ReasoningKind::GeminiLevel(level),
            Self::OpenAiEffort(effort) => ReasoningKind::OpenAiEffort(effort),
            Self::CompatibleBudget(_) => ReasoningKind::CompatibleBudget,
        }
    }
}

/// The host's verified knowledge for an exact provider/model pair. Catalog
/// unknowns stay None. This is explicit host policy, not a library assertion
/// that a vendor's model has the declared capabilities.
#[derive(Debug, Clone)]
pub struct ModelPolicy {
    pub provider: String,
    pub model: String,
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<bool>,
    pub top_p: Option<bool>,
    pub top_k: Option<bool>,
    pub reasoning: Vec<ReasoningKind>,
}
impl ModelPolicy {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: vec![],
        }
    }
}

fn unsupported() -> Error {
    Error::Unsupported(
        "generation option is unsupported or its model compatibility is unknown".into(),
    )
}
fn invalid() -> Error {
    Error::InvalidArguments("invalid generation settings or output ceiling".into())
}

pub(crate) fn validate(
    provider: &str,
    model: &str,
    cap: u32,
    options: &GenerationOptions,
    policy: &ModelPolicy,
) -> Result<()> {
    if policy.provider != provider || policy.model != model {
        return Err(unsupported());
    }
    if cap == 0 || policy.max_output_tokens.is_some_and(|limit| cap > limit) {
        return Err(invalid());
    }
    for (value, support, max) in [
        (options.temperature, policy.temperature, 2.0),
        (options.top_p, policy.top_p, 1.0),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value < 0.0 || value > max {
                return Err(invalid());
            }
            if support != Some(true) {
                return Err(unsupported());
            }
        }
    }
    // Current Anthropic API no longer exposes sampling controls. Avoid an
    // apparent host override enabling a wire path we cannot promise to support.
    if provider == "anthropic"
        && (options.temperature.is_some()
            || options.top_p.is_some()
            || options.top_k.is_some()
            || options.reasoning.is_some())
    {
        return Err(unsupported());
    }
    if let Some(k) = options.top_k {
        if k == 0 {
            return Err(invalid());
        }
        if policy.top_k != Some(true) || provider == "openai" {
            return Err(unsupported());
        }
    }
    if let Some(reasoning) = options.reasoning {
        if !policy.reasoning.contains(&reasoning.kind()) {
            return Err(unsupported());
        }
        match reasoning {
            ReasoningOptions::GeminiBudget(budget) if provider == "gemini" => {
                if budget < -1 || i64::from(budget) > i64::from(cap) {
                    return Err(invalid());
                }
            }
            ReasoningOptions::GeminiLevel(_) if provider == "gemini" => {}
            ReasoningOptions::OpenAiEffort(_)
                if provider == "openai" || provider == "fireworks" => {}
            ReasoningOptions::CompatibleBudget(budget) if provider == "fireworks" => {
                if budget == 0 || budget >= cap {
                    return Err(invalid());
                }
            }
            _ => return Err(unsupported()),
        }
    }
    Ok(())
}

pub(crate) fn configure(
    provider: &str,
    model: &str,
    current: u32,
    options: &GenerationOptions,
    policy: &ModelPolicy,
) -> Result<u32> {
    let cap = options
        .max_output_tokens
        .unwrap_or_else(|| current.min(policy.max_output_tokens.unwrap_or(current)));
    validate(provider, model, cap, options, policy)?;
    Ok(cap)
}

pub(crate) fn apply(
    body: &mut Value,
    provider: &str,
    model: &str,
    cap: u32,
    configuration: &Option<(GenerationOptions, ModelPolicy)>,
) -> Result<()> {
    if cap == 0 {
        return Err(invalid());
    }
    let Some((options, policy)) = configuration else {
        return Ok(());
    };
    validate(provider, model, cap, options, policy)?;
    let config = if provider == "gemini" {
        &mut body["generationConfig"]
    } else {
        &mut *body
    };
    for (key, value) in [
        ("temperature", options.temperature),
        (
            if provider == "gemini" {
                "topP"
            } else {
                "top_p"
            },
            options.top_p,
        ),
    ] {
        if let Some(value) = value {
            config[key] = json!(value);
        }
    }
    if let Some(k) = options.top_k {
        config[if provider == "gemini" {
            "topK"
        } else {
            "top_k"
        }] = json!(k);
    }
    if let Some(reasoning) = options.reasoning {
        match reasoning {
            ReasoningOptions::GeminiBudget(budget) => {
                config["thinkingConfig"] = json!({"thinkingBudget":budget})
            }
            ReasoningOptions::GeminiLevel(level) => {
                config["thinkingConfig"] = json!({"thinkingLevel":match level {ThinkingLevel::Minimal=>"MINIMAL",ThinkingLevel::Low=>"LOW",ThinkingLevel::Medium=>"MEDIUM",ThinkingLevel::High=>"HIGH"}})
            }
            ReasoningOptions::OpenAiEffort(effort) => {
                config["reasoning_effort"] = json!(match effort {
                    ReasoningEffort::None => "none",
                    ReasoningEffort::Minimal => "minimal",
                    ReasoningEffort::Low => "low",
                    ReasoningEffort::Medium => "medium",
                    ReasoningEffort::High => "high",
                    ReasoningEffort::XHigh => "xhigh",
                    ReasoningEffort::Max => "max",
                })
            }
            ReasoningOptions::CompatibleBudget(budget) => {
                config["reasoning_effort"] = json!(budget)
            }
        }
    }
    Ok(())
}
