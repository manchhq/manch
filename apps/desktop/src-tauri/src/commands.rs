//! Tauri commands the frontend invokes. Thin glue over `db` + `agent`.

use crate::agent::{AnthropicAgent, ChatAgent, ClaudeCodeAgent, Provider, offerable_providers};
use crate::db::Db;
use manch_dto::{
    CreateSchedule, CreateTeam, CreateWorkspace, CrossVerify, Report, RunStep, Schedule, SearchHit,
    StreamEvent, Team, TeamRun, Workspace,
};
use tauri::{State, ipc::Channel};

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

/// `EventSink` that forwards each event over a Tauri IPC channel to the frontend.
struct ChannelSink(Channel<StreamEvent>);
impl crate::agent::EventSink for ChannelSink {
    fn emit(&self, event: StreamEvent) {
        // A closed channel (window gone) is not actionable here.
        let _ = self.0.send(event);
    }
}

#[tauri::command]
pub async fn send_prompt_stream(
    state: State<'_, Db>,
    provider: String,
    text: String,
    channel: Channel<StreamEvent>,
) -> Result<(), String> {
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
            let key = state.get_key("claude-code").map_err(|e| e.to_string())?;
            Box::new(ClaudeCodeAgent::new(key))
        }
    };
    let sink = ChannelSink(channel);
    agent.stream(&text, &sink).await
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
