//! Fireworks — an **OpenAI-compatible** provider, served from Fireworks' own
//! base with its own catalogue and model ids.
//!
//! There is no separate wire implementation here and there should not be: the
//! dialect is OpenAI's, so this module is the three facts that differ (id,
//! base, default model) plus a catalogue call that reads Fireworks' list
//! rather than OpenAI's. Everything else is [`crate::openai`].
//!
//! Fireworks serves open-weight models — Kimi, Qwen, DeepSeek, Llama — which
//! makes it the cheap path for bulk work and for testing against real
//! inference without a frontier bill. A host chooses *when* to use it; this
//! module only makes it reachable.

use crate::openai::OpenAiAgent;
use crate::{ModelInfo, Result};

/// Fireworks' OpenAI-compatible inference base.
pub const DEFAULT_BASE: &str = "https://api.fireworks.ai/inference/v1";

/// The id this provider answers to, distinct from `"openai"` so a host
/// routing on [`manch_protocol::Agent::id`] is told the truth.
pub const ID: &str = "fireworks";

/// Used when a caller names no model. OpenAI's own fallback is meaningless
/// here — Fireworks addresses models by account-qualified path — so a
/// compatible provider without its own default builds an agent that 404s on
/// first use.
pub const FALLBACK_MODEL: &str = "accounts/fireworks/models/kimi-k2-instruct";

/// A Fireworks agent. Reads `MANCH_FIREWORKS_BASE_URL` when set, independently
/// of `MANCH_OPENAI_BASE_URL`, so pointing OpenAI at a proxy does not silently
/// redirect Fireworks as well.
pub fn agent(api_key: String, model: Option<String>) -> OpenAiAgent {
    OpenAiAgent::compatible(ID, ID, api_key, model, DEFAULT_BASE, FALLBACK_MODEL)
}

/// Fireworks' catalogue — **not** OpenAI's.
///
/// The distinction is the whole point: `openai::list_models` resolves the
/// OpenAI base, so calling it with a Fireworks key authenticates against the
/// wrong vendor and fails a perfectly good key.
pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    list_models_at(api_key, None).await
}

/// As [`list_models`], against an explicit base — a proxy in front of
/// Fireworks, say.
pub async fn list_models_at(api_key: &str, base: Option<&str>) -> Result<Vec<ModelInfo>> {
    let base = crate::resolve_base(ID, base, DEFAULT_BASE);
    crate::openai::list_models_at(api_key, Some(&base)).await
}

#[cfg(test)]
mod tests {
    use manch_protocol::Agent;

    use super::*;

    #[test]
    fn the_agent_reports_fireworks_not_openai() {
        // A host routing on `id` must not be told this is OpenAI: the key,
        // the bill and the catalogue all belong to a different vendor.
        let a = agent("k".to_string(), None);
        assert_eq!(a.id(), "fireworks");
    }

    #[test]
    fn the_default_base_is_fireworks() {
        let a = agent("k".to_string(), None);
        assert_eq!(a.base(), DEFAULT_BASE);
    }

    #[test]
    fn a_named_model_wins_over_the_fallback() {
        let a = agent("k".to_string(), Some("accounts/x/models/y".to_string()));
        assert_eq!(a.model_for_test(), "accounts/x/models/y");
    }

    #[test]
    fn the_fallback_is_not_an_openai_model_id() {
        // The bug this module exists to prevent: an OpenAI-compatible agent
        // defaulting to an OpenAI id that does not exist on the other vendor.
        assert_ne!(FALLBACK_MODEL, crate::openai::fallback_model_for_test());
    }
}
