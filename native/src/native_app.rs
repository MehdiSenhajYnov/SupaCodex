use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs,
    path::Path,
    path::PathBuf,
    rc::Rc,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use adw::prelude::*;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use gtk::{gdk, gio, glib};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::runtime::Runtime;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    codex_profiles::{
        codex_profile_id_for_thread, detect_codex_threads_for_workspace, runtime_profile_by_id,
        runtime_profile_from_config, set_codex_profile_id,
    },
    config::app_config::{AppConfig, CodexProfileConfig},
    db,
    engines::{
        approval_response_route_for_engine, normalize_approval_response_for_engine, EngineEvent,
        OutputStream, SandboxPolicy, ThreadScope, TurnAttachment, TurnCompletionStatus, TurnInput,
        TurnInputItem,
    },
    git::multi_repo,
    models::{
        MessageDto, MessageStatusDto, ThreadDto, ThreadStatusDto, TrustLevelDto, WorkspaceDto,
    },
};

const APP_ID: &str = "com.supacodex.app";
const DEFAULT_ENGINE_ID: &str = "codex";
const DEFAULT_MODEL_ID: &str = "gpt-5.3-codex";
const SIDEBAR_WIDTH: i32 = 320;
const SIDEBAR_PROJECTS_INITIAL_HEIGHT: i32 = 390;

const STYLE: &str = r#"
window.supacodex-window > contents,
window.supacodex-window:backdrop > contents,
window.supacodex-window dialog-host,
window.supacodex-window:backdrop dialog-host,
window.supacodex-window dialog-host > widget,
window.supacodex-window:backdrop dialog-host > widget,
window.supacodex-window dialog-host > widget > widget,
window.supacodex-window:backdrop dialog-host > widget > widget {
  background: transparent;
  background-color: transparent;
  background-image: none;
}

window.supacodex-window toastoverlay,
window.supacodex-window toastoverlay:backdrop,
window.supacodex-window overlay-split-view.supacodex-split-view,
window.supacodex-window:backdrop overlay-split-view.supacodex-split-view,
window.supacodex-window .sidebar-shell,
window.supacodex-window:backdrop .sidebar-shell,
window.supacodex-window .content-shell,
window.supacodex-window:backdrop .content-shell,
window.supacodex-window .chat-surface,
window.supacodex-window:backdrop .chat-surface {
  background: transparent;
  background-color: transparent;
  background-image: none;
  box-shadow: none;
}

window.supacodex-window headerbar.app-header,
window.supacodex-window headerbar.app-header:backdrop {
  background: transparent;
  background-image: none;
  border: none;
  box-shadow: none;
  color: @headerbar_fg_color;
  min-height: 48px;
  padding-left: 10px;
  padding-right: 10px;
}

.header-actions,
.header-actions:backdrop {
  background: transparent;
  background-image: none;
  border: none;
  box-shadow: none;
  padding: 0;
}

window.supacodex-window headerbar.app-header button,
window.supacodex-window headerbar.app-header button:backdrop,
window.supacodex-window headerbar.app-header entry,
window.supacodex-window headerbar.app-header entry:backdrop {
  opacity: 1;
}

window.supacodex-window headerbar.app-header button,
window.supacodex-window headerbar.app-header button:backdrop {
  background: alpha(@headerbar_fg_color, 0.030);
  border-color: alpha(@headerbar_fg_color, 0.030);
  box-shadow: none;
}

window.supacodex-window headerbar.app-header button:hover {
  background: alpha(@headerbar_fg_color, 0.10);
  border-color: alpha(@headerbar_fg_color, 0.08);
}

.app-title {
  margin: 0 12px;
  min-width: 0;
}

.app-title-main {
  color: @headerbar_fg_color;
  font-weight: 700;
}

.app-title-subtitle {
  color: alpha(@headerbar_fg_color, 0.62);
  font-size: 11px;
}

window.supacodex-window headerbar.app-header button.header-icon-button,
window.supacodex-window headerbar.app-header button.header-icon-button:backdrop {
  background: alpha(@headerbar_fg_color, 0.055);
  border: 1px solid alpha(@headerbar_fg_color, 0.065);
  border-radius: 999px;
  box-shadow: none;
  min-height: 34px;
  min-width: 34px;
  padding: 0;
}

window.supacodex-window headerbar.app-header button.header-icon-button:hover {
  background: alpha(@headerbar_fg_color, 0.12);
  border-color: alpha(@headerbar_fg_color, 0.10);
}

.sidebar-header,
.sidebar-header:backdrop {
  background: transparent;
  background-image: none;
  box-shadow: none;
  padding: 0 0 2px;
}

.app-search,
.app-search:backdrop {
  background: color-mix(in srgb, @window_fg_color 4%, transparent);
  border: 1px solid alpha(@window_fg_color, 0.060);
  border-radius: 999px;
  color: @window_fg_color;
  min-height: 34px;
  padding: 0 10px;
}

.app-search:focus {
  background: color-mix(in srgb, @window_fg_color 6%, transparent);
  border-color: alpha(@accent_bg_color, 0.32);
}

.app-search image {
  color: alpha(@window_fg_color, 0.68);
}

.sidebar-surface,
.sidebar-surface:backdrop {
  background: transparent;
  background-color: transparent;
  background-image: none;
  border: none;
  box-shadow: none;
  margin: 0;
  padding: 12px 16px 14px;
  transition: none;
}

.sidebar-actions {
  margin: 0 0 6px;
}

.sidebar-section {
  color: alpha(@window_fg_color, 0.58);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0;
  margin: 12px 6px 4px;
  text-transform: uppercase;
}

.sidebar-sections {
  margin: 0;
}

.sidebar-sections separator {
  background: transparent;
  border-radius: 999px;
  margin: 2px 6px;
  min-height: 8px;
}

.sidebar-sections separator:hover,
.sidebar-sections separator:active {
  background: alpha(@window_fg_color, 0.105);
}

.sidebar-section-pane,
.sidebar-section-pane:backdrop {
  background: transparent;
  background-image: none;
}

.sidebar-action {
  background-color: alpha(@window_fg_color, 0.040);
  background-image: none;
  border: 1px solid alpha(@window_fg_color, 0.055);
  border-radius: 8px;
  box-shadow: none;
  color: @window_fg_color;
  font-weight: 600;
  min-height: 38px;
  padding: 0 12px;
  transition: 140ms ease;
}

.sidebar-action:backdrop {
  background-color: alpha(@window_fg_color, 0.040);
  background-image: none;
  border: 1px solid alpha(@window_fg_color, 0.055);
  border-radius: 8px;
  box-shadow: none;
  color: @window_fg_color;
  font-weight: 600;
  min-height: 38px;
  padding: 0 12px;
}

.sidebar-action.sidebar-action-primary {
  background-color: @accent_bg_color;
  border-color: @accent_bg_color;
  color: @accent_fg_color;
}

.sidebar-action.sidebar-action-primary:backdrop {
  background-color: alpha(@accent_bg_color, 0.82);
  border-color: alpha(@accent_bg_color, 0.82);
  color: @accent_fg_color;
}

.sidebar-action:hover {
  background-color: alpha(@window_fg_color, 0.070);
  border-color: alpha(@window_fg_color, 0.095);
}

.sidebar-action.sidebar-action-primary:hover {
  background-color: color-mix(in srgb, @accent_bg_color 88%, white);
  border-color: color-mix(in srgb, @accent_bg_color 88%, white);
}

.sidebar-action:active {
  background-color: alpha(@window_fg_color, 0.095);
}

.sidebar-action-content {
  margin: 0;
}

.sidebar-action-icon {
  color: alpha(@window_fg_color, 0.76);
}

.sidebar-action.sidebar-action-primary .sidebar-action-icon {
  color: @accent_fg_color;
}

.sidebar-action-label {
  font-size: 13px;
  font-weight: 600;
}

.native-list,
.native-list:backdrop,
.native-list row,
.native-list row:backdrop {
  background: transparent;
  background-image: none;
  padding: 0;
}

.sidebar-scroll,
.sidebar-scroll:backdrop,
.sidebar-scroll viewport,
.sidebar-scroll viewport:backdrop {
  background: transparent;
  background-color: transparent;
  background-image: none;
  border: none;
  box-shadow: none;
}

.sidebar-scroll scrollbar,
.messages-scroll scrollbar {
  background: transparent;
  border: none;
  min-width: 7px;
}

.sidebar-scroll scrollbar trough,
.messages-scroll scrollbar trough {
  background: color-mix(in srgb, @window_fg_color 3%, transparent);
  border-radius: 999px;
  min-width: 7px;
}

.sidebar-scroll scrollbar slider,
.messages-scroll scrollbar slider {
  background: color-mix(in srgb, @window_fg_color 22%, transparent);
  border-radius: 999px;
  min-height: 30px;
  min-width: 7px;
}

.workspace-row,
.workspace-row:backdrop,
.thread-row,
.thread-row:backdrop {
  background-image: none;
  border-radius: 8px;
  margin: 1px 0;
  min-height: 40px;
  padding: 8px;
}

.workspace-row:hover,
.thread-row:hover {
  background: alpha(@window_fg_color, 0.060);
}

.workspace-row.active,
.workspace-row.active:backdrop,
.thread-row.active,
.thread-row.active:backdrop {
  background: alpha(@window_fg_color, 0.105);
  color: @window_fg_color;
}

.row-title {
  color: @window_fg_color;
  font-weight: 600;
}

.row-subtitle {
  color: alpha(@window_fg_color, 0.50);
  font-size: 11px;
}

.row-badge {
  background: color-mix(in srgb, @window_fg_color 10%, transparent);
  border-radius: 999px;
  color: alpha(@window_fg_color, 0.72);
  font-size: 11px;
  font-weight: 600;
  min-width: 22px;
  padding: 2px 7px;
}

.chat-surface,
.chat-surface:backdrop {
  background: transparent;
  padding: 4px 16px 14px;
}

.chat-subtitle,
.dim-label {
  color: alpha(@window_fg_color, 0.56);
}

.mode-pill,
.mode-pill:backdrop {
  background-color: alpha(@headerbar_fg_color, 0.045);
  background-image: none;
  border: 1px solid alpha(@headerbar_fg_color, 0.055);
  border-radius: 999px;
  box-shadow: none;
  color: alpha(@headerbar_fg_color, 0.88);
  font-weight: 600;
  min-height: 34px;
  padding: 0 12px;
}

.header-actions button,
.header-actions button:backdrop {
  background: transparent;
  border-color: transparent;
  box-shadow: none;
}

.header-actions button:hover {
  background: alpha(@headerbar_fg_color, 0.095);
  border-color: transparent;
}

.header-actions .mode-pill,
.header-actions .mode-pill:backdrop {
  background: alpha(@headerbar_fg_color, 0.050);
  border: 1px solid alpha(@headerbar_fg_color, 0.060);
  border-radius: 999px;
  min-width: 86px;
}

.header-actions .mode-pill:hover {
  background: alpha(@headerbar_fg_color, 0.105);
  border-color: alpha(@headerbar_fg_color, 0.085);
}

.header-actions button.header-icon-button,
.header-actions button.header-icon-button:backdrop {
  background: alpha(@headerbar_fg_color, 0.055);
  border: 1px solid alpha(@headerbar_fg_color, 0.065);
  border-radius: 999px;
}

.header-actions button.header-icon-button:hover {
  background: alpha(@headerbar_fg_color, 0.12);
  border-color: alpha(@headerbar_fg_color, 0.10);
}

.mode-pill label {
  font-weight: 600;
}

.runtime-popover,
.runtime-popover:backdrop {
  background: color-mix(in srgb, @window_bg_color 78%, transparent);
  border-radius: 12px;
  padding: 8px;
}

.runtime-option {
  border-radius: 8px;
  min-height: 34px;
  padding: 0 10px;
}

.runtime-option.active {
  background: alpha(@window_fg_color, 0.105);
}

.thread-tabbar,
.thread-tabbar:backdrop {
  background: transparent;
  background-color: transparent;
  background-image: none;
  border: none;
  box-shadow: none;
  color: alpha(@window_fg_color, 0.88);
  min-height: 38px;
  padding: 3px 4px 4px;
}

.thread-tabbar .box,
.thread-tabbar .box:backdrop {
  background: transparent;
  background-image: none;
  border: none;
  box-shadow: none;
}

.thread-tabbar tab,
.thread-tabbar tab:backdrop {
  min-height: 30px;
  opacity: 1;
}

.attachment-bar {
  padding: 0 0 4px;
}

.attachment-chip {
  background: color-mix(in srgb, @window_fg_color 7%, transparent);
  border: 1px solid alpha(@window_fg_color, 0.060);
  border-radius: 8px;
  margin-right: 6px;
  min-height: 34px;
  padding: 4px 6px;
}

.attachment-thumb {
  border-radius: 6px;
}

.attachment-chip button {
  background: transparent;
  border: none;
  box-shadow: none;
  min-height: 22px;
  min-width: 22px;
  padding: 0;
}

.message-toolbar button {
  background: transparent;
  border: none;
  box-shadow: none;
  min-height: 20px;
  min-width: 20px;
  padding: 0;
}

.message-edit-button {
  color: alpha(@window_fg_color, 0.58);
  opacity: 0.48;
}

.message-edit-button:hover {
  background: alpha(@window_fg_color, 0.075);
  opacity: 0.92;
}

.messages-list {
  padding: 4px 0 10px;
}

.message-card {
  background: alpha(@window_fg_color, 0.018);
  border: 1px solid alpha(@window_fg_color, 0.042);
  border-radius: 10px;
  padding: 8px 10px;
}

.message-card:backdrop {
  background: alpha(@window_fg_color, 0.018);
  border: 1px solid alpha(@window_fg_color, 0.042);
  border-radius: 10px;
  padding: 8px 10px;
}

.message-card.user-message,
.message-card.user-message:backdrop {
  background: color-mix(in srgb, @accent_bg_color 18%, transparent);
  border-color: color-mix(in srgb, @accent_bg_color 28%, transparent);
}

