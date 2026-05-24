use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

use crate::{
    config::app_config::{AppConfig, CodexProfileConfig},
    engines::codex::CodexRuntimeProfile,
    models::{ThreadDto, WorkspaceDto},
    path_utils, runtime_env,
};

#[derive(Debug, Clone)]
pub struct DetectedCodexProjectProfile {
    pub profile_id: String,
    pub profile_name: String,
    pub thread_count: i64,
    pub last_activity_at: String,
    pub latest_thread_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DetectedCodexProjectThread {
    pub engine_thread_id: String,
    pub title: String,
    pub preview: String,
    pub created_at: String,
    pub updated_at: String,
    pub profile_id: String,
    pub profile_name: String,
    pub model_provider: String,
    pub archived: bool,
}

#[derive(Debug, Clone)]
pub struct CodexWorkspaceThread {
    pub engine_thread_id: String,
    pub title: String,
    pub preview: String,
    pub created_at: String,
    pub updated_at: String,
    pub profile_id: String,
    pub profile_name: String,
    pub model_provider: String,
    pub archived: bool,
}

#[derive(Debug, Clone)]
pub struct DetectedCodexProject {
    pub path: String,
    pub name: String,
    pub thread_count: i64,
    pub last_activity_at: String,
    pub workspace_id: Option<String>,
    pub profiles: Vec<DetectedCodexProjectProfile>,
    pub threads: Vec<DetectedCodexProjectThread>,
}

#[derive(Debug)]
struct ProfileThreadRow {
    engine_thread_id: String,
    cwd: String,
    title: String,
    first_user_message: String,
    created_at: i64,
    last_updated_at: i64,
    model_provider: String,
    archived: bool,
}

#[derive(Debug)]
struct AggregatedProjectProfile {
    profile_id: String,
    profile_name: String,
    thread_count: i64,
    last_updated_at: i64,
    latest_thread_title: Option<String>,
}

#[derive(Debug)]
struct AggregatedProject {
    path: String,
    name: String,
    thread_count: i64,
    last_updated_at: i64,
    workspace_id: Option<String>,
    profiles: HashMap<String, AggregatedProjectProfile>,
    threads: Vec<DetectedCodexProjectThread>,
}

pub fn runtime_profile_from_config(config: &AppConfig) -> CodexRuntimeProfile {
    runtime_profile_from_entry(config.codex.active_profile())
}

pub fn runtime_profile_by_id(config: &AppConfig, profile_id: &str) -> Option<CodexRuntimeProfile> {
    config
        .codex
        .profile_by_id(profile_id)
        .map(runtime_profile_from_entry)
}

pub fn codex_profile_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata
        .and_then(|value| value.get("codexProfileId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn codex_profile_id_for_thread(thread: &ThreadDto) -> String {
    codex_profile_id_from_metadata(thread.engine_metadata.as_ref())
        .unwrap_or_else(|| "default".to_string())
}

pub fn thread_uses_codex_profile(thread: &ThreadDto, profile_id: &str) -> bool {
    let normalized_profile_id = profile_id.trim();
    !normalized_profile_id.is_empty()
        && codex_profile_id_for_thread(thread) == normalized_profile_id
}

pub fn set_codex_profile_id(metadata: &mut Value, profile_id: &str) {
    if !metadata.is_object() {
        *metadata = json!({});
    }

    if let Some(object) = metadata.as_object_mut() {
        object.insert(
            "codexProfileId".to_string(),
            Value::String(profile_id.to_string()),
        );
    }
}

pub fn codex_profile_continuation_pending(metadata: Option<&Value>) -> bool {
    metadata
        .and_then(|value| value.get("codexProfileContinuationPending"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn clear_codex_profile_continuation_pending(metadata: &mut Value) {
    if !metadata.is_object() {
        *metadata = json!({});
    }

    if let Some(object) = metadata.as_object_mut() {
        object.remove("codexProfileContinuationPending");
        object.remove("codexProfileContinuationSourceEngineThreadId");
    }
}

pub fn codex_profile_continuation_source_engine_thread_id(
    metadata: Option<&Value>,
) -> Option<String> {
    metadata
        .and_then(|value| value.get("codexProfileContinuationSourceEngineThreadId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn set_codex_profile_continuation_pending(
    metadata: &mut Value,
    source_engine_thread_id: Option<&str>,
) {
    if !metadata.is_object() {
        *metadata = json!({});
    }

    if let Some(object) = metadata.as_object_mut() {
        object.insert(
            "codexProfileContinuationPending".to_string(),
            Value::Bool(true),
        );
        match source_engine_thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(engine_thread_id) => {
                object.insert(
                    "codexProfileContinuationSourceEngineThreadId".to_string(),
                    Value::String(engine_thread_id.to_string()),
                );
            }
            None => {
                object.remove("codexProfileContinuationSourceEngineThreadId");
            }
        }
    }
}

pub fn detect_codex_projects(
    config: &AppConfig,
    workspaces: &[WorkspaceDto],
) -> anyhow::Result<Vec<DetectedCodexProject>> {
    let workspace_ids_by_root = workspace_ids_by_canonical_root(workspaces);
    let home_dir =
        runtime_env::home_dir().and_then(|home| path_utils::canonicalize_path(&home).ok());
    let mut aggregated = HashMap::<String, AggregatedProject>::new();

    for profile in &config.codex.profiles {
        for row in read_threads_for_profile(profile)? {
            let Some(canonical_path) = normalize_existing_project_dir(&row.cwd) else {
                continue;
            };
            let rendered_path = canonical_path.to_string_lossy().to_string();
            let project_name = project_name_from_path(&canonical_path);
            let workspace_id = workspace_ids_by_root.get(&rendered_path).cloned();
            if workspace_id.is_none() && home_dir.as_ref() == Some(&canonical_path) {
                continue;
            }
            let normalized_title = normalize_detected_thread_title(
                &row.title,
                &row.first_user_message,
                &row.engine_thread_id,
            );
            let preview = normalize_detected_thread_preview(&row.first_user_message);

            let entry =
                aggregated
                    .entry(rendered_path.clone())
                    .or_insert_with(|| AggregatedProject {
                        path: rendered_path.clone(),
                        name: project_name,
                        thread_count: 0,
                        last_updated_at: row.last_updated_at,
                        workspace_id: workspace_id.clone(),
                        profiles: HashMap::new(),
                        threads: Vec::new(),
                    });

            entry.thread_count += 1;
            if row.last_updated_at > entry.last_updated_at {
                entry.last_updated_at = row.last_updated_at;
            }
            if entry.workspace_id.is_none() {
                entry.workspace_id = workspace_id.clone();
            }
            entry.threads.push(DetectedCodexProjectThread {
                engine_thread_id: row.engine_thread_id,
                title: normalized_title.clone(),
                preview,
                created_at: timestamp_to_rfc3339(row.created_at),
                updated_at: timestamp_to_rfc3339(row.last_updated_at),
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                model_provider: row.model_provider,
                archived: row.archived,
            });

            let profile_entry = entry.profiles.entry(profile.id.clone()).or_insert_with(|| {
                AggregatedProjectProfile {
                    profile_id: profile.id.clone(),
                    profile_name: profile.name.clone(),
                    thread_count: 0,
                    last_updated_at: row.last_updated_at,
                    latest_thread_title: Some(normalized_title.clone()),
                }
            });
            profile_entry.thread_count += 1;
            if row.last_updated_at >= profile_entry.last_updated_at {
                profile_entry.last_updated_at = row.last_updated_at;
                profile_entry.latest_thread_title = Some(normalized_title);
            }
        }
    }

    let mut projects = aggregated.into_values().collect::<Vec<_>>();
    for project in &mut projects {
        project
            .threads
            .sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    }
    projects.sort_by(|left, right| right.last_updated_at.cmp(&left.last_updated_at));

    Ok(projects
        .into_iter()
        .map(|project| {
            let mut profiles = project.profiles.into_values().collect::<Vec<_>>();
            profiles.sort_by(|left, right| right.last_updated_at.cmp(&left.last_updated_at));

            DetectedCodexProject {
                path: project.path,
                name: project.name,
                thread_count: project.thread_count,
                last_activity_at: timestamp_to_rfc3339(project.last_updated_at),
                workspace_id: project.workspace_id,
                profiles: profiles
                    .into_iter()
                    .map(|profile| DetectedCodexProjectProfile {
                        profile_id: profile.profile_id,
                        profile_name: profile.profile_name,
                        thread_count: profile.thread_count,
                        last_activity_at: timestamp_to_rfc3339(profile.last_updated_at),
                        latest_thread_title: profile.latest_thread_title,
                    })
                    .collect(),
                threads: project.threads,
            }
        })
        .collect())
}

pub fn detect_codex_threads_for_workspace(
    config: &AppConfig,
    workspace: &WorkspaceDto,
) -> anyhow::Result<Vec<CodexWorkspaceThread>> {
    let workspace_root = path_utils::canonicalize_path(Path::new(&workspace.root_path))
        .unwrap_or_else(|_| PathBuf::from(&workspace.root_path));
    let mut threads = Vec::new();

    for profile in &config.codex.profiles {
        for row in read_threads_for_profile(profile)? {
            let Some(canonical_path) = normalize_existing_project_dir(&row.cwd) else {
                continue;
            };
            if canonical_path != workspace_root && !canonical_path.starts_with(&workspace_root) {
                continue;
            }

            let title = normalize_detected_thread_title(
                &row.title,
                &row.first_user_message,
                &row.engine_thread_id,
            );
            let preview = normalize_detected_thread_preview(&row.first_user_message);
            threads.push(CodexWorkspaceThread {
                engine_thread_id: row.engine_thread_id,
                title,
                preview,
                created_at: timestamp_to_rfc3339(row.created_at),
                updated_at: timestamp_to_rfc3339(row.last_updated_at),
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                model_provider: row.model_provider,
                archived: row.archived,
            });
        }
    }

    threads.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(threads)
}

fn workspace_ids_by_canonical_root(workspaces: &[WorkspaceDto]) -> HashMap<String, String> {
    workspaces
        .iter()
        .filter_map(|workspace| {
            let root_path = workspace.root_path.trim();
            if root_path.is_empty() {
                return None;
            }

            let canonical_path = path_utils::canonicalize_path(Path::new(root_path))
                .unwrap_or_else(|_| PathBuf::from(root_path));
            Some((
                canonical_path.to_string_lossy().to_string(),
                workspace.id.clone(),
            ))
        })
        .collect()
}

pub fn build_codex_resume_command(profile: &CodexProfileConfig, engine_thread_id: &str) -> String {
    if cfg!(target_os = "windows") {
        let escaped_home = profile.codex_home.replace('"', "\"\"");
        let escaped_thread = engine_thread_id.replace('"', "\"\"");
        return format!("set CODEX_HOME={escaped_home}&& codex --resume \"{escaped_thread}\"");
    }

    format!(
        "env CODEX_HOME={} codex --resume {}",
        shell_single_quote_escape(&profile.codex_home),
        shell_single_quote_escape(engine_thread_id),
    )
}

fn runtime_profile_from_entry(entry: &CodexProfileConfig) -> CodexRuntimeProfile {
    CodexRuntimeProfile {
        id: entry.id.clone(),
        name: entry.name.clone(),
        codex_home: PathBuf::from(&entry.codex_home),
    }
}

fn read_threads_for_profile(profile: &CodexProfileConfig) -> anyhow::Result<Vec<ProfileThreadRow>> {
    let db_path = Path::new(&profile.codex_home).join("state_5.sqlite");
    if !db_path.is_file() {
        return Ok(Vec::new());
    }

    let connection = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open Codex state database `{}`",
            db_path.display()
        )
    })?;
    connection.busy_timeout(std::time::Duration::from_millis(1_500))?;

    read_threads_with_query(
        &connection,
        "SELECT id,
                cwd,
                title,
                first_user_message,
                created_at,
                updated_at,
                model_provider,
                archived
         FROM threads
         ORDER BY updated_at DESC, id DESC",
    )
}

fn read_threads_with_query(
    connection: &Connection,
    query: &str,
) -> anyhow::Result<Vec<ProfileThreadRow>> {
    let mut statement = connection.prepare(query)?;
    let rows = statement.query_map([], |row| {
        Ok(ProfileThreadRow {
            engine_thread_id: row.get(0)?,
            cwd: row.get(1)?,
            title: row.get(2)?,
            first_user_message: row.get(3)?,
            created_at: row.get(4)?,
            last_updated_at: row.get(5)?,
            model_provider: row.get(6)?,
            archived: row.get::<_, i64>(7)? != 0,
        })
    })?;

    let mut output = Vec::new();
    for row in rows {
        output.push(row?);
    }
    Ok(output)
}

fn normalize_detected_thread_title(
    title: &str,
    first_user_message: &str,
    engine_thread_id: &str,
) -> String {
    let normalized_title = title.trim();
    if !normalized_title.is_empty() {
        return normalized_title.to_string();
    }

    let preview = normalize_detected_thread_preview(first_user_message);
    if !preview.is_empty() {
        return preview;
    }

    format!("Codex thread {}", short_thread_label(engine_thread_id))
}

fn normalize_detected_thread_preview(first_user_message: &str) -> String {
    first_user_message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(180).collect::<String>())
        .unwrap_or_default()
}

fn short_thread_label(engine_thread_id: &str) -> String {
    engine_thread_id.chars().take(8).collect()
}

fn normalize_existing_project_dir(raw_cwd: &str) -> Option<PathBuf> {
    let trimmed = raw_cwd.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = Path::new(trimmed);
    if !path.is_dir() {
        return None;
    }

    path_utils::canonicalize_path(path).ok()
}

fn project_name_from_path(path: &Path) -> String {
    path.file_name()
        .map(|segment| segment.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn timestamp_to_rfc3339(timestamp: i64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn shell_single_quote_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}
