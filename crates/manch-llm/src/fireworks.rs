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
use crate::{ModelInfo, ModelKind, Result};

/// Fireworks' OpenAI-compatible inference base.
pub const DEFAULT_BASE: &str = "https://api.fireworks.ai/inference/v1";

/// The id this provider answers to, distinct from `"openai"` so a host
/// routing on [`manch_protocol::Agent::id`] is told the truth.
pub const ID: &str = "fireworks";

/// Used when a caller names no model. OpenAI's own fallback is meaningless
/// here — Fireworks addresses models by account-qualified path — so a
/// compatible provider without its own default builds an agent that 404s on
/// first use.
///
/// **Verified against the live catalogue** (`GET {DEFAULT_BASE}/models`,
/// 2026-09-03), which is the only way to get this right: the first value
/// written here was `kimi-k2-instruct`, a plausible id that does not exist.
/// That is precisely the failure this module exists to prevent, so the id is
/// checked against the catalogue rather than inferred from a naming pattern.
pub const FALLBACK_MODEL: &str = "accounts/fireworks/models/kimi-k3";

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
    let resp = crate::http::shared()
        .client()
        .get(format!("{base}/models"))
        .bearer_auth(api_key)
        .send()
        .await;
    crate::list_models_with(resp, FALLBACK_MODEL, parse_models).await
}

/// Parse Fireworks' `/v1/models`.
///
/// **Not** `openai::parse_models`, which this delegated to until now. That
/// parser curates its list with an id heuristic matching the `gpt-*`,
/// `chatgpt*` and `o1|o3|o4` families — and no `accounts/fireworks/models/...`
/// id matches any of them, so *every* model was filtered out and `list_models`
/// silently degraded to the single fallback. Sharing a request dialect does not
/// mean sharing a catalogue.
///
/// Fireworks publishes real capability data, which is why this exists at all:
/// `context_length`, `supports_tools`, `supports_image_input` and `kind`.
///
/// `supports_chat` is deliberately ignored. It is `true` for every entry the
/// catalogue returns — including both embedding models — so it distinguishes
/// nothing. `kind` is what separates them.
pub(crate) fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    body.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let flag = |k: &str| m.get(k).and_then(|v| v.as_bool());
                    Some(ModelInfo {
                        // Absent *and* explicitly null both read as unknown:
                        // two live models publish `"context_length": null`, and
                        // a router told `0` would conclude nothing fits.
                        context_window: m
                            .get("context_length")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32),
                        supports_tools: flag("supports_tools"),
                        supports_image_input: flag("supports_image_input"),
                        kind: Some(model_kind(m.get("kind").and_then(|k| k.as_str()))),
                        ..ModelInfo::new(m.get("id")?.as_str()?)
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Map Fireworks' `kind` onto [`ModelKind`].
///
/// Fireworks' taxonomy describes *provenance* — `HF_BASE_MODEL`,
/// `CUSTOM_MODEL`, `EMBEDDING_MODEL` — not modality, so only `EMBEDDING_MODEL`
/// carries a capability. Everything else is something the account can prompt,
/// which is [`ModelKind::Chat`].
///
/// Note that `qwen3-reranker-8b` is filed under `EMBEDDING_MODEL` too, so a
/// `Rerank` kind cannot be derived from this field and is not invented here.
fn model_kind(kind: Option<&str>) -> ModelKind {
    match kind {
        Some("EMBEDDING_MODEL") => ModelKind::Embedding,
        _ => ModelKind::Chat,
    }
}

#[cfg(test)]
mod tests {
    use manch_protocol::Agent;

    use super::*;
    use crate::ModelKind;

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

    /// Captured verbatim from `GET {DEFAULT_BASE}/models` on 2026-09-04, trimmed
    /// to four entries that between them cover every shape the parser must
    /// handle: a vision+tools chat model, a plain chat model, an embedding
    /// model, and one whose `context_length` is null.
    fn catalogue() -> serde_json::Value {
        serde_json::json!({ "object": "list", "data": [
            { "id": "accounts/fireworks/models/kimi-k3", "object": "model",
              "owned_by": "fireworks", "kind": "HF_BASE_MODEL", "supports_chat": true,
              "supports_image_input": true, "supports_tools": true,
              "context_length": 1048576 },
            { "id": "accounts/fireworks/models/glm-5p2", "object": "model",
              "owned_by": "fireworks", "kind": "HF_BASE_MODEL", "supports_chat": true,
              "supports_image_input": false, "supports_tools": true,
              "context_length": 1048576 },
            { "id": "accounts/fireworks/models/qwen3-embedding-8b", "object": "model",
              "owned_by": "fireworks", "kind": "EMBEDDING_MODEL", "supports_chat": true,
              "supports_image_input": false, "supports_tools": false,
              "context_length": 40960 },
            { "id": "accounts/fireworks/models/qwen3p8-max", "object": "model",
              "owned_by": "fireworks", "kind": "HF_BASE_MODEL", "supports_chat": true,
              "supports_image_input": true, "supports_tools": true,
              "context_length": null }
        ]})
    }

    #[test]
    fn the_whole_catalogue_survives_parsing() {
        // The regression that motivated splitting this parser out: borrowing
        // OpenAI's dropped all 25 live models and left only the fallback.
        let ids: Vec<_> = parse_models(&catalogue())
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids.len(), 4, "every entry must survive, got {ids:?}");
    }

    #[test]
    fn capability_flags_are_read_rather_than_discarded() {
        let models = parse_models(&catalogue());
        let k3 = models
            .iter()
            .find(|m| m.id.ends_with("kimi-k3"))
            .expect("kimi-k3 present");
        assert_eq!(k3.context_window, Some(1_048_576));
        assert_eq!(k3.supports_tools, Some(true));
        assert_eq!(k3.supports_image_input, Some(true));
        assert_eq!(k3.kind, Some(ModelKind::Chat));
    }

    #[test]
    fn an_embedding_model_is_tagged_so_a_picker_can_exclude_it() {
        // `supports_chat` cannot do this job — Fireworks reports `true` for
        // embedding models too. `kind` is the field that separates them.
        let models = parse_models(&catalogue());
        let e = models
            .iter()
            .find(|m| m.id.ends_with("qwen3-embedding-8b"))
            .expect("embedding model present");
        assert_eq!(e.kind, Some(ModelKind::Embedding));
        assert_eq!(e.supports_tools, Some(false));
    }

    #[test]
    fn a_null_context_length_reads_as_unknown_not_zero() {
        // Two live models really do return null here. Reporting 0 would make a
        // router believe nothing fits in the window.
        let models = parse_models(&catalogue());
        let q = models
            .iter()
            .find(|m| m.id.ends_with("qwen3p8-max"))
            .expect("qwen3p8-max present");
        assert_eq!(q.context_window, None);
    }

    #[tokio::test]
    async fn the_dispatcher_routes_fireworks_instead_of_rejecting_it() {
        // `manch_llm::list_models_at` had no `"fireworks"` arm, so a consumer
        // asking the crate (rather than this module) got `NotFound`. Pointed at
        // a dead port, routing shows up as the graceful fallback rather than an
        // error — no network needed.
        let models = crate::list_models_at("fireworks", "k", Some("http://127.0.0.1:1"))
            .await
            .expect("fireworks must be a routed provider, not NotFound");
        assert_eq!(models[0].id, FALLBACK_MODEL);
    }
}