.message-card.assistant-message {
  background: alpha(@window_fg_color, 0.018);
}

.message-card.assistant-message:backdrop {
  background: alpha(@window_fg_color, 0.018);
}

.message-author {
  color: alpha(@window_fg_color, 0.55);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0;
  text-transform: uppercase;
}

.message-text {
  color: @window_fg_color;
  line-height: 1.45;
}

.block-card {
  background: alpha(@window_fg_color, 0.020);
  border: 1px solid alpha(@window_fg_color, 0.042);
  border-radius: 12px;
  padding: 9px 10px;
}

.block-card:backdrop {
  background: alpha(@window_fg_color, 0.020);
  border: 1px solid alpha(@window_fg_color, 0.042);
  border-radius: 12px;
  padding: 9px 10px;
}

.block-title {
  color: alpha(@window_fg_color, 0.90);
  font-weight: 700;
}

.code-output {
  background: color-mix(in srgb, @view_bg_color 36%, transparent);
  border-radius: 8px;
  color: alpha(@window_fg_color, 0.88);
  font-family: monospace;
  font-size: 12px;
  padding: 9px;
}

.approval-actions button {
  min-height: 30px;
}

.composer-wrap {
  background: transparent;
  border: 1px solid alpha(@window_fg_color, 0.035);
  border-radius: 12px;
  padding: 6px;
}

.composer-wrap:backdrop {
  background: transparent;
  border: 1px solid alpha(@window_fg_color, 0.035);
  border-radius: 12px;
  padding: 6px;
}

.composer-scroll {
  background: transparent;
  background-color: transparent;
  background-image: none;
  border: none;
  border-radius: 10px;
  box-shadow: none;
}

.composer-scroll:backdrop {
  background: transparent;
  background-color: transparent;
  background-image: none;
  border: none;
  border-radius: 10px;
  box-shadow: none;
}

.composer-scroll viewport,
textview.composer-view,
textview.composer-view.view,
textview.composer-view text {
  background: transparent;
  background-color: transparent;
  color: @window_fg_color;
}

.composer-scroll viewport:backdrop,
textview.composer-view:backdrop,
textview.composer-view.view:backdrop,
textview.composer-view text:backdrop {
  background: transparent;
  background-color: transparent;
  color: @window_fg_color;
}

textview.composer-view {
  color: @window_fg_color;
  min-height: 44px;
}

.send-button {
  border-radius: 999px;
  min-height: 34px;
  min-width: 34px;
}

.empty-state {
  color: alpha(@window_fg_color, 0.56);
  margin-top: 64px;
}

.status-dot {
  border-radius: 999px;
  min-height: 8px;
  min-width: 8px;
}

.status-idle { background: rgba(154, 164, 178, .70); }
.status-completed { background: rgba(95, 185, 138, .82); }
.status-streaming { background: rgba(92, 166, 255, .92); }
.status-awaiting_approval { background: rgba(255, 190, 92, .92); }
.status-error { background: rgba(255, 98, 118, .92); }
"#;

const COMPOSER_TRANSPARENCY_STYLE: &str = r#"
window.supacodex-window .composer-wrap,
window.supacodex-window .composer-wrap:backdrop,
window.supacodex-window .composer-scroll,
window.supacodex-window .composer-scroll:backdrop,
window.supacodex-window .composer-scroll viewport,
window.supacodex-window .composer-scroll viewport:backdrop,
window.supacodex-window .composer-scroll textview,
window.supacodex-window .composer-scroll textview:backdrop,
window.supacodex-window .composer-scroll textview.view,
window.supacodex-window .composer-scroll textview.view:backdrop,
window.supacodex-window .composer-scroll textview text,
window.supacodex-window .composer-scroll textview text:focus,
window.supacodex-window .composer-scroll textview text:backdrop,
window.supacodex-window textview.composer-view,
window.supacodex-window textview.composer-view:focus,
window.supacodex-window textview.composer-view:backdrop,
window.supacodex-window textview.composer-view text,
window.supacodex-window textview.composer-view text:focus,
window.supacodex-window textview.composer-view text:backdrop {
  background: transparent;
  background-color: transparent;
  background-image: none;
  box-shadow: none;
}
"#;

#[derive(Debug)]
enum UiEvent {
    Reload,
    SelectThread(String),
    OpenThreadTab(String),
    CloseThreadTab(String),
    EditMessage(String),
    SetCodexProfile(String),
    SetWorkspaceTrust(TrustLevelDto),
    RemoveAttachment(usize),
    Toast(String),
}

#[derive(Debug, Clone)]
struct PendingAttachment {
    file_name: String,
    file_path: String,
    size_bytes: u64,
    mime_type: Option<String>,
}

