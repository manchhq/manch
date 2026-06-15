//! BYOK completion via `rig`. The provider client sits *below* the (future)
//! manch-core loop; for this slice it just does one completion call.

// NOTE: the crate is `rig-core`, whose library name is `rig_core` (not `rig`).
// The doc snippets that use `rig::...` assume a renamed dependency; with the
// plain `rig-core = "0.38.2"` dep the import path is `rig_core::...`.
use rig_core::client::CompletionClient;
use rig_core::completion::Prompt;
use rig_core::providers::{anthropic, gemini};

/// Anthropic model id (authoritative per the claude-api skill — do NOT change this string).
const ANTHROPIC_MODEL: &str = "claude-opus-4-8";
/// Gemini model id. Verify against current Gemini model names at run time.
const GEMINI_MODEL: &str = "gemini-2.5-flash";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    Gemini,
}

impl Provider {
    pub fn from_id(id: &str) -> Option<Provider> {
        match id {
            "anthropic" => Some(Provider::Anthropic),
            "gemini" => Some(Provider::Gemini),
            _ => None,
        }
    }
}

/// Run one BYOK completion and return the assistant's text.
/// Errors are stringified for transport across the Tauri boundary.
pub async fn complete(provider: Provider, api_key: &str, prompt: &str) -> Result<String, String> {
    match provider {
        Provider::Anthropic => {
            // rig 0.38.2: clients are built via `Client::builder().api_key(...)...build()`,
            // and `build()` returns a `Result`. (There is no `ClientBuilder::new(api_key)`.)
            let client = anthropic::Client::builder()
                .api_key(api_key)
                .anthropic_version("2023-06-01")
                .build()
                .map_err(|e| e.to_string())?;
            let agent = client.agent(ANTHROPIC_MODEL).build();
            agent.prompt(prompt).await.map_err(|e| e.to_string())
        }
        Provider::Gemini => {
            // rig 0.38.2: `Client::new` returns a `Result`, unlike the doc snippet.
            let client = gemini::Client::new(api_key).map_err(|e| e.to_string())?;
            let agent = client.agent(GEMINI_MODEL).build();
            agent.prompt(prompt).await.map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_providers_parse() {
        assert_eq!(Provider::from_id("anthropic"), Some(Provider::Anthropic));
        assert_eq!(Provider::from_id("gemini"), Some(Provider::Gemini));
    }

    #[test]
    fn unknown_provider_is_none() {
        assert_eq!(Provider::from_id("openai"), None);
        assert_eq!(Provider::from_id(""), None);
    }
}
