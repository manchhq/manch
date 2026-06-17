//! Tauri commands the frontend invokes. Thin glue over `db` + `agent`.

use crate::agent::{
    claude_code_key, offerable_providers, AnthropicAgent, ChatAgent, ClaudeCodeAgent, Provider,
};
use crate::db::Db;
use tauri::State;

#[tauri::command]
pub fn save_api_key(state: State<Db>, provider: String, api_key: String) -> Result<(), String> {
    if Provider::from_id(&provider).is_none() {
        return Err(format!("unknown provider: {provider}"));
    }
    state.save_key(&provider, &api_key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_configured_providers(state: State<Db>) -> Result<Vec<String>, String> {
    let saved = state.list_providers().map_err(|e| e.to_string())?;
    Ok(offerable_providers(saved))
}

#[tauri::command]
pub async fn send_prompt(
    state: State<'_, Db>,
    provider: String,
    text: String,
) -> Result<String, String> {
    let prov =
        Provider::from_id(&provider).ok_or_else(|| format!("unknown provider: {provider}"))?;
    // Resolve owned keys here; the mutex guard is released inside `get_key`,
    // never held across the network/subprocess await below.
    let agent: Box<dyn ChatAgent> = match prov {
        Provider::Anthropic => {
            let key = state
                .get_key("anthropic")
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no API key saved for anthropic".to_string())?;
            Box::new(AnthropicAgent::new(key))
        }
        Provider::ClaudeCode => {
            let own = state.get_key("claude-code").map_err(|e| e.to_string())?;
            let ant = state.get_key("anthropic").map_err(|e| e.to_string())?;
            let key = claude_code_key(own, ant)
                .ok_or_else(|| "no API key saved for claude-code (or anthropic)".to_string())?;
            Box::new(ClaudeCodeAgent::new(key))
        }
    };
    agent.ask(&text).await
}