impl PendingAttachment {
    fn to_turn_attachment(&self) -> TurnAttachment {
        TurnAttachment {
            file_name: self.file_name.clone(),
            file_path: self.file_path.clone(),
            size_bytes: self.size_bytes,
            mime_type: self.mime_type.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum NativeContentBlock {
    #[serde(rename = "text")]
    Text {
        content: String,
        #[serde(rename = "planMode", skip_serializing_if = "Option::is_none")]
        plan_mode: Option<bool>,
        #[serde(rename = "isSteer", skip_serializing_if = "Option::is_none")]
        is_steer: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking { content: String },
    #[serde(rename = "diff")]
    Diff { diff: String, scope: String },
    #[serde(rename = "action")]
    Action {
        #[serde(rename = "actionId")]
        action_id: String,
        #[serde(rename = "engineActionId", skip_serializing_if = "Option::is_none")]
        engine_action_id: Option<String>,
        #[serde(rename = "actionType")]
        action_type: String,
        summary: String,
        details: Value,
        #[serde(rename = "outputChunks")]
        output_chunks: Vec<NativeActionOutputChunk>,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<NativeActionResult>,
    },
    #[serde(rename = "approval")]
    Approval {
        #[serde(rename = "approvalId")]
        approval_id: String,
        #[serde(rename = "actionType")]
        action_type: String,
        summary: String,
        details: Value,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision: Option<String>,
    },
    #[serde(rename = "notice")]
    Notice {
        kind: String,
        level: String,
        title: String,
        message: String,
    },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "attachment")]
    Attachment {
        #[serde(rename = "fileName")]
        file_name: String,
        #[serde(rename = "filePath")]
        file_path: String,
        #[serde(rename = "sizeBytes")]
        size_bytes: u64,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    #[serde(rename = "skill")]
    Skill { name: String, path: String },
    #[serde(rename = "mention")]
    Mention { name: String, path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NativeActionOutputChunk {
    stream: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeActionResult {
    success: bool,
    output: Option<String>,
    error: Option<String>,
    diff: Option<String>,
    duration_ms: u64,
}

struct NativeBackend {
    runtime: Arc<Runtime>,
    db: db::Database,
    config: Arc<Mutex<AppConfig>>,
    engines: Arc<crate::engines::EngineManager>,
    running: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl NativeBackend {
    fn new() -> anyhow::Result<Arc<Self>> {
        let _ = env_logger::try_init();

        let runtime = Arc::new(Runtime::new()?);
        let db = db::Database::init()?;
        let recovery = db::threads::reconcile_runtime_state(&db)?;
        if recovery.messages_marked_interrupted > 0 || recovery.thread_status_updates > 0 {
            log::info!(
                "runtime recovery applied: interrupted_messages={}, thread_status_updates={}",
                recovery.messages_marked_interrupted,
                recovery.thread_status_updates
            );
        }

        let config = Arc::new(Mutex::new(AppConfig::load_or_create()?));
        let _ = db::workspaces::ensure_default_workspace(&db)?;
        let engines = Arc::new(crate::engines::EngineManager::new());
        let initial_profile = {
            let config = config
                .lock()
                .map_err(|_| anyhow::anyhow!("app config lock is poisoned"))?;
            runtime_profile_from_config(&config)
        };
        runtime.block_on(engines.set_codex_profile(initial_profile))?;

        Ok(Arc::new(Self {
            runtime,
            db,
            config,
            engines,
            running: Arc::new(Mutex::new(HashMap::new())),
        }))
    }

    fn list_workspaces(&self) -> anyhow::Result<Vec<WorkspaceDto>> {
        db::workspaces::list_workspaces(&self.db)
    }

    fn list_threads(&self, workspace_id: &str) -> anyhow::Result<Vec<ThreadDto>> {
        self.sync_codex_threads_for_workspace(workspace_id)?;
        db::threads::list_threads_for_workspace(&self.db, workspace_id)
    }

    fn get_thread(&self, thread_id: &str) -> anyhow::Result<Option<ThreadDto>> {
        db::threads::get_thread(&self.db, thread_id)
    }

    fn get_messages(&self, thread_id: &str) -> anyhow::Result<Vec<MessageDto>> {
        if let Err(error) = self.sync_codex_thread_transcript_if_needed(thread_id) {
            log::warn!("failed to sync codex thread transcript for {thread_id}: {error:#}");
        }
        db::messages::get_thread_messages(&self.db, thread_id)
    }

    fn open_workspace(&self, path: &str) -> anyhow::Result<WorkspaceDto> {
        let workspace = db::workspaces::upsert_workspace(&self.db, path, Some(3))?;
        let repos =
            multi_repo::scan_git_repositories(&workspace.root_path, workspace.scan_depth as usize)?;
        let repo_paths = repos
            .iter()
            .map(|repo| repo.path.clone())
            .collect::<Vec<_>>();
        db::repos::reconcile_workspace_repos(&self.db, &workspace.id, &repo_paths)?;
        let selection_configured =
            db::workspaces::is_git_repo_selection_configured(&self.db, &workspace.id)?;

        for repo in repos {
            let _ = db::repos::upsert_repo(
                &self.db,
                &workspace.id,
                &repo.name,
                &repo.path,
                &repo.default_branch,
                !selection_configured,
            );
        }

        Ok(workspace)
    }

    fn create_thread(&self, workspace_id: &str) -> anyhow::Result<ThreadDto> {
        let created = db::threads::create_thread(
            &self.db,
            workspace_id,
            None,
            DEFAULT_ENGINE_ID,
            DEFAULT_MODEL_ID,
            "New thread",
        )?;

        let active_profile_id = self
            .runtime
            .block_on(self.engines.active_codex_profile())
            .id;
        let mut metadata = created.engine_metadata.clone().unwrap_or_else(|| json!({}));
        set_codex_profile_id(&mut metadata, &active_profile_id);
        db::threads::update_engine_metadata(&self.db, &created.id, &metadata)?;

        db::threads::get_thread(&self.db, &created.id)?
            .ok_or_else(|| anyhow::anyhow!("thread not found after creation"))
    }

    fn sync_codex_threads_for_workspace(&self, workspace_id: &str) -> anyhow::Result<usize> {
        let workspace = db::workspaces::find_workspace_by_id(&self.db, workspace_id)?
            .ok_or_else(|| anyhow::anyhow!("workspace not found: {workspace_id}"))?;
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("app config lock is poisoned"))?
            .clone();
        let detected = detect_codex_threads_for_workspace(&config, &workspace)?;

        let mut synced = 0usize;
        for thread in detected {
            let metadata = json!({
                "codexProfileId": thread.profile_id,
                "codexProfileName": thread.profile_name,
                "codexRemoteCreatedAt": thread.created_at,
                "codexRemoteUpdatedAt": thread.updated_at,
                "codexPreview": thread.preview,
                "codexModelProvider": thread.model_provider,
                "codexImportedFromCli": true,
            });
            db::threads::upsert_imported_codex_thread(
                &self.db,
                workspace_id,
                &thread.engine_thread_id,
                DEFAULT_MODEL_ID,
                &thread.title,
                metadata
                    .get("codexRemoteCreatedAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                metadata
                    .get("codexRemoteUpdatedAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                thread.archived,
                &metadata,
            )?;
            synced += 1;
        }

        Ok(synced)
    }

    fn sync_codex_thread_transcript_if_needed(&self, thread_id: &str) -> anyhow::Result<()> {
        let thread = db::threads::get_thread(&self.db, thread_id)?
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        if thread.engine_id != "codex" || thread.engine_thread_id.is_none() {
            return Ok(());
        }
        if self.is_running(&thread.id) || !codex_transcript_sync_needed(&thread) {
            return Ok(());
        }

        let snapshot = self.runtime.block_on(async {
            self.set_codex_profile_for_thread(&thread).await?;
            self.engines
                .read_codex_thread_transcript_snapshot(&thread)
                .await
        })?;
        let Some(snapshot) = snapshot else {
            return Ok(());
        };

        let effective_model_id = thread_last_model_id(thread.engine_metadata.as_ref())
            .unwrap_or_else(|| thread.model_id.clone());
        let reasoning_effort = thread_reasoning_effort(thread.engine_metadata.as_ref());
        let imported_messages = snapshot
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| db::messages::ImportedThreadMessage {
                role: message.role.as_str().to_string(),
                text: message.content.clone(),
                created_at: transcript_message_timestamp(snapshot.created_at, index),
            })
            .collect::<Vec<_>>();

        db::messages::replace_thread_messages(
            &self.db,
            &thread.id,
            &imported_messages,
            Some("codex"),
            Some(&effective_model_id),
            reasoning_effort.as_deref(),
        )?;
        db::threads::refresh_thread_message_stats(&self.db, &thread.id)?;

        let mut metadata = thread.engine_metadata.clone().unwrap_or_else(|| json!({}));
        if !metadata.is_object() {
            metadata = json!({});
        }
        let remote_updated_at = metadata
            .get("codexRemoteUpdatedAt")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| snapshot.updated_at.map(timestamp_to_rfc3339));
        if let Some(remote_updated_at) = remote_updated_at {
            metadata["codexTranscriptSyncedRemoteUpdatedAt"] = Value::String(remote_updated_at);
        }
        metadata["codexTranscriptSyncedAt"] = Value::String(Utc::now().to_rfc3339());

        db::threads::update_thread_runtime_snapshot(
            &self.db,
            &thread.id,
            snapshot.sync.title.as_deref(),
            None,
            Some(&metadata),
        )?;

        Ok(())
    }

    async fn set_codex_profile_for_thread(&self, thread: &ThreadDto) -> anyhow::Result<()> {
        if thread.engine_id != "codex" {
            return Ok(());
        }

        let profile = {
            let config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("app config lock is poisoned"))?;
            let profile_id = codex_profile_id_for_thread(thread);
            runtime_profile_by_id(&config, &profile_id)
                .unwrap_or_else(|| runtime_profile_from_config(&config))
        };

        self.engines.set_codex_profile(profile).await
    }

    fn codex_profiles(&self) -> Vec<CodexProfileConfig> {
        self.config
            .lock()
            .map(|config| config.codex.profiles.clone())
            .unwrap_or_default()
    }

    fn active_codex_profile_id(&self) -> String {
        self.config
            .lock()
            .map(|config| config.codex.active_profile_id.clone())
            .unwrap_or_else(|_| "default".to_string())
    }

    fn set_active_codex_profile(&self, profile_id: &str) -> anyhow::Result<()> {
        let profile = AppConfig::mutate(|config| {
            config.codex.active_profile_id = profile_id.to_string();
            config.codex.normalize();
            runtime_profile_by_id(config, profile_id)
                .ok_or_else(|| anyhow::anyhow!("unknown Codex profile: {profile_id}"))
        })?;

        {
            let mut config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("app config lock is poisoned"))?;
            *config = AppConfig::load_or_create()?;
        }

        self.runtime
            .block_on(self.engines.set_codex_profile(profile))?;
        Ok(())
    }

    fn workspace_trust_level(&self, workspace_id: &str) -> TrustLevelDto {
        db::repos::get_repos(&self.db, workspace_id)
            .map(|repos| aggregate_workspace_trust_level(&repos))
            .unwrap_or(TrustLevelDto::Standard)
    }

    fn set_workspace_trust_level(
        &self,
        workspace_id: &str,
        trust_level: TrustLevelDto,
    ) -> anyhow::Result<()> {
        let repos = db::repos::get_repos(&self.db, workspace_id)?;
        if repos.is_empty() {
            anyhow::bail!("No git repository is configured for this workspace.");
        }
        for repo in repos {
            db::repos::set_repo_trust_level(&self.db, &repo.id, trust_level.clone())?;
        }
        Ok(())
    }

    fn is_running(&self, thread_id: &str) -> bool {
        self.running
            .lock()
            .map(|running| running.contains_key(thread_id))
            .unwrap_or(false)
    }

    fn send_message(
        self: &Arc<Self>,
        thread_id: String,
        message: String,
        attachments: Vec<TurnAttachment>,
        ui_tx: mpsc::Sender<UiEvent>,
    ) {
        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            if let Err(error) = backend
                .run_message_turn(thread_id.clone(), message, attachments, ui_tx.clone())
                .await
            {
                let _ = ui_tx.send(UiEvent::Toast(format!("{error:#}")));
                let _ = ui_tx.send(UiEvent::Reload);
                backend.finish_running(&thread_id);
            }
        });
    }

    fn cancel_turn(self: &Arc<Self>, thread_id: String, ui_tx: mpsc::Sender<UiEvent>) {
        let cancellation = self
            .running
            .lock()
            .ok()
            .and_then(|running| running.get(&thread_id).cloned());
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
        }

        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            match db::threads::get_thread(&backend.db, &thread_id) {
                Ok(Some(thread)) => {
                    if let Err(error) = backend.engines.interrupt(&thread).await {
                        log::debug!("failed to interrupt engine turn: {error}");
                    }
                }
                Ok(None) => {}
                Err(error) => log::debug!("failed to load thread for cancellation: {error}"),
            }
            let _ = ui_tx.send(UiEvent::Reload);
        });
    }

    fn edit_and_resume(
        self: &Arc<Self>,
        thread_id: String,
        message_id: String,
        message: String,
        ui_tx: mpsc::Sender<UiEvent>,
    ) {
        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let result = async {
                let message = message.trim().to_string();
                if message.is_empty() {
                    anyhow::bail!("Le message modifie ne peut pas etre vide.");
                }

                let thread = db::threads::get_thread(&backend.db, &thread_id)?
                    .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
                if backend.is_running(&thread.id) {
                    anyhow::bail!("Impossible de modifier un thread pendant une generation.");
                }

                let messages = db::messages::get_thread_messages(&backend.db, &thread.id)?;
                let target_index = messages
                    .iter()
                    .position(|candidate| candidate.id == message_id)
                    .ok_or_else(|| anyhow::anyhow!("message not found: {message_id}"))?;
                if messages[target_index].role != "user" {
                    anyhow::bail!("Seuls les messages utilisateur peuvent etre modifies.");
                }

                let turns_to_drop = messages
                    .iter()
                    .skip(target_index)
                    .filter(|candidate| candidate.role == "user")
                    .count() as u32;
                if turns_to_drop == 0 {
                    anyhow::bail!("Aucun tour utilisateur a reprendre.");
                }

                if thread.engine_id == "codex" {
                    if let Some(engine_thread_id) = thread.engine_thread_id.as_deref() {
                        backend
                            .engines
                            .rollback_codex_thread(engine_thread_id, turns_to_drop)
                            .await?;
                    }
                }

                db::messages::drop_last_turns(&backend.db, &thread.id, turns_to_drop)?;
                db::threads::refresh_thread_message_stats(&backend.db, &thread.id)?;
                let _ = ui_tx.send(UiEvent::SelectThread(thread.id.clone()));
                let _ = ui_tx.send(UiEvent::Reload);

                backend
                    .run_message_turn(thread.id, message, Vec::new(), ui_tx.clone())
                    .await
            }
            .await;

            if let Err(error) = result {
                let _ = ui_tx.send(UiEvent::Toast(format!("{error:#}")));
                let _ = ui_tx.send(UiEvent::Reload);
            }
        });
    }

    fn respond_to_approval(
        self: &Arc<Self>,
        thread_id: String,
        approval_id: String,
        details: Value,
        decision: &'static str,
        ui_tx: mpsc::Sender<UiEvent>,
    ) {
        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let result = async {
                let thread = db::threads::get_thread(&backend.db, &thread_id)?
                    .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
                let response = normalize_approval_response_for_engine(
                    &thread.engine_id,
                    json!({ "decision": decision }),
                )
                .map_err(|error| anyhow::anyhow!(error))?;
                let route = approval_response_route_for_engine(&thread.engine_id, &details);
                backend
                    .engines
                    .respond_to_approval(&thread, &approval_id, response, route)
                    .await?;
                Ok::<_, anyhow::Error>(())
            }
            .await;

            if let Err(error) = result {
                let _ = ui_tx.send(UiEvent::Toast(format!("{error:#}")));
            }
            let _ = ui_tx.send(UiEvent::Reload);
        });
    }

    async fn run_message_turn(
        &self,
        thread_id: String,
        message: String,
        attachments: Vec<TurnAttachment>,
        ui_tx: mpsc::Sender<UiEvent>,
    ) -> anyhow::Result<()> {
        if message.trim().is_empty() && attachments.is_empty() {
            return Ok(());
        }

        let mut thread = db::threads::get_thread(&self.db, &thread_id)?
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        if self.is_running(&thread.id) {
            anyhow::bail!("A turn is already running for this thread.");
        }

        let effective_model_id = thread_last_model_id(thread.engine_metadata.as_ref())
            .unwrap_or_else(|| thread.model_id.clone());
        let reasoning_effort = thread_reasoning_effort(thread.engine_metadata.as_ref());
        let (scope, sandbox) =
            self.execution_scope_and_sandbox(&thread, reasoning_effort.clone())?;

        self.set_codex_profile_for_thread(&thread).await?;

        let engine_thread_id = self
            .engines
            .ensure_engine_thread(&thread, Some(&effective_model_id), scope, sandbox)
            .await?;
        if thread.engine_thread_id.as_deref() != Some(engine_thread_id.as_str()) {
            db::threads::set_engine_thread_id(&self.db, &thread.id, &engine_thread_id)?;
            thread.engine_thread_id = Some(engine_thread_id.clone());
        }

        let user_content = if message.trim().is_empty() {
            format!(
                "{} fichier{} joint{}.",
                attachments.len(),
                if attachments.len() > 1 { "s" } else { "" },
                if attachments.len() > 1 { "s" } else { "" }
            )
        } else {
            message.clone()
        };
        let mut user_blocks = Vec::new();
        if !message.trim().is_empty() {
            user_blocks.push(NativeContentBlock::Text {
                content: message.clone(),
                plan_mode: None,
                is_steer: None,
            });
        }
        for attachment in &attachments {
            user_blocks.push(NativeContentBlock::Attachment {
                file_name: attachment.file_name.clone(),
                file_path: attachment.file_path.clone(),
                size_bytes: attachment.size_bytes,
                mime_type: attachment.mime_type.clone(),
            });
        }
        db::messages::insert_user_message(
            &self.db,
            &thread.id,
            &user_content,
            Some(serde_json::to_value(&user_blocks)?),
            Some(&thread.engine_id),
            Some(&effective_model_id),
            reasoning_effort.as_deref(),
        )?;
        let assistant_message = db::messages::insert_assistant_placeholder(
            &self.db,
            &thread.id,
            Some(&thread.engine_id),
            Some(&effective_model_id),
            reasoning_effort.as_deref(),
        )?;
        db::threads::update_thread_status(&self.db, &thread.id, ThreadStatusDto::Streaming)?;
        self.maybe_title_thread(&thread, &user_content)?;

        let cancellation = CancellationToken::new();
        self.running
            .lock()
            .map_err(|_| anyhow::anyhow!("running-turn registry is poisoned"))?
            .insert(thread.id.clone(), cancellation.clone());

        let _ = ui_tx.send(UiEvent::SelectThread(thread.id.clone()));
        let _ = ui_tx.send(UiEvent::Reload);

        let (event_tx, mut event_rx) = tokio_mpsc::channel::<EngineEvent>(128);
        let engines = self.engines.clone();
        let thread_for_engine = thread.clone();
        let engine_message = if message.trim().is_empty() {
            "Utilise les fichiers joints comme contexte.".to_string()
        } else {
            message.clone()
        };
        let input = TurnInput {
            message: engine_message.clone(),
            attachments,
            plan_mode: false,
            input_items: vec![TurnInputItem::Text {
                text: engine_message,
            }],
        };
        let engine_thread_id_for_task = engine_thread_id.clone();
        let cancellation_for_task = cancellation.clone();
        let engine_task = tokio::spawn(async move {
            engines
                .send_message(
                    &thread_for_engine,
                    &engine_thread_id_for_task,
                    input,
                    event_tx,
                    cancellation_for_task,
                )
                .await
        });

        let mut blocks = Vec::<NativeContentBlock>::new();
        let mut action_index = HashMap::<String, usize>::new();
        let mut approval_index = HashMap::<String, usize>::new();
        let mut message_status = MessageStatusDto::Streaming;
        let mut thread_status = ThreadStatusDto::Streaming;
        let mut token_usage = None;

        self.apply_event(
            &assistant_message.id,
            &mut blocks,
            &mut action_index,
            &mut approval_index,
            &EngineEvent::TurnStarted {
                client_turn_id: None,
            },
            &mut message_status,
            &mut thread_status,
            &mut token_usage,
        )?;

        while let Some(event) = event_rx.recv().await {
            self.apply_event(
                &assistant_message.id,
                &mut blocks,
                &mut action_index,
                &mut approval_index,
                &event,
                &mut message_status,
                &mut thread_status,
                &mut token_usage,
            )?;
            let _ = ui_tx.send(UiEvent::Reload);
        }

        match engine_task.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                message_status = MessageStatusDto::Error;
                thread_status = ThreadStatusDto::Error;
                blocks.push(NativeContentBlock::Error {
                    message: format!("Engine error: {error:#}"),
                });
            }
            Err(error) => {
                message_status = MessageStatusDto::Error;
                thread_status = ThreadStatusDto::Error;
                blocks.push(NativeContentBlock::Error {
                    message: format!("Engine task join error: {error}"),
                });
            }
        }

        if cancellation.is_cancelled() && matches!(message_status, MessageStatusDto::Streaming) {
            message_status = MessageStatusDto::Interrupted;
            thread_status = ThreadStatusDto::Idle;
        }
        if matches!(message_status, MessageStatusDto::Streaming) {
            message_status = MessageStatusDto::Completed;
        }
        if matches!(thread_status, ThreadStatusDto::Streaming) {
            thread_status = ThreadStatusDto::Completed;
        }

        self.persist_blocks(
            &assistant_message.id,
            &blocks,
            message_status.clone(),
            &effective_model_id,
        )?;
        db::messages::complete_assistant_message(
            &self.db,
            &assistant_message.id,
            message_status.clone(),
            token_usage,
            Some(&effective_model_id),
        )?;
        db::threads::update_thread_status(&self.db, &thread.id, thread_status)?;
        if matches!(message_status, MessageStatusDto::Completed) {
            db::threads::bump_message_counters(&self.db, &thread.id, token_usage)?;
        }

        self.finish_running(&thread.id);
        let _ = ui_tx.send(UiEvent::Reload);
        Ok(())
    }

    fn execution_scope_and_sandbox(
        &self,
        thread: &ThreadDto,
        reasoning_effort: Option<String>,
    ) -> anyhow::Result<(ThreadScope, SandboxPolicy)> {
        let workspace = db::workspaces::find_workspace_by_id(&self.db, &thread.workspace_id)?
            .ok_or_else(|| anyhow::anyhow!("workspace not found: {}", thread.workspace_id))?;
        let repos = db::repos::get_repos(&self.db, &thread.workspace_id)?;
        let selected_repo = thread
            .repo_id
            .as_deref()
            .and_then(|repo_id| repos.iter().find(|repo| repo.id == repo_id));

        let scope = if let Some(repo) = selected_repo {
            ThreadScope::Repo {
                repo_path: repo.path.clone(),
            }
        } else {
            ThreadScope::Workspace {
                root_path: workspace.root_path.clone(),
                writable_roots: vec![workspace.root_path.clone()],
            }
        };

        let trust_level = selected_repo
            .map(|repo| repo.trust_level.clone())
            .unwrap_or_else(|| aggregate_workspace_trust_level(&repos));
        let approval_policy =
            approval_policy_for_engine_and_trust_level(&thread.engine_id, &trust_level);

        let writable_roots = match &scope {
            ThreadScope::Repo { repo_path } => vec![repo_path.clone()],
            ThreadScope::Workspace {
                writable_roots,
                root_path,
            } => {
                if writable_roots.is_empty() {
                    vec![root_path.clone()]
                } else {
                    writable_roots.clone()
                }
            }
        };

        Ok((
            scope,
            SandboxPolicy {
                writable_roots,
                allow_network: matches!(trust_level, TrustLevelDto::Trusted),
                approval_policy: Some(Value::String(approval_policy.to_string())),
                reasoning_effort,
                sandbox_mode: Some("workspace-write".to_string()),
                service_tier: None,
                personality: None,
                output_schema: None,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_event(
        &self,
        assistant_message_id: &str,
        blocks: &mut Vec<NativeContentBlock>,
        action_index: &mut HashMap<String, usize>,
        approval_index: &mut HashMap<String, usize>,
        event: &EngineEvent,
        message_status: &mut MessageStatusDto,
        thread_status: &mut ThreadStatusDto,
        token_usage: &mut Option<(u64, u64)>,
    ) -> anyhow::Result<()> {
        match event {
            EngineEvent::TurnStarted { .. } => {}
            EngineEvent::TurnCompleted {
                token_usage: usage,
                status,
            } => {
                *token_usage = usage.as_ref().map(|usage| (usage.input, usage.output));
                match status {
                    TurnCompletionStatus::Completed => {
                        *message_status = MessageStatusDto::Completed;
                        *thread_status = ThreadStatusDto::Completed;
                    }
                    TurnCompletionStatus::Interrupted => {
                        *message_status = MessageStatusDto::Interrupted;
                        *thread_status = ThreadStatusDto::Idle;
                    }
                    TurnCompletionStatus::Failed => {
                        *message_status = MessageStatusDto::Error;
                        *thread_status = ThreadStatusDto::Error;
                    }
                }
            }
            EngineEvent::TextDelta { content } => {
                append_text_block(blocks, content);
            }
            EngineEvent::ThinkingDelta { content } => {
                append_thinking_block(blocks, content);
            }
            EngineEvent::DiffUpdated { diff, scope } => {
                blocks.push(NativeContentBlock::Diff {
                    diff: diff.clone(),
                    scope: format!("{scope:?}").to_lowercase(),
                });
            }
            EngineEvent::ActionStarted {
                action_id,
                engine_action_id,
                action_type,
                summary,
                details,
            } => {
                action_index.insert(action_id.clone(), blocks.len());
                blocks.push(NativeContentBlock::Action {
                    action_id: action_id.clone(),
                    engine_action_id: engine_action_id.clone(),
                    action_type: action_type.as_str().to_string(),
                    summary: summary.clone(),
                    details: details.clone(),
                    output_chunks: Vec::new(),
                    status: "running".to_string(),
                    result: None,
                });
            }
            EngineEvent::ActionOutputDelta {
                action_id,
                stream,
                content,
            } => {
                if let Some(index) = action_index.get(action_id).copied() {
                    if let Some(NativeContentBlock::Action { output_chunks, .. }) =
                        blocks.get_mut(index)
                    {
                        output_chunks.push(NativeActionOutputChunk {
                            stream: stream_label(stream).to_string(),
                            content: content.clone(),
                        });
                    }
                }
            }
            EngineEvent::ActionProgressUpdated { action_id, message } => {
                if let Some(index) = action_index.get(action_id).copied() {
                    if let Some(NativeContentBlock::Action { summary, .. }) = blocks.get_mut(index)
                    {
                        *summary = message.clone();
                    }
                }
            }
            EngineEvent::ActionCompleted { action_id, result } => {
                if let Some(index) = action_index.get(action_id).copied() {
                    if let Some(NativeContentBlock::Action {
                        status,
                        result: stored,
                        ..
                    }) = blocks.get_mut(index)
                    {
                        *status = if result.success { "completed" } else { "error" }.to_string();
                        *stored = Some(NativeActionResult {
                            success: result.success,
                            output: result.output.clone(),
                            error: result.error.clone(),
                            diff: result.diff.clone(),
                            duration_ms: result.duration_ms,
                        });
                    }
                }
            }
            EngineEvent::ApprovalRequested {
                approval_id,
                action_type,
                summary,
                details,
            } => {
                *thread_status = ThreadStatusDto::AwaitingApproval;
                approval_index.insert(approval_id.clone(), blocks.len());
                blocks.push(NativeContentBlock::Approval {
                    approval_id: approval_id.clone(),
                    action_type: action_type.as_str().to_string(),
                    summary: summary.clone(),
                    details: details.clone(),
                    status: "pending".to_string(),
                    decision: None,
                });
            }
            EngineEvent::UsageLimitsUpdated { .. } => {}
            EngineEvent::ModelRerouted {
                from_model,
                to_model,
                reason,
            } => {
                blocks.push(NativeContentBlock::Notice {
                    kind: "model_rerouted".to_string(),
                    level: "info".to_string(),
                    title: format!("{from_model} -> {to_model}"),
                    message: reason.clone(),
                });
            }
            EngineEvent::Notice {
                kind,
                level,
                title,
                message,
            } => {
                blocks.push(NativeContentBlock::Notice {
                    kind: kind.clone(),
                    level: level.clone(),
                    title: title.clone(),
                    message: message.clone(),
                });
            }
            EngineEvent::Error { message, .. } => {
                *message_status = MessageStatusDto::Error;
                *thread_status = ThreadStatusDto::Error;
                blocks.push(NativeContentBlock::Error {
                    message: message.clone(),
                });
            }
        }

        self.persist_blocks(assistant_message_id, blocks, message_status.clone(), "")?;
        Ok(())
    }

    fn persist_blocks(
        &self,
        assistant_message_id: &str,
        blocks: &[NativeContentBlock],
        status: MessageStatusDto,
        model_id: &str,
    ) -> anyhow::Result<()> {
        let blocks_json = serde_json::to_string(blocks)?;
        let model_id = if model_id.is_empty() {
            None
        } else {
            Some(model_id)
        };
        db::messages::update_assistant_blocks_json(
            &self.db,
            assistant_message_id,
            &blocks_json,
            status,
            model_id,
        )?;
        Ok(())
    }

    fn maybe_title_thread(&self, thread: &ThreadDto, message: &str) -> anyhow::Result<()> {
        if thread.message_count != 0 || thread.title != "New thread" {
            return Ok(());
        }
        let title = normalize_thread_title(message).unwrap_or_else(|| "New thread".to_string());
        db::threads::update_thread_title(&self.db, &thread.id, &title)?;
        Ok(())
    }

    fn finish_running(&self, thread_id: &str) {
        if let Ok(mut running) = self.running.lock() {
            running.remove(thread_id);
        }
    }

    fn shutdown(&self) {
        self.runtime.block_on(self.engines.shutdown());
    }
}

struct AppController {
    backend: Arc<NativeBackend>,
    ui_tx: mpsc::Sender<UiEvent>,
    ui_rx: RefCell<mpsc::Receiver<UiEvent>>,
    toast_overlay: adw::ToastOverlay,
    window: adw::ApplicationWindow,
    title_label: gtk::Label,
    subtitle_label: gtk::Label,
    profile_button: gtk::MenuButton,
    profile_button_label: gtk::Label,
    permission_button: gtk::MenuButton,
    permission_button_label: gtk::Label,
    tab_bar: adw::TabBar,
    tab_view: adw::TabView,
    workspace_list: gtk::ListBox,
    thread_list: gtk::ListBox,
    messages_box: gtk::Box,
    messages_scroll: gtk::ScrolledWindow,
    attachment_bar: gtk::Box,
    composer: gtk::TextView,
    send_button: gtk::Button,
    search_entry: gtk::SearchEntry,
    workspaces: RefCell<Vec<WorkspaceDto>>,
    threads: RefCell<Vec<ThreadDto>>,
    visible_workspace_ids: RefCell<Vec<String>>,
    visible_thread_ids: RefCell<Vec<String>>,
    thread_tabs_by_workspace: RefCell<HashMap<String, Vec<String>>>,
    syncing_tab_view: Cell<bool>,
    pending_attachments: RefCell<Vec<PendingAttachment>>,
    active_workspace_id: RefCell<Option<String>>,
    active_thread_id: RefCell<Option<String>>,
    split_view: adw::OverlaySplitView,
}

impl AppController {
    fn new(app: &adw::Application, backend: Arc<NativeBackend>) -> Rc<Self> {
        let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>();

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("SupaCodex")
            .default_width(1460)
            .default_height(920)
            .width_request(980)
            .height_request(640)
            .build();
        window.add_css_class("supacodex-window");

        let toast_overlay = adw::ToastOverlay::new();
        window.set_content(Some(&toast_overlay));

        let split_view = adw::OverlaySplitView::new();
        split_view.add_css_class("supacodex-split-view");
        split_view.set_enable_hide_gesture(true);
        split_view.set_enable_show_gesture(true);
        split_view.set_pin_sidebar(true);
        split_view.set_show_sidebar(true);
        split_view.set_min_sidebar_width(SIDEBAR_WIDTH as f64);
        split_view.set_max_sidebar_width((SIDEBAR_WIDTH + 48) as f64);
        split_view.set_sidebar_width_fraction(0.24);
        toast_overlay.set_child(Some(&split_view));

        let sidebar_shell = adw::ToolbarView::new();
        sidebar_shell.add_css_class("sidebar-shell");
        sidebar_shell.set_top_bar_style(adw::ToolbarStyle::Flat);
        split_view.set_sidebar(Some(&sidebar_shell));

        let sidebar_header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        sidebar_header.add_css_class("sidebar-header");

        let main_shell = adw::ToolbarView::new();
        main_shell.add_css_class("content-shell");
        main_shell.add_css_class("view");
        main_shell.set_top_bar_style(adw::ToolbarStyle::Flat);
        split_view.set_content(Some(&main_shell));

        let header = adw::HeaderBar::new();
        header.add_css_class("app-header");
        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        title_box.add_css_class("app-title");
        title_box.set_halign(gtk::Align::Center);
        title_box.set_valign(gtk::Align::Center);

        let title_label = gtk::Label::new(Some("SupaCodex"));
        title_label.add_css_class("app-title-main");
        title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_label.set_justify(gtk::Justification::Center);
        title_label.set_max_width_chars(48);
        title_label.set_width_chars(1);
        title_label.set_xalign(0.5);

        let subtitle_label = gtk::Label::new(Some("codex"));
        subtitle_label.add_css_class("app-title-subtitle");
        subtitle_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        subtitle_label.set_justify(gtk::Justification::Center);
        subtitle_label.set_max_width_chars(42);
        subtitle_label.set_width_chars(1);
        subtitle_label.set_xalign(0.5);

        title_box.append(&title_label);
        title_box.append(&subtitle_label);
        header.set_title_widget(Some(&title_box));
        main_shell.add_top_bar(&header);

        let sidebar_button = gtk::Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Afficher le panneau")
            .build();
        sidebar_button.add_css_class("header-icon-button");
        sidebar_button.set_size_request(34, 34);
        sidebar_button.set_valign(gtk::Align::Center);
        header.pack_start(&sidebar_button);

        let search_entry = gtk::SearchEntry::new();
        search_entry.add_css_class("app-search");
        search_entry.set_placeholder_text(Some("Rechercher"));
        search_entry.set_hexpand(true);
        search_entry.set_size_request(-1, 34);
        sidebar_header.append(&search_entry);

        let new_thread_header = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Nouveau thread")
            .build();
        new_thread_header.add_css_class("header-icon-button");
        new_thread_header.set_size_request(34, 34);
        new_thread_header.set_valign(gtk::Align::Center);

        let profile_button_label = gtk::Label::new(Some("Codex"));
        let profile_button = gtk::MenuButton::new();
        profile_button.add_css_class("mode-pill");
        profile_button.set_tooltip_text(Some("Profil Codex"));
        profile_button.set_child(Some(&profile_button_label));

        let permission_button_label = gtk::Label::new(Some("Standard"));
        let permission_button = gtk::MenuButton::new();
        permission_button.add_css_class("mode-pill");
        permission_button.set_tooltip_text(Some("Permissions"));
        permission_button.set_child(Some(&permission_button_label));

        let header_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header_actions.add_css_class("header-actions");
        header_actions.append(&profile_button);
        header_actions.append(&permission_button);
        header_actions.append(&new_thread_header);
        header.pack_end(&header_actions);

        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 10);
        sidebar.add_css_class("sidebar-surface");
        sidebar.set_hexpand(false);
        sidebar.set_size_request(SIDEBAR_WIDTH, -1);
        sidebar_shell.set_content(Some(&sidebar));
        sidebar.append(&sidebar_header);

        let sidebar_actions = gtk::Box::new(gtk::Orientation::Vertical, 6);
        sidebar_actions.add_css_class("sidebar-actions");
        sidebar.append(&sidebar_actions);

        let new_thread_sidebar = icon_label_button("list-add-symbolic", "Nouveau thread");
        new_thread_sidebar.add_css_class("sidebar-action");
        new_thread_sidebar.add_css_class("sidebar-action-primary");
        sidebar_actions.append(&new_thread_sidebar);

        let open_workspace_button = icon_label_button("folder-open-symbolic", "Ouvrir un projet");
        open_workspace_button.add_css_class("sidebar-action");
        sidebar_actions.append(&open_workspace_button);

        let sidebar_sections = gtk::Paned::new(gtk::Orientation::Vertical);
        sidebar_sections.add_css_class("sidebar-sections");
        sidebar_sections.set_vexpand(true);
        sidebar_sections.set_wide_handle(true);
        sidebar_sections.set_resize_start_child(true);
        sidebar_sections.set_resize_end_child(true);
        sidebar_sections.set_shrink_start_child(false);
        sidebar_sections.set_shrink_end_child(false);
        sidebar_sections.set_position(SIDEBAR_PROJECTS_INITIAL_HEIGHT);
        sidebar.append(&sidebar_sections);

        let workspace_section = gtk::Box::new(gtk::Orientation::Vertical, 4);
        workspace_section.add_css_class("sidebar-section-pane");
        workspace_section.set_vexpand(true);

        let workspace_header = section_label("Projets");
        workspace_section.append(&workspace_header);

        let workspace_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(120)
            .vexpand(true)
            .build();
        workspace_scroller.add_css_class("sidebar-scroll");

        let workspace_list = gtk::ListBox::new();
        workspace_list.add_css_class("native-list");
        workspace_list.set_selection_mode(gtk::SelectionMode::None);
        workspace_scroller.set_child(Some(&workspace_list));
        workspace_section.append(&workspace_scroller);
        sidebar_sections.set_start_child(Some(&workspace_section));

        let thread_section = gtk::Box::new(gtk::Orientation::Vertical, 4);
        thread_section.add_css_class("sidebar-section-pane");
        thread_section.set_vexpand(true);

        let thread_header = section_label("Threads");
        thread_section.append(&thread_header);

        let thread_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(130)
            .vexpand(true)
            .build();
        thread_scroller.add_css_class("sidebar-scroll");
        let thread_list = gtk::ListBox::new();
        thread_list.add_css_class("native-list");
        thread_list.set_selection_mode(gtk::SelectionMode::None);
        thread_scroller.set_child(Some(&thread_list));
        thread_section.append(&thread_scroller);
        sidebar_sections.set_end_child(Some(&thread_section));

        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.add_css_class("chat-surface");
        content.set_hexpand(true);
        content.set_vexpand(true);
        main_shell.set_content(Some(&content));

        let tab_view = adw::TabView::new();
        let tab_bar = adw::TabBar::new();
        tab_bar.add_css_class("thread-tabbar");
        tab_bar.set_autohide(false);
        tab_bar.set_expand_tabs(true);
        tab_bar.set_view(Some(&tab_view));
        tab_bar.set_visible(false);
        main_shell.add_top_bar(&tab_bar);

        let messages_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .build();
        messages_scroll.add_css_class("messages-scroll");
        let messages_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        messages_box.add_css_class("messages-list");
        messages_scroll.set_child(Some(&messages_box));
        content.append(&messages_scroll);

        let composer_wrap = gtk::Box::new(gtk::Orientation::Vertical, 6);
        composer_wrap.add_css_class("composer-wrap");
        content.append(&composer_wrap);

        let attachment_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        attachment_bar.add_css_class("attachment-bar");
        attachment_bar.set_visible(false);
        composer_wrap.append(&attachment_bar);

        let composer_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        composer_wrap.append(&composer_row);

        let attach_button = gtk::Button::builder()
            .icon_name("mail-attachment-symbolic")
            .tooltip_text("Ajouter une piece jointe")
            .build();
        attach_button.add_css_class("send-button");
        attach_button.set_valign(gtk::Align::End);
        composer_row.append(&attach_button);

        let composer = gtk::TextView::new();
        composer.add_css_class("composer-view");
        composer.set_wrap_mode(gtk::WrapMode::WordChar);
        composer.set_vexpand(false);
        composer.set_monospace(false);
        let composer_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(44)
            .max_content_height(116)
            .build();
        composer_scroll.add_css_class("composer-scroll");
        composer_scroll.set_hexpand(true);
        composer_scroll.set_child(Some(&composer));
        composer_row.append(&composer_scroll);

        let send_button = gtk::Button::builder()
            .icon_name("send-symbolic")
            .tooltip_text("Envoyer")
            .build();
        send_button.add_css_class("send-button");
        send_button.set_valign(gtk::Align::End);
        composer_row.append(&send_button);

        let controller = Rc::new(Self {
            backend,
            ui_tx,
            ui_rx: RefCell::new(ui_rx),
            toast_overlay,
            window,
            title_label,
            subtitle_label,
            profile_button,
            profile_button_label,
            permission_button,
            permission_button_label,
            tab_bar,
            tab_view,
            workspace_list,
            thread_list,
            messages_box,
            messages_scroll,
            attachment_bar,
            composer,
            send_button,
            search_entry,
            workspaces: RefCell::new(Vec::new()),
            threads: RefCell::new(Vec::new()),
            visible_workspace_ids: RefCell::new(Vec::new()),
            visible_thread_ids: RefCell::new(Vec::new()),
            thread_tabs_by_workspace: RefCell::new(HashMap::new()),
            syncing_tab_view: Cell::new(false),
            pending_attachments: RefCell::new(Vec::new()),
            active_workspace_id: RefCell::new(None),
            active_thread_id: RefCell::new(None),
            split_view,
        });

        let weak = Rc::downgrade(&controller);
        sidebar_button.connect_clicked(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.toggle_sidebar();
            }
        });

        let weak = Rc::downgrade(&controller);
        new_thread_header.connect_clicked(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.create_thread();
            }
        });

        let weak = Rc::downgrade(&controller);
        new_thread_sidebar.connect_clicked(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.create_thread();
            }
        });

        let weak = Rc::downgrade(&controller);
        open_workspace_button.connect_clicked(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.open_workspace_dialog();
            }
        });

        let weak = Rc::downgrade(&controller);
        attach_button.connect_clicked(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.open_attachment_dialog();
            }
        });

        let drop_target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let weak = Rc::downgrade(&controller);
        drop_target.connect_drop(move |_, value, _, _| {
            let Ok(file_list) = value.get::<gdk::FileList>() else {
                return false;
            };
            let paths = file_list
                .files()
                .into_iter()
                .filter_map(|file| file.path())
                .collect::<Vec<_>>();
            if paths.is_empty() {
                return false;
            }
            if let Some(controller) = weak.upgrade() {
                controller.add_attachments(paths);
                return true;
            }
            false
        });
        controller.window.add_controller(drop_target);

        let weak = Rc::downgrade(&controller);
        controller
            .workspace_list
            .connect_row_activated(move |_, row| {
                if let Some(controller) = weak.upgrade() {
                    controller.select_workspace_by_index(row.index());
                }
            });

        let weak = Rc::downgrade(&controller);
        controller.thread_list.connect_row_activated(move |_, row| {
            if let Some(controller) = weak.upgrade() {
                controller.select_thread_by_index(row.index());
            }
        });

        let weak = Rc::downgrade(&controller);
        controller
            .tab_view
            .connect_selected_page_notify(move |tab_view| {
                let Some(controller) = weak.upgrade() else {
                    return;
                };
                if controller.syncing_tab_view.get() {
                    return;
                }
                let Some(page) = tab_view.selected_page() else {
                    return;
                };
                let thread_id = page.child().widget_name().to_string();
                if thread_id.is_empty()
                    || controller.active_thread_id.borrow().as_deref() == Some(thread_id.as_str())
                {
                    return;
                }
                let _ = controller.ui_tx.send(UiEvent::SelectThread(thread_id));
            });

        let weak = Rc::downgrade(&controller);
        controller
            .tab_view
            .connect_close_page(move |tab_view, page| {
                let Some(controller) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if controller.syncing_tab_view.get() {
                    tab_view.close_page_finish(page, true);
                    return glib::Propagation::Stop;
                }
                let thread_id = page.child().widget_name().to_string();
                controller.syncing_tab_view.set(true);
                tab_view.close_page_finish(page, true);
                controller.syncing_tab_view.set(false);
                if !thread_id.is_empty() {
                    let _ = controller.ui_tx.send(UiEvent::CloseThreadTab(thread_id));
                }
                glib::Propagation::Stop
            });

        let weak = Rc::downgrade(&controller);
        controller.search_entry.connect_search_changed(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.render_all();
            }
        });

        let weak = Rc::downgrade(&controller);
        controller.send_button.connect_clicked(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.submit_or_cancel();
            }
        });

        let weak = Rc::downgrade(&controller);
        controller.composer.buffer().connect_changed(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.sync_composer_state();
            }
        });

        let key_controller = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&controller);
        key_controller.connect_key_pressed(move |_, key, _, state| {
            if key == gdk::Key::Return && !state.contains(gdk::ModifierType::SHIFT_MASK) {
                if let Some(controller) = weak.upgrade() {
                    controller.submit_or_cancel();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        controller.composer.add_controller(key_controller);

        let window_key_controller = gtk::EventControllerKey::new();
        window_key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&controller);
        window_key_controller.connect_key_pressed(move |_, key, _, state| {
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            if let Some(controller) = weak.upgrade() {
                if ctrl && key == gdk::Key::k {
                    controller.search_entry.grab_focus();
                    return glib::Propagation::Stop;
                }

                if ctrl && key == gdk::Key::n {
                    controller.create_thread();
                    return glib::Propagation::Stop;
                }

                if key == gdk::Key::Escape && controller.search_entry.has_focus() {
                    controller.search_entry.set_text("");
                    controller.composer.grab_focus();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        controller.window.add_controller(window_key_controller);

        let weak = Rc::downgrade(&controller);
        glib::timeout_add_local(Duration::from_millis(80), move || {
            if let Some(controller) = weak.upgrade() {
                controller.drain_ui_events();
                return glib::ControlFlow::Continue;
            }
            glib::ControlFlow::Break
        });

        let weak = Rc::downgrade(&controller);
        glib::timeout_add_local(Duration::from_secs(5), move || {
            if let Some(controller) = weak.upgrade() {
                let _ = controller.ui_tx.send(UiEvent::Reload);
                return glib::ControlFlow::Continue;
            }
            glib::ControlFlow::Break
        });

        let backend = Arc::clone(&controller.backend);
        controller.window.connect_close_request(move |_| {
            backend.shutdown();
            glib::Propagation::Proceed
        });

        controller.load_initial();
        controller.sync_composer_state();
        controller
    }

    fn show(&self) {
        self.window.present();
    }

    fn load_initial(&self) {
        match self.backend.list_workspaces() {
            Ok(workspaces) => {
                let active_id = workspaces.first().map(|workspace| workspace.id.clone());
                *self.workspaces.borrow_mut() = workspaces;
                *self.active_workspace_id.borrow_mut() = active_id;
                self.reload_threads();
                self.render_all();
            }
            Err(error) => self.toast(format!("{error:#}")),
        }
    }

    fn reload_threads(&self) {
        let Some(workspace_id) = self.active_workspace_id.borrow().clone() else {
            self.threads.borrow_mut().clear();
            *self.active_thread_id.borrow_mut() = None;
            return;
        };

        match self.backend.list_threads(&workspace_id) {
            Ok(mut threads) => {
                if let Some(active_thread_id) = self.active_thread_id.borrow().clone() {
                    if !threads.iter().any(|thread| thread.id == active_thread_id) {
                        if let Ok(Some(active_thread)) = self.backend.get_thread(&active_thread_id)
                        {
                            if active_thread.workspace_id == workspace_id {
                                threads.insert(0, active_thread);
                            }
                        }
                    }
                }

                if self.active_thread_id.borrow().is_none() {
                    *self.active_thread_id.borrow_mut() =
                        threads.first().map(|thread| thread.id.clone());
                }
                *self.threads.borrow_mut() = threads;
            }
            Err(error) => self.toast(format!("{error:#}")),
        }
    }

    fn render_all(&self) {
        self.render_workspaces();
        self.render_threads();
        self.render_thread_tabs();
        self.render_messages();
        self.sync_composer_state();
    }

    fn render_workspaces(&self) {
        clear_list_box(&self.workspace_list);
        let active_id = self.active_workspace_id.borrow().clone();
        let query = self.search_entry.text().to_string().to_lowercase();
        let mut visible_ids = Vec::new();

        for (index, workspace) in self.workspaces.borrow().iter().enumerate() {
            if !query.is_empty()
                && !workspace.name.to_lowercase().contains(&query)
                && !workspace.root_path.to_lowercase().contains(&query)
            {
                continue;
            }
            visible_ids.push(workspace.id.clone());
            let row = gtk::ListBoxRow::new();
            row.set_selectable(false);
            row.set_activatable(true);
            row.set_widget_name(&format!("workspace-row-{index}"));
            let content = row_box(
                "folder-symbolic",
                &workspace.name,
                Some(&workspace.root_path),
            );
            content.add_css_class("workspace-row");
            if active_id.as_deref() == Some(workspace.id.as_str()) {
                content.add_css_class("active");
            }
            row.set_child(Some(&content));
            self.workspace_list.append(&row);
        }

        *self.visible_workspace_ids.borrow_mut() = visible_ids;
    }

    fn render_threads(&self) {
        clear_list_box(&self.thread_list);
        let active_id = self.active_thread_id.borrow().clone();
        let query = self.search_entry.text().to_string().to_lowercase();
        let mut visible_ids = Vec::new();

        for thread in self.threads.borrow().iter() {
            if !query.is_empty() && !thread.title.to_lowercase().contains(&query) {
                continue;
            }
            visible_ids.push(thread.id.clone());

            let row = gtk::ListBoxRow::new();
            row.set_selectable(false);
            row.set_activatable(true);
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            content.add_css_class("thread-row");
            if active_id.as_deref() == Some(thread.id.as_str()) {
                content.add_css_class("active");
            }

            let dot = gtk::Box::new(gtk::Orientation::Vertical, 0);
            dot.add_css_class("status-dot");
            dot.add_css_class(status_class(&thread.status));
            dot.set_valign(gtk::Align::Center);
            content.append(&dot);

            let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
            labels.set_hexpand(true);
            let title = gtk::Label::new(Some(thread.title.trim()));
            title.add_css_class("row-title");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.set_max_width_chars(24);
            let subtitle = gtk::Label::new(Some(&format!(
                "{} messages - {}",
                thread.message_count,
                compact_timestamp(&thread.last_activity_at)
            )));
            subtitle.add_css_class("row-subtitle");
            subtitle.set_xalign(0.0);
            subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
            subtitle.set_max_width_chars(28);
            labels.append(&title);
            labels.append(&subtitle);
            content.append(&labels);

            if self.backend.is_running(&thread.id) {
                let badge = gtk::Label::new(Some("Live"));
                badge.add_css_class("row-badge");
                content.append(&badge);
            }

            let middle_click = gtk::GestureClick::new();
            middle_click.set_button(gdk::BUTTON_MIDDLE);
            let ui_tx = self.ui_tx.clone();
            let thread_id = thread.id.clone();
            middle_click.connect_pressed(move |gesture, _, _, _| {
                let _ = gesture.set_state(gtk::EventSequenceState::Claimed);
                let _ = ui_tx.send(UiEvent::OpenThreadTab(thread_id.clone()));
            });
            row.add_controller(middle_click);

            row.set_child(Some(&content));
            self.thread_list.append(&row);
        }

        *self.visible_thread_ids.borrow_mut() = visible_ids;
    }

    fn render_thread_tabs(&self) {
        let Some(workspace_id) = self.active_workspace_id.borrow().clone() else {
            self.syncing_tab_view.set(true);
            self.remove_stale_tab_pages(&[]);
            self.tab_bar.set_visible(false);
            self.syncing_tab_view.set(false);
            return;
        };

        let active_thread_id = self.active_thread_id.borrow().clone();
        let mut tabs = self
            .thread_tabs_by_workspace
            .borrow()
            .get(&workspace_id)
            .cloned()
            .unwrap_or_default();
        tabs.retain(|thread_id| {
            self.threads
                .borrow()
                .iter()
                .any(|thread| thread.id == *thread_id)
                || active_thread_id.as_deref() == Some(thread_id.as_str())
        });
        if let Some(active_id) = active_thread_id.as_ref() {
            if !tabs.iter().any(|thread_id| thread_id == active_id) {
                tabs.push(active_id.clone());
            }
        }
        self.thread_tabs_by_workspace
            .borrow_mut()
            .insert(workspace_id, tabs.clone());

        let show_tabs = tabs.len() > 1;

        self.syncing_tab_view.set(true);
        self.remove_stale_tab_pages(&tabs);
        let mut selected_page = None;
        for (index, thread_id) in tabs.iter().enumerate() {
            let page = self.ensure_tab_page(thread_id, index as i32);
            let title = self.thread_tab_title(thread_id);
            let display_title = compact_title(&title, 36);
            page.set_title(&display_title);
            page.set_tooltip(&title);
            let is_active = active_thread_id.as_deref() == Some(thread_id.as_str());
            if is_active {
                selected_page = Some(page);
            }
        }

        self.tab_bar.set_visible(show_tabs);
        if let Some(page) = selected_page {
            self.tab_view.set_selected_page(&page);
        }
        self.syncing_tab_view.set(false);
    }

    fn remove_stale_tab_pages(&self, desired_thread_ids: &[String]) {
        let mut retained_thread_ids: Vec<String> = Vec::new();
        let mut pages_to_close = Vec::new();

        for index in 0..self.tab_view.n_pages() {
            let page = self.tab_view.nth_page(index);
            let thread_id = page.child().widget_name().to_string();
            let is_desired = desired_thread_ids
                .iter()
                .any(|desired_id| desired_id == &thread_id);
            let is_duplicate = retained_thread_ids
                .iter()
                .any(|retained_id| retained_id == &thread_id);

            if thread_id.is_empty() || !is_desired || is_duplicate {
                pages_to_close.push(page);
            } else {
                retained_thread_ids.push(thread_id);
            }
        }

        for page in pages_to_close {
            self.tab_view.close_page(&page);
        }
    }

    fn ensure_tab_page(&self, thread_id: &str, index: i32) -> adw::TabPage {
        if let Some(page) = self.find_tab_page(thread_id) {
            if self.tab_view.page_position(&page) != index {
                self.tab_view.reorder_page(&page, index);
            }
            return page;
        }

        let host = adw::Bin::new();
        host.set_widget_name(thread_id);
        self.tab_view.insert(&host, index)
    }

    fn find_tab_page(&self, thread_id: &str) -> Option<adw::TabPage> {
        for index in 0..self.tab_view.n_pages() {
            let page = self.tab_view.nth_page(index);
            if page.child().widget_name().as_str() == thread_id {
                return Some(page);
            }
        }
        None
    }

    fn thread_tab_title(&self, thread_id: &str) -> String {
        let title = self
            .threads
            .borrow()
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| thread.title.clone())
            .or_else(|| {
                self.backend
                    .get_thread(thread_id)
                    .ok()
                    .flatten()
                    .map(|thread| thread.title)
            })
            .unwrap_or_else(|| "Thread".to_string());
        let title = title.trim();
        if title.is_empty() {
            "Thread".to_string()
        } else {
            title.to_string()
        }
    }

    fn render_messages(&self) {
        clear_box(&self.messages_box);
        let Some(thread_id) = self.active_thread_id.borrow().clone() else {
            self.render_empty("Aucun thread selectionne");
            self.title_label.set_text("SupaCodex");
            self.title_label.set_tooltip_text(None);
            self.subtitle_label.set_text("codex");
            self.render_runtime_controls(None);
            return;
        };

        let thread = match self.backend.get_thread(&thread_id) {
            Ok(Some(thread)) => thread,
            Ok(None) => {
                self.render_empty("Thread introuvable");
                return;
            }
            Err(error) => {
                self.render_empty(&format!("{error:#}"));
                return;
            }
        };

        let header_title = thread.title.trim();
        let header_title = if header_title.is_empty() {
            "Thread"
        } else {
            header_title
        };
        self.title_label.set_text(header_title);
        self.title_label.set_tooltip_text(Some(header_title));
        self.subtitle_label.set_text(&format!(
            "{} - {} - {} tokens",
            thread.engine_id, thread.model_id, thread.total_tokens
        ));
        self.render_runtime_controls(Some(&thread));
        self.send_button
            .set_icon_name(if self.backend.is_running(&thread.id) {
                "process-stop-symbolic"
            } else {
                "send-symbolic"
            });

        match self.backend.get_messages(&thread_id) {
            Ok(messages) if messages.is_empty() => {
                self.render_empty("Pret a demarrer une conversation.");
            }
            Ok(messages) => {
                for message in messages {
                    self.render_message(&thread, &message);
                }
                let scroll = self.messages_scroll.clone();
                glib::idle_add_local_once(move || {
                    let adjustment = scroll.vadjustment();
                    adjustment.set_value(adjustment.upper());
                });
            }
            Err(error) => self.render_empty(&format!("{error:#}")),
        }
    }

    fn render_empty(&self, text: &str) {
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 10);
        empty.add_css_class("empty-state");
        empty.set_halign(gtk::Align::Center);
        empty.set_valign(gtk::Align::Start);
        let icon = gtk::Image::from_icon_name("dialog-information-symbolic");
        icon.set_pixel_size(42);
        let label = gtk::Label::new(Some(text));
        label.add_css_class("dim-label");
        label.set_wrap(true);
        empty.append(&icon);
        empty.append(&label);
        self.messages_box.append(&empty);
    }

    fn render_runtime_controls(&self, thread: Option<&ThreadDto>) {
        let active_profile_id = self.backend.active_codex_profile_id();
        let profiles = self.backend.codex_profiles();
        let active_profile_label = profiles
            .iter()
            .find(|profile| profile.id == active_profile_id)
            .map(|profile| display_codex_profile_name(profile))
            .unwrap_or_else(|| "Codex".to_string());
        self.profile_button_label.set_text(&active_profile_label);
        self.profile_button.set_popover(Some(
            &self.build_profile_popover(&profiles, &active_profile_id),
        ));

        let workspace_id = thread
            .map(|thread| thread.workspace_id.clone())
            .or_else(|| self.active_workspace_id.borrow().clone());
        let trust_level = workspace_id
            .as_deref()
            .map(|workspace_id| self.backend.workspace_trust_level(workspace_id))
            .unwrap_or(TrustLevelDto::Standard);
        self.permission_button_label
            .set_text(trust_level_label(&trust_level));
        self.permission_button.set_popover(Some(
            &self.build_permission_popover(workspace_id, &trust_level),
        ));
    }

    fn build_profile_popover(
        &self,
        profiles: &[CodexProfileConfig],
        active_profile_id: &str,
    ) -> gtk::Popover {
        let popover = gtk::Popover::new();
        popover.add_css_class("runtime-popover");
        let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        for profile in profiles {
            let label = display_codex_profile_name(profile);
            let button = gtk::Button::with_label(&label);
            button.add_css_class("runtime-option");
            if profile.id == active_profile_id {
                button.add_css_class("active");
            }
            button.set_tooltip_text(Some(&profile.codex_home));
            let ui_tx = self.ui_tx.clone();
            let profile_id = profile.id.clone();
            button.connect_clicked(move |_| {
                let _ = ui_tx.send(UiEvent::SetCodexProfile(profile_id.clone()));
            });
            list.append(&button);
        }
        popover.set_child(Some(&list));
        popover
    }

    fn build_permission_popover(
        &self,
        workspace_id: Option<String>,
        active: &TrustLevelDto,
    ) -> gtk::Popover {
        let popover = gtk::Popover::new();
        popover.add_css_class("runtime-popover");
        let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        for level in [
            TrustLevelDto::Restricted,
            TrustLevelDto::Standard,
            TrustLevelDto::Trusted,
        ] {
            let button = gtk::Button::with_label(trust_level_label(&level));
            button.add_css_class("runtime-option");
            if &level == active {
                button.add_css_class("active");
            }
            button.set_tooltip_text(Some(trust_level_description(&level)));
            let ui_tx = self.ui_tx.clone();
            let level_to_set = level.clone();
            let has_workspace = workspace_id.is_some();
            button.set_sensitive(has_workspace);
            button.connect_clicked(move |_| {
                let _ = ui_tx.send(UiEvent::SetWorkspaceTrust(level_to_set.clone()));
            });
            list.append(&button);
        }
        popover.set_child(Some(&list));
        popover
    }

    fn render_message(&self, thread: &ThreadDto, message: &MessageDto) {
        let is_user = message.role == "user";
        let blocks = parse_blocks(message);
        let has_visible_blocks = blocks.iter().any(block_has_visible_content);
        let fallback_text = message.content.as_deref().unwrap_or("").trim().to_string();
        let empty_status_text = if has_visible_blocks || !fallback_text.is_empty() {
            None
        } else {
            match message.status {
                MessageStatusDto::Streaming => Some("Generation en cours..."),
                MessageStatusDto::Interrupted => Some("Generation interrompue."),
                MessageStatusDto::Error => Some("Erreur sans detail."),
                MessageStatusDto::Completed => None,
            }
        };
        if empty_status_text.is_none() && !has_visible_blocks && fallback_text.is_empty() {
            return;
        }

        let outer = gtk::Box::new(gtk::Orientation::Vertical, 3);
        outer.set_hexpand(true);
        outer.set_margin_start(if is_user { 160 } else { 0 });
        outer.set_margin_end(if is_user { 0 } else { 160 });
        outer.set_halign(if is_user {
            gtk::Align::End
        } else {
            gtk::Align::Start
        });

        let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
        card.add_css_class("message-card");
        if is_user {
            card.add_css_class("user-message");
        } else {
            card.add_css_class("assistant-message");
        }
        outer.append(&card);

        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toolbar.add_css_class("message-toolbar");
        let author = gtk::Label::new(Some(if is_user { "Vous" } else { &thread.engine_id }));
        author.add_css_class("message-author");
        author.set_xalign(0.0);
        author.set_hexpand(true);
        toolbar.append(&author);
        if is_user {
            let edit_button = gtk::Button::builder()
                .icon_name("document-edit-symbolic")
                .tooltip_text("Modifier et reprendre depuis ce message")
                .build();
            edit_button.add_css_class("message-edit-button");
            edit_button.set_has_frame(false);
            let ui_tx = self.ui_tx.clone();
            let message_id = message.id.clone();
            edit_button.connect_clicked(move |_| {
                let _ = ui_tx.send(UiEvent::EditMessage(message_id.clone()));
            });
            toolbar.append(&edit_button);
        }
        card.append(&toolbar);

        if let Some(status_text) = empty_status_text {
            let pending = gtk::Label::new(Some(status_text));
            pending.add_css_class("dim-label");
            pending.set_xalign(0.0);
            card.append(&pending);
        } else if !has_visible_blocks {
            card.append(&message_label(&fallback_text));
        } else {
            for block in blocks {
                self.render_block(thread, &card, block);
            }
        }

        self.messages_box.append(&outer);
    }

    fn render_block(&self, thread: &ThreadDto, parent: &gtk::Box, block: NativeContentBlock) {
        match block {
            NativeContentBlock::Text { content, .. } | NativeContentBlock::Thinking { content } => {
                if !content.trim().is_empty() {
                    parent.append(&message_label(&content));
                }
            }
            NativeContentBlock::Error { message } => {
                let card = block_card("Erreur", Some(&message));
                parent.append(&card);
            }
            NativeContentBlock::Notice { title, message, .. } => {
                let card = block_card(&title, Some(&message));
                parent.append(&card);
            }
            NativeContentBlock::Diff { diff, scope } => {
                let title = format!("Diff {scope}");
                let card = code_card(&title, &diff);
                parent.append(&card);
            }
            NativeContentBlock::Action {
                summary,
                action_type,
                output_chunks,
                result,
                ..
            } => {
                let title = format!("{action_type}: {summary}");
                let mut body = output_chunks
                    .iter()
                    .map(|chunk| chunk.content.as_str())
                    .collect::<Vec<_>>()
                    .join("");
                if body.trim().is_empty() {
                    if let Some(result) = result {
                        body = result
                            .output
                            .or(result.error)
                            .or(result.diff)
                            .unwrap_or_else(|| "Termine".to_string());
                    }
                }
                parent.append(&code_card(&title, &body));
            }
            NativeContentBlock::Approval {
                approval_id,
                action_type,
                summary,
                details,
                status,
                ..
            } => {
                let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
                card.add_css_class("block-card");
                let title = gtk::Label::new(Some(&format!("{action_type}: {summary}")));
                title.add_css_class("block-title");
                title.set_xalign(0.0);
                title.set_wrap(true);
                card.append(&title);

                let detail = gtk::Label::new(Some(&format!("Approval {status}")));
                detail.add_css_class("dim-label");
                detail.set_xalign(0.0);
                card.append(&detail);

                if status == "pending" {
                    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    actions.add_css_class("approval-actions");
                    let accept = gtk::Button::with_label("Accepter");
                    let decline = gtk::Button::with_label("Refuser");
                    let thread_id = thread.id.clone();
                    let accept_approval_id = approval_id.clone();
                    let accept_details = details.clone();
                    let backend = Arc::clone(&self.backend);
                    let ui_tx = self.ui_tx.clone();
                    accept.connect_clicked(move |_| {
                        backend.respond_to_approval(
                            thread_id.clone(),
                            accept_approval_id.clone(),
                            accept_details.clone(),
                            "accept",
                            ui_tx.clone(),
                        );
                    });

                    let thread_id = thread.id.clone();
                    let decline_approval_id = approval_id;
                    let decline_details = details;
                    let backend = Arc::clone(&self.backend);
                    let ui_tx = self.ui_tx.clone();
                    decline.connect_clicked(move |_| {
                        backend.respond_to_approval(
                            thread_id.clone(),
                            decline_approval_id.clone(),
                            decline_details.clone(),
                            "decline",
                            ui_tx.clone(),
                        );
                    });
                    actions.append(&accept);
                    actions.append(&decline);
                    card.append(&actions);
                }
                parent.append(&card);
            }
            NativeContentBlock::Attachment {
                file_name,
                file_path,
                ..
            }
            | NativeContentBlock::Skill {
                name: file_name,
                path: file_path,
            }
            | NativeContentBlock::Mention {
                name: file_name,
                path: file_path,
            } => {
                if is_image_path(&file_path) {
                    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
                    card.add_css_class("block-card");
                    let title = gtk::Label::new(Some(&file_name));
                    title.add_css_class("block-title");
                    title.set_xalign(0.0);
                    card.append(&title);
                    let file = gio::File::for_path(&file_path);
                    let picture = gtk::Picture::for_file(&file);
                    picture.set_content_fit(gtk::ContentFit::Contain);
                    picture.set_size_request(220, 160);
                    card.append(&picture);
                    parent.append(&card);
                } else {
                    let card = block_card(&file_name, Some(&file_path));
                    parent.append(&card);
                }
            }
        }
    }

    fn submit_or_cancel(&self) {
        let Some(thread_id) = self.ensure_thread() else {
            return;
        };

        if self.backend.is_running(&thread_id) {
            self.backend.cancel_turn(thread_id, self.ui_tx.clone());
            return;
        }

        let buffer = self.composer.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        let message = text.trim().to_string();
        let attachments = self
            .pending_attachments
            .borrow()
            .iter()
            .map(PendingAttachment::to_turn_attachment)
            .collect::<Vec<_>>();
        if message.is_empty() && attachments.is_empty() {
            return;
        }
        buffer.set_text("");
        self.pending_attachments.borrow_mut().clear();
        self.render_attachment_bar();
        self.composer.grab_focus();
        self.backend
            .send_message(thread_id, message, attachments, self.ui_tx.clone());
    }

    fn ensure_thread(&self) -> Option<String> {
        if let Some(thread_id) = self.active_thread_id.borrow().clone() {
            return Some(thread_id);
        }
        self.create_thread()
    }

    fn create_thread(&self) -> Option<String> {
        let workspace_id = self.active_workspace_id.borrow().clone().or_else(|| {
            self.workspaces
                .borrow()
                .first()
                .map(|workspace| workspace.id.clone())
        })?;

        match self.backend.create_thread(&workspace_id) {
            Ok(thread) => {
                *self.active_thread_id.borrow_mut() = Some(thread.id.clone());
                self.reload_threads();
                self.render_all();
                self.composer.grab_focus();
                Some(thread.id)
            }
            Err(error) => {
                self.toast(format!("{error:#}"));
                None
            }
        }
    }

    fn select_workspace_by_index(&self, index: i32) {
        let Some(workspace_id) = self
            .visible_workspace_ids
            .borrow()
            .get(index.max(0) as usize)
            .cloned()
        else {
            return;
        };
        *self.active_workspace_id.borrow_mut() = Some(workspace_id);
        *self.active_thread_id.borrow_mut() = None;
        self.reload_threads();
        self.render_all();
    }

    fn select_thread_by_index(&self, index: i32) {
        let Some(thread_id) = self
            .visible_thread_ids
            .borrow()
            .get(index.max(0) as usize)
            .cloned()
        else {
            return;
        };
        *self.active_thread_id.borrow_mut() = Some(thread_id);
        self.render_all();
        self.composer.grab_focus();
    }

    fn open_workspace_dialog(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Ouvrir un projet")
            .modal(true)
            .build();
        let weak = Rc::downgrade(self);
        dialog.select_folder(
            Some(&self.window),
            None::<&gio::Cancellable>,
            move |result| {
                if let Ok(file) = result {
                    if let Some(controller) = weak.upgrade() {
                        if let Some(path) = file.path() {
                            controller.open_workspace_path(&path);
                        }
                    }
                }
            },
        );
    }

    fn open_attachment_dialog(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Ajouter des pieces jointes")
            .modal(true)
            .build();
        let weak = Rc::downgrade(self);
        dialog.open_multiple(
            Some(&self.window),
            None::<&gio::Cancellable>,
            move |result| {
                let Ok(files) = result else {
                    return;
                };
                let mut paths = Vec::new();
                for index in 0..files.n_items() {
                    let Some(item) = files.item(index) else {
                        continue;
                    };
                    if let Ok(file) = item.downcast::<gio::File>() {
                        if let Some(path) = file.path() {
                            paths.push(path);
                        }
                    }
                }
                if let Some(controller) = weak.upgrade() {
                    controller.add_attachments(paths);
                }
            },
        );
    }

    fn add_attachments(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let mut attachments = self.pending_attachments.borrow_mut();
        for path in paths {
            let path_string = path.to_string_lossy().to_string();
            if attachments
                .iter()
                .any(|attachment| attachment.file_path == path_string)
            {
                continue;
            }
            let Ok(metadata) = fs::metadata(&path) else {
                self.toast(format!("Fichier introuvable: {path_string}"));
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("fichier")
                .to_string();
            attachments.push(PendingAttachment {
                mime_type: guess_mime_type(&path),
                file_name,
                file_path: path_string,
                size_bytes: metadata.len(),
            });
        }
        drop(attachments);
        self.render_attachment_bar();
        self.sync_composer_state();
    }

    fn render_attachment_bar(&self) {
        clear_box(&self.attachment_bar);
        let attachments = self.pending_attachments.borrow().clone();
        self.attachment_bar.set_visible(!attachments.is_empty());

        for (index, attachment) in attachments.iter().enumerate() {
            let chip = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            chip.add_css_class("attachment-chip");

            if is_image_attachment(attachment) {
                let file = gio::File::for_path(&attachment.file_path);
                let picture = gtk::Picture::for_file(&file);
                picture.add_css_class("attachment-thumb");
                picture.set_size_request(28, 28);
                picture.set_content_fit(gtk::ContentFit::Cover);
                chip.append(&picture);
            } else {
                let icon = gtk::Image::from_icon_name("text-x-generic-symbolic");
                icon.set_pixel_size(16);
                chip.append(&icon);
            }

            let labels = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let title = gtk::Label::new(Some(&attachment.file_name));
            title.add_css_class("row-title");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.set_max_width_chars(22);
            let subtitle = gtk::Label::new(Some(&format_size(attachment.size_bytes)));
            subtitle.add_css_class("row-subtitle");
            subtitle.set_xalign(0.0);
            labels.append(&title);
            labels.append(&subtitle);
            chip.append(&labels);

            let remove = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .tooltip_text("Retirer")
                .build();
            let ui_tx = self.ui_tx.clone();
            remove.connect_clicked(move |_| {
                let _ = ui_tx.send(UiEvent::RemoveAttachment(index));
            });
            chip.append(&remove);
            self.attachment_bar.append(&chip);
        }
    }

    fn open_workspace_path(&self, path: &Path) {
        match self.backend.open_workspace(&path.to_string_lossy()) {
            Ok(workspace) => {
                if let Ok(workspaces) = self.backend.list_workspaces() {
                    *self.workspaces.borrow_mut() = workspaces;
                }
                *self.active_workspace_id.borrow_mut() = Some(workspace.id);
                *self.active_thread_id.borrow_mut() = None;
                self.reload_threads();
                self.render_all();
            }
            Err(error) => self.toast(format!("{error:#}")),
        }
    }

    fn toggle_sidebar(&self) {
        self.split_view
            .set_show_sidebar(!self.split_view.shows_sidebar());
    }

    fn sync_composer_state(&self) {
        let active_thread_id = self.active_thread_id.borrow().clone();
        let running = active_thread_id
            .as_ref()
            .is_some_and(|thread_id| self.backend.is_running(thread_id));
        let buffer = self.composer.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        let has_text = !text.trim().is_empty();
        let has_attachments = !self.pending_attachments.borrow().is_empty();
        let has_context = active_thread_id.is_some()
            || self.active_workspace_id.borrow().is_some()
            || !self.workspaces.borrow().is_empty();

        self.send_button.set_icon_name(if running {
            "process-stop-symbolic"
        } else {
            "send-symbolic"
        });
        self.send_button.set_tooltip_text(Some(if running {
            "Annuler la generation"
        } else {
            "Envoyer"
        }));
        self.send_button
            .set_sensitive(running || (has_context && (has_text || has_attachments)));
    }

    fn close_thread_tab(&self, thread_id: &str) {
        let Some(workspace_id) = self.active_workspace_id.borrow().clone() else {
            return;
        };
        let mut tabs_by_workspace = self.thread_tabs_by_workspace.borrow_mut();
        let tabs = tabs_by_workspace.entry(workspace_id).or_default();
        let removed_index = tabs.iter().position(|candidate| candidate == thread_id);
        tabs.retain(|candidate| candidate != thread_id);

        if self.active_thread_id.borrow().as_deref() == Some(thread_id) {
            let next_thread_id = removed_index
                .and_then(|index| {
                    tabs.get(index)
                        .or_else(|| tabs.get(index.saturating_sub(1)))
                })
                .cloned()
                .or_else(|| {
                    self.threads
                        .borrow()
                        .iter()
                        .find(|thread| thread.id != thread_id)
                        .map(|thread| thread.id.clone())
                });
            *self.active_thread_id.borrow_mut() = next_thread_id;
        }
        drop(tabs_by_workspace);
        self.render_all();
    }

    fn open_thread_tab(&self, thread_id: &str) {
        let Some(workspace_id) = self.active_workspace_id.borrow().clone() else {
            return;
        };
        let active_thread_id = self.active_thread_id.borrow().clone();
        let mut tabs_by_workspace = self.thread_tabs_by_workspace.borrow_mut();
        let tabs = tabs_by_workspace.entry(workspace_id).or_default();

        if let Some(active_thread_id) = active_thread_id {
            if !tabs.iter().any(|candidate| candidate == &active_thread_id) {
                tabs.push(active_thread_id);
            }
        }

        if !tabs.iter().any(|candidate| candidate == thread_id) {
            tabs.push(thread_id.to_string());
        }

        drop(tabs_by_workspace);
        self.render_thread_tabs();
    }

    fn show_edit_message_dialog(&self, message_id: &str) {
        let Some(thread_id) = self.active_thread_id.borrow().clone() else {
            return;
        };
        let message = self
            .backend
            .get_messages(&thread_id)
            .ok()
            .and_then(|messages| {
                messages
                    .into_iter()
                    .find(|message| message.id == message_id && message.role == "user")
            });
        let Some(message) = message else {
            self.toast("Message introuvable.".to_string());
            return;
        };

        let editor = gtk::TextView::new();
        editor.add_css_class("composer-view");
        editor.set_wrap_mode(gtk::WrapMode::WordChar);
        editor
            .buffer()
            .set_text(&message_plain_text(&message).unwrap_or_default());
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(160)
            .max_content_height(280)
            .width_request(520)
            .build();
        scroller.add_css_class("composer-scroll");
        scroller.set_child(Some(&editor));

        let dialog = adw::AlertDialog::builder()
            .heading("Modifier le message")
            .body("La conversation sera reprise depuis ce message. Les reponses suivantes seront remplacees.")
            .extra_child(&scroller)
            .close_response("cancel")
            .default_response("resume")
            .build();
        dialog.add_response("cancel", "Annuler");
        dialog.add_response("resume", "Reprendre");
        dialog.set_response_appearance("resume", adw::ResponseAppearance::Suggested);

        let backend = Arc::clone(&self.backend);
        let ui_tx = self.ui_tx.clone();
        let thread_id_for_edit = thread_id;
        let message_id_for_edit = message.id.clone();
        let editor_for_response = editor.clone();
        dialog.connect_response(None, move |_, response| {
            if response != "resume" {
                return;
            }
            let buffer = editor_for_response.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .trim()
                .to_string();
            backend.edit_and_resume(
                thread_id_for_edit.clone(),
                message_id_for_edit.clone(),
                text,
                ui_tx.clone(),
            );
        });
        dialog.present(Some(&self.window));
        editor.grab_focus();
    }

    fn drain_ui_events(&self) {
        while let Ok(event) = self.ui_rx.borrow_mut().try_recv() {
            match event {
                UiEvent::Reload => {
                    if let Ok(workspaces) = self.backend.list_workspaces() {
                        *self.workspaces.borrow_mut() = workspaces;
                    }
                    self.reload_threads();
                    self.render_all();
                }
                UiEvent::SelectThread(thread_id) => {
                    *self.active_thread_id.borrow_mut() = Some(thread_id);
                    self.reload_threads();
                    self.render_all();
                }
                UiEvent::OpenThreadTab(thread_id) => {
                    self.open_thread_tab(&thread_id);
                }
                UiEvent::CloseThreadTab(thread_id) => {
                    self.close_thread_tab(&thread_id);
                }
                UiEvent::EditMessage(message_id) => {
                    self.show_edit_message_dialog(&message_id);
                }
                UiEvent::SetCodexProfile(profile_id) => {
                    match self.backend.set_active_codex_profile(&profile_id) {
                        Ok(()) => self.render_all(),
                        Err(error) => self.toast(format!("{error:#}")),
                    }
                }
                UiEvent::SetWorkspaceTrust(trust_level) => {
                    let workspace_id = self.active_workspace_id.borrow().clone().or_else(|| {
                        self.threads
                            .borrow()
                            .first()
                            .map(|thread| thread.workspace_id.clone())
                    });
                    match workspace_id {
                        Some(workspace_id) => {
                            match self
                                .backend
                                .set_workspace_trust_level(&workspace_id, trust_level)
                            {
                                Ok(()) => self.render_all(),
                                Err(error) => self.toast(format!("{error:#}")),
                            }
                        }
                        None => self.toast("Aucun projet actif.".to_string()),
                    }
                }
                UiEvent::RemoveAttachment(index) => {
                    let mut attachments = self.pending_attachments.borrow_mut();
                    if index < attachments.len() {
                        attachments.remove(index);
                    }
                    drop(attachments);
                    self.render_attachment_bar();
                    self.sync_composer_state();
                }
                UiEvent::Toast(message) => self.toast(message),
            }
        }
    }

    fn toast(&self, message: String) {
        self.toast_overlay.add_toast(adw::Toast::new(&message));
    }
}

pub fn run() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        install_css();
    });
    app.connect_activate(|app| match NativeBackend::new() {
        Ok(backend) => {
            let controller = AppController::new(app, backend);
            controller.show();
            unsafe {
                app.set_data("supacodex-controller", controller);
            }
        }
        Err(error) => {
            let dialog = adw::AlertDialog::builder()
                .heading("SupaCodex")
                .body(&format!("Impossible de demarrer l'application:\n{error:#}"))
                .build();
            let window = adw::ApplicationWindow::builder()
                .application(app)
                .title("SupaCodex")
                .default_width(520)
                .default_height(220)
                .build();
            dialog.present(Some(&window));
            window.present();
        }
    });

    app.run();
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(STYLE);

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let composer_provider = gtk::CssProvider::new();
        composer_provider.load_from_string(COMPOSER_TRANSPARENCY_STYLE);
        gtk::style_context_add_provider_for_display(
            &display,
            &composer_provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
    }
}

fn icon_label_button(icon_name: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_hexpand(true);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    content.add_css_class("sidebar-action-content");
    content.set_hexpand(true);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("sidebar-action-icon");
    icon.set_pixel_size(16);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);

    let text = gtk::Label::new(Some(label));
    text.add_css_class("sidebar-action-label");
    text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.set_xalign(0.0);
    text.set_hexpand(true);

    content.append(&icon);
    content.append(&text);
    button.set_child(Some(&content));
    button
}

