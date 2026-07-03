//! Tauri commands the frontend invokes. Thin glue over `db` + `agent`.

use std::sync::Arc;

use crate::agent::{ChannelSink, is_known_provider, offerable_providers, resolve_agent};
use crate::db::Db;
use manch_dto::{
    CreateSchedule, CreateTeam, CreateWorkspace, CrossVerify, Report, RunStep, Schedule, SearchHit,
    StreamEvent, Team, TeamRun, Workspace,
};
use manch_protocol::Context;
use tauri::{State, ipc::Channel};

#[tauri::command]
pub fn save_api_key(state: State<Db>, provider: String, api_key: String) -> Result<(), String> {
    if !is_known_provider(&provider) {
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

/// Fetch selectable models for a BYOK provider (needs a saved key).
#[tauri::command]
pub async fn list_models(
    state: State<'_, Db>,
    provider: String,
) -> Result<Vec<manch_llm::ModelInfo>, String> {
    let key = state
        .get_key(&provider)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no API key saved for {provider}"))?;
    manch_llm::list_models(&provider, &key)
        .await
        .map_err(|e| e.to_string())
}

/// Persist the user's chosen model for a provider.
#[tauri::command]
pub fn set_model(state: State<Db>, provider: String, model: String) -> Result<(), String> {
    state
        .set_model(&provider, &model)
        .map_err(|e| e.to_string())
}

/// Read back the persisted model for a provider (`None` if never chosen), so the
/// UI can default its dropdown to the saved choice instead of the first listed.
#[tauri::command]
pub fn get_model(state: State<Db>, provider: String) -> Result<Option<String>, String> {
    state.get_model(&provider).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_prompt_stream(
    state: State<'_, Db>,
    provider: String,
    text: String,
    channel: Channel<StreamEvent>,
) -> Result<(), String> {
    let agent = resolve_agent(&provider, &state)?;
    let ctx = Context {
        session_id: "desktop".to_string(),
        blocks: vec![manch_protocol::acp::ContentBlock::Text(
            manch_protocol::acp::TextContent::new(text),
        )],
    };
    let sink = Arc::new(ChannelSink(channel));
    match agent.prompt(ctx, &[], sink.clone()).await {
        Ok(_) => Ok(()),
        Err(e) => {
            sink.send_error(e.to_string());
            Ok(())
        }
    }
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
pub fn list_schedules(state: State<Db>, workspace_id: String) -> Result<Vec<Schedule>, String> {
    state
        .list_schedules(&workspace_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_schedule(state: State<Db>, input: CreateSchedule) -> Result<Schedule, String> {
    state.create_schedule(input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search(
    state: State<Db>,
    workspace_id: String,
    query: String,
    kinds: Vec<String>,
) -> Result<Vec<SearchHit>, String> {
    state
        .search(&workspace_id, &query, &kinds)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cross_verify(providers: Vec<String>, text: String) -> Result<CrossVerify, String> {
    if providers.is_empty() {
        return Err("select at least one provider".into());
    }
    let reports = providers
        .iter()
        .map(|p| Report {
            provider: p.clone(),
            text: format!("**{p}** analysis of \"{text}\": (mock) the claim appears consistent."),
        })
        .collect();
    Ok(CrossVerify {
        reports,
        summary: format!(
            "{} providers broadly agree (mock synthesis).",
            providers.len()
        ),
    })
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
