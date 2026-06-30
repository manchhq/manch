//! Tauri commands the frontend invokes. Thin glue over `db` + `agent`.

use crate::agent::{AnthropicAgent, ChatAgent, ClaudeCodeAgent, Provider, offerable_providers};
use crate::db::Db;
use manch_dto::{CreateTeam, CreateWorkspace, RunStep, Team, TeamRun, Workspace};
use tauri::State;

#[tauri::command]
pub fn save_api_key(state: State<Db>, provider: String, api_key: String) -> Result<(), String> {
    if Provider::from_id(&provider).is_none() {
        return Err(format!("unknown provider: {provider}"));
    }
    state
        .save_key(&provider, &api_key)
        .map_err(|e| e.to_string())
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
            // BYOC: Claude Code authenticates itself (its own login). A key saved
            // explicitly under "claude-code" is an optional BYOK override; none is normal.
            let key = state.get_key("claude-code").map_err(|e| e.to_string())?;
            Box::new(ClaudeCodeAgent::new(key))
        }
    };
    agent.ask(&text).await
}

#[tauri::command]
pub fn list_workspaces(state: State<Db>) -> Result<Vec<Workspace>, String> {
    state.list_workspaces().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_workspace(state: State<Db>, input: CreateWorkspace) -> Result<Workspace, String> {
    state
        .create_workspace(&input.name, &input.description)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_workspace(state: State<Db>, id: String, name: String) -> Result<Workspace, String> {
    state
        .rename_workspace(&id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_workspace(state: State<Db>, id: String) -> Result<(), String> {
    state.delete_workspace(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_teams(state: State<Db>, workspace_id: String) -> Result<Vec<Team>, String> {
    state.list_teams(&workspace_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_team(state: State<Db>, input: CreateTeam) -> Result<Team, String> {
    state.create_team(input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_team(state: State<Db>, id: String) -> Result<Team, String> {
    state
        .get_team(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "team not found".into())
}

#[tauri::command]
pub fn assign_team_task(
    state: State<Db>,
    team_id: String,
    task: String,
) -> Result<TeamRun, String> {
    let team = state
        .get_team(&team_id)
        .map_err(|e| e.to_string())?
        .ok_or("team not found")?;
    let steps = team
        .members
        .iter()
        .map(|m| RunStep {
            member_role: m.role.clone(),
            detail: format!("{} handled part of: {task}", m.role),
            status: "done".into(),
        })
        .collect();
    Ok(TeamRun {
        team_id,
        task,
        steps,
        result: "Synthesized result from the team (mock).".into(),
    })
}