fn section_label(label: &str) -> gtk::Label {
    let widget = gtk::Label::new(Some(label));
    widget.add_css_class("sidebar-section");
    widget.set_xalign(0.0);
    widget
}

fn display_codex_profile_name(profile: &CodexProfileConfig) -> String {
    let name = profile.name.trim();
    let raw = if name.is_empty() { &profile.id } else { name };
    raw.trim_start_matches('.')
        .replace("codex", "Codex")
        .replace("CODEX", "Codex")
}

fn compact_title(title: &str, max_chars: usize) -> String {
    let trimmed = title.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let keep = max_chars - 3;
    let mut shortened = trimmed.chars().take(keep).collect::<String>();
    shortened.push_str("...");
    shortened
}

#[cfg(test)]
mod tests {
    use super::compact_title;

    #[test]
    fn compact_title_keeps_short_title() {
        assert_eq!(compact_title("  Short thread  ", 20), "Short thread");
    }

    #[test]
    fn compact_title_truncates_long_title() {
        assert_eq!(
            compact_title("abcdefghijklmnopqrstuvwxyz", 10),
            "abcdefg..."
        );
    }

    #[test]
    fn compact_title_does_not_split_unicode() {
        assert_eq!(
            compact_title("\u{e9}conomie internationale", 9),
            "\u{e9}conom..."
        );
    }
}

