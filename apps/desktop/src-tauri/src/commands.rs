//! Tauri commands the frontend invokes. Thin glue over `db` + `agent`.

use crate::agent::{complete, Provider};
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
    state.list_providers().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_prompt(
    state: State<'_, Db>,
    provider: String,
    text: String,
) -> Result<String, String> {
    let prov =
        Provider::from_id(&provider).ok_or_else(|| format!("unknown provider: {provider}"))?;
    // Read the key and release the mutex guard BEFORE awaiting the network call.
    let key = state
        .get_key(&provider)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no API key saved for {provider}"))?;
    complete(prov, &key, &text).await
}