fn trust_level_label(level: &TrustLevelDto) -> &'static str {
    match level {
        TrustLevelDto::Trusted => "Trusted",
        TrustLevelDto::Standard => "Standard",
        TrustLevelDto::Restricted => "Restricted",
    }
}

fn trust_level_description(level: &TrustLevelDto) -> &'static str {
    match level {
        TrustLevelDto::Trusted => "Moins de confirmations, reseau autorise.",
        TrustLevelDto::Standard => "Confirmations sur demande pour les changements sensibles.",
        TrustLevelDto::Restricted => "Mode prudent pour les projets non verifies.",
    }
}

fn guess_mime_type(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_lowercase())?;
    let mime = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "txt" | "md" | "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "css" | "html" | "sh" => {
            "text/plain"
        }
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "xml" => "application/xml",
        "csv" => "text/csv",
        _ => return None,
    };
    Some(mime.to_string())
}

fn is_image_attachment(attachment: &PendingAttachment) -> bool {
    attachment
        .mime_type
        .as_deref()
        .is_some_and(|mime| mime.starts_with("image/"))
        || is_image_path(&attachment.file_path)
}

fn is_image_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|extension| {
            matches!(
                extension.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "svg"
            )
        })
        .unwrap_or(false)
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} o");
    }
    let kib = bytes as f64 / 1024.0;
    if kib < 1024.0 {
        return format!("{kib:.1} Ko");
    }
    let mib = kib / 1024.0;
    format!("{mib:.1} Mo")
}

fn row_box(icon_name: &str, title: &str, subtitle: Option<&str>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    row.append(&icon);

    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("row-title");
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title_label.set_max_width_chars(24);
    labels.append(&title_label);
    if let Some(subtitle) = subtitle {
        let subtitle_label = gtk::Label::new(Some(subtitle));
        subtitle_label.add_css_class("row-subtitle");
        subtitle_label.set_xalign(0.0);
        subtitle_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        subtitle_label.set_max_width_chars(28);
        labels.append(&subtitle_label);
    }
    row.append(&labels);
    row
}

fn message_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text.trim()));
    label.add_css_class("message-text");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_max_width_chars(88);
    label.set_selectable(true);
    label
}

fn block_card(title: &str, body: Option<&str>) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("block-card");
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("block-title");
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.set_max_width_chars(88);
    card.append(&title_label);
    if let Some(body) = body.filter(|value| !value.trim().is_empty()) {
        let body_label = gtk::Label::new(Some(body.trim()));
        body_label.add_css_class("dim-label");
        body_label.set_xalign(0.0);
        body_label.set_wrap(true);
        body_label.set_max_width_chars(88);
        body_label.set_selectable(true);
        card.append(&body_label);
    }
    card
}

fn code_card(title: &str, body: &str) -> gtk::Box {
    let card = block_card(title, None);
    if !body.trim().is_empty() {
        let output = gtk::Label::new(Some(&truncate_display(body.trim(), 12000)));
        output.add_css_class("code-output");
        output.set_xalign(0.0);
        output.set_wrap(true);
        output.set_max_width_chars(96);
        output.set_selectable(true);
        card.append(&output);
    }
    card
}

fn clear_list_box(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn parse_blocks(message: &MessageDto) -> Vec<NativeContentBlock> {
    message
        .blocks
        .as_ref()
        .and_then(|blocks| serde_json::from_value::<Vec<NativeContentBlock>>(blocks.clone()).ok())
        .unwrap_or_default()
}

fn block_has_visible_content(block: &NativeContentBlock) -> bool {
    match block {
        NativeContentBlock::Text { content, .. } | NativeContentBlock::Thinking { content } => {
            !content.trim().is_empty()
        }
        NativeContentBlock::Diff { diff, scope } => {
            !diff.trim().is_empty() || !scope.trim().is_empty()
        }
        NativeContentBlock::Action {
            action_type,
            summary,
            output_chunks,
            result,
            ..
        } => {
            !action_type.trim().is_empty()
                || !summary.trim().is_empty()
                || output_chunks
                    .iter()
                    .any(|chunk| !chunk.content.trim().is_empty())
                || result.as_ref().is_some_and(|result| {
                    result
                        .output
                        .as_deref()
                        .or(result.error.as_deref())
                        .or(result.diff.as_deref())
                        .is_some_and(|value| !value.trim().is_empty())
                })
        }
        NativeContentBlock::Approval {
            action_type,
            summary,
            status,
            ..
        } => {
            !action_type.trim().is_empty()
                || !summary.trim().is_empty()
                || !status.trim().is_empty()
        }
        NativeContentBlock::Notice { title, message, .. } => {
            !title.trim().is_empty() || !message.trim().is_empty()
        }
        NativeContentBlock::Error { message } => !message.trim().is_empty(),
        NativeContentBlock::Attachment {
            file_name,
            file_path,
            ..
        }
        | NativeContentBlock::Skill {
            name: file_name,
            path: file_path,
        }
        | NativeContentBlock::Mention {
            name: file_name,
            path: file_path,
        } => !file_name.trim().is_empty() || !file_path.trim().is_empty(),
    }
}

fn message_plain_text(message: &MessageDto) -> Option<String> {
    let blocks = parse_blocks(message);
    let text = blocks
        .into_iter()
        .filter_map(|block| match block {
            NativeContentBlock::Text { content, .. } => Some(content),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string();
    if !text.is_empty() {
        return Some(text);
    }
    message
        .content
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn append_text_block(blocks: &mut Vec<NativeContentBlock>, content: &str) {
    match blocks.last_mut() {
        Some(NativeContentBlock::Text {
            content: existing, ..
        }) => existing.push_str(content),
        _ => blocks.push(NativeContentBlock::Text {
            content: content.to_string(),
            plan_mode: None,
            is_steer: None,
        }),
    }
}

fn append_thinking_block(blocks: &mut Vec<NativeContentBlock>, content: &str) {
    match blocks.last_mut() {
        Some(NativeContentBlock::Thinking { content: existing }) => existing.push_str(content),
        _ => blocks.push(NativeContentBlock::Thinking {
            content: content.to_string(),
        }),
    }
}

fn stream_label(stream: &OutputStream) -> &'static str {
    match stream {
        OutputStream::Stdout => "stdout",
        OutputStream::Stderr => "stderr",
        OutputStream::Stdin => "stdin",
    }
}

fn aggregate_workspace_trust_level(repos: &[crate::models::RepoDto]) -> TrustLevelDto {
    if repos
        .iter()
        .any(|repo| matches!(repo.trust_level, TrustLevelDto::Restricted))
    {
        return TrustLevelDto::Restricted;
    }

    if !repos.is_empty()
        && repos
            .iter()
            .all(|repo| matches!(repo.trust_level, TrustLevelDto::Trusted))
    {
        return TrustLevelDto::Trusted;
    }

    TrustLevelDto::Standard
}

fn approval_policy_for_engine_and_trust_level(
    engine_id: &str,
    trust_level: &TrustLevelDto,
) -> &'static str {
    match engine_id {
        "claude" => match trust_level {
            TrustLevelDto::Trusted => "trusted",
            TrustLevelDto::Standard => "standard",
            TrustLevelDto::Restricted => "restricted",
        },
        _ => match trust_level {
            TrustLevelDto::Trusted | TrustLevelDto::Standard => "on-request",
            TrustLevelDto::Restricted => "untrusted",
        },
    }
}

fn thread_reasoning_effort(metadata: Option<&Value>) -> Option<String> {
    metadata
        .and_then(|value| value.get("reasoningEffort"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn thread_last_model_id(metadata: Option<&Value>) -> Option<String> {
    metadata
        .and_then(|value| value.get("lastModelId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn codex_transcript_sync_needed(thread: &ThreadDto) -> bool {
    if thread.message_count == 0 {
        return true;
    }

    let Some(metadata) = thread.engine_metadata.as_ref() else {
        return false;
    };
    let remote_updated_at = metadata
        .get("codexRemoteUpdatedAt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let synced_remote_updated_at = metadata
        .get("codexTranscriptSyncedRemoteUpdatedAt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    remote_updated_at.is_some() && remote_updated_at != synced_remote_updated_at
}

fn transcript_message_timestamp(thread_created_at: Option<i64>, index: usize) -> String {
    let base = thread_created_at
        .and_then(|timestamp| DateTime::<Utc>::from_timestamp(timestamp, 0))
        .unwrap_or_else(Utc::now);
    (base + ChronoDuration::milliseconds(index as i64))
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string()
}

fn timestamp_to_rfc3339(timestamp: i64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn normalize_thread_title(raw: &str) -> Option<String> {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim_matches(|c| c == '"' || c == '\'').trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_display(trimmed, 72))
}

fn truncate_display(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return value.chars().take(max_chars).collect();
    }
    let mut output = value.chars().take(max_chars - 3).collect::<String>();
    output.push_str("...");
    output
}

fn compact_timestamp(value: &str) -> String {
    value
        .split('T')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(value)
        .to_string()
}

fn status_class(status: &ThreadStatusDto) -> &'static str {
    match status {
        ThreadStatusDto::Idle => "status-idle",
        ThreadStatusDto::Streaming => "status-streaming",
        ThreadStatusDto::AwaitingApproval => "status-awaiting_approval",
        ThreadStatusDto::Error => "status-error",
        ThreadStatusDto::Completed => "status-completed",
    }
}
