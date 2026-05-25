use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use adw::prelude::*;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::{channel::mpsc as futures_mpsc, StreamExt};
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
        approval_response_route_for_engine, normalize_approval_response_for_engine, DiffScope,
        EngineEvent, OutputStream, SandboxPolicy, ThreadScope, ThreadTranscriptBlock,
        ThreadTranscriptMessage, TurnAttachment, TurnCompletionStatus, TurnInput, TurnInputItem,
    },
    git::multi_repo,
    models::{
        MessageDto, MessageStatusDto, MessageWindowCursorDto, ThreadDto, ThreadStatusDto,
        TrustLevelDto, WorkspaceDto,
    },
};

const APP_ID: &str = "com.supacodex.app";
const DEFAULT_ENGINE_ID: &str = "codex";
const DEFAULT_MODEL_ID: &str = "gpt-5.3-codex";
const SIDEBAR_WIDTH: i32 = 320;
const COMPOSER_SINGLE_LINE_HEIGHT: i32 = 36;
const COMPOSER_LINE_HEIGHT: i32 = 24;
const COMPOSER_MAX_HEIGHT: i32 = 132;
const SIDEBAR_PROJECTS_INITIAL_HEIGHT: i32 = 390;
const ATTACHMENT_PREVIEW_WIDTH: i32 = 96;
const ATTACHMENT_PREVIEW_HEIGHT: i32 = 96;
const ATTACHMENT_MENTION_TAG_NAME: &str = "attachment-mention";
const INITIAL_MESSAGE_WINDOW_LIMIT: usize = 48;
const BACKGROUND_MESSAGE_PAGE_LIMIT: usize = 160;
const MESSAGE_SCROLL_SETTLE_PASSES: u8 = 4;
const MESSAGE_SCROLL_SETTLE_INTERVAL: Duration = Duration::from_millis(32);
const STREAM_UI_FLUSH_INTERVAL: Duration = Duration::from_millis(48);
const STREAM_DB_FLUSH_INTERVAL: Duration = Duration::from_millis(260);
const PERF_WARN_THRESHOLD: Duration = Duration::from_millis(24);
const CODEX_TRANSCRIPT_SYNC_VERSION: i64 = 2;

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
  background: @window_bg_color;
  background-color: @window_bg_color;
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
  padding: 4px 16px 8px;
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
  padding: 0;
}

window.supacodex-window headerbar.app-header menubutton.mode-pill > button,
window.supacodex-window headerbar.app-header menubutton.mode-pill > button:backdrop {
  background: transparent;
  background-image: none;
  border-color: transparent;
  border-radius: 999px;
  box-shadow: none;
  color: inherit;
  font-weight: 600;
  min-height: 34px;
  padding: 0 12px;
}

window.supacodex-window headerbar.app-header menubutton.mode-pill > button:hover,
window.supacodex-window headerbar.app-header menubutton.mode-pill > button:active,
window.supacodex-window headerbar.app-header menubutton.mode-pill > button:checked {
  background: transparent;
  border-color: transparent;
  box-shadow: none;
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
  background: @window_bg_color;
  background-color: @window_bg_color;
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

.attachment-chip,
.attachment-image-chip {
  background: color-mix(in srgb, @window_fg_color 7%, transparent);
  border: 1px solid alpha(@window_fg_color, 0.060);
  border-radius: 8px;
  margin-right: 6px;
}

.attachment-chip {
  min-height: 34px;
  padding: 4px 6px;
}

.attachment-image-chip {
  min-height: 96px;
  min-width: 96px;
}

.attachment-thumb {
  border-radius: 6px;
}

.attachment-preview-fallback {
  background: alpha(@window_fg_color, 0.055);
  border-radius: 8px;
  color: alpha(@window_fg_color, 0.54);
}

.attachment-preview-loading {
  background: alpha(@window_fg_color, 0.050);
  border-radius: 8px;
  color: alpha(@window_fg_color, 0.62);
}

.attachment-image-meta {
  background: rgba(0, 0, 0, 0.58);
  border-radius: 6px;
  margin: 5px;
  padding: 3px 5px;
}

.attachment-image-title {
  color: rgba(255, 255, 255, 0.96);
  font-size: 10px;
  font-weight: 700;
}

.attachment-image-size {
  color: rgba(255, 255, 255, 0.74);
  font-size: 9px;
}

.attachment-remove-button {
  background: rgba(24, 24, 28, 0.72);
  border: 1px solid rgba(255, 255, 255, 0.24);
  border-radius: 999px;
  box-shadow: none;
  margin: 5px;
  min-height: 22px;
  min-width: 22px;
  padding: 0;
}

.attachment-remove-button:hover {
  background: rgba(42, 42, 48, 0.86);
}

.attachment-chip button {
  background: transparent;
  border: none;
  box-shadow: none;
  min-height: 22px;
  min-width: 22px;
  padding: 0;
}

.attachment-chip button.attachment-remove-button {
  background: rgba(24, 24, 28, 0.72);
  border: 1px solid rgba(255, 255, 255, 0.24);
  border-radius: 999px;
  margin: 0 0 0 2px;
  min-height: 28px;
  min-width: 28px;
}

.attachment-chip button.attachment-remove-button:hover {
  background: rgba(42, 42, 48, 0.86);
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

.message-card.message-editing,
.message-card.message-editing:backdrop {
  background: color-mix(in srgb, @accent_bg_color 13%, transparent);
  border-color: color-mix(in srgb, @accent_bg_color 34%, transparent);
}

.message-edit-scroll {
  background: alpha(@window_fg_color, 0.045);
  border: 1px solid alpha(@window_fg_color, 0.070);
  border-radius: 8px;
  box-shadow: none;
}

.message-edit-scroll viewport,
textview.message-edit-view,
textview.message-edit-view.view,
textview.message-edit-view text {
  background: transparent;
  background-color: transparent;
  color: @window_fg_color;
}

.message-edit-actions {
  margin-top: 2px;
}

.message-edit-actions button {
  border-radius: 8px;
  min-height: 32px;
  min-width: 112px;
  padding: 0 12px;
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

.collapsible-block {
  padding: 7px 10px;
}

.collapsible-block.reasoning-block,
.collapsible-block.reasoning-block:backdrop {
  background: color-mix(in srgb, @accent_bg_color 8%, transparent);
  border-color: color-mix(in srgb, @accent_bg_color 24%, transparent);
}

.collapsible-block.changes-block,
.collapsible-block.changes-block:backdrop {
  background: alpha(@window_fg_color, 0.024);
  border-color: alpha(@window_fg_color, 0.070);
}

.collapsible-header {
  margin-bottom: 0;
}

.collapsible-title-row {
  min-height: 22px;
}

.collapsible-toggle {
  background: transparent;
  background-image: none;
  border: none;
  box-shadow: none;
  padding: 0;
}

.collapsible-toggle:hover {
  background: alpha(@window_fg_color, 0.045);
}

.collapsible-chevron {
  color: alpha(@window_fg_color, 0.62);
}

.collapsible-content {
  margin-top: 6px;
}

.block-subtitle {
  color: alpha(@window_fg_color, 0.56);
  font-size: 11px;
}

.reasoning-text {
  color: alpha(@window_fg_color, 0.78);
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
  background: color-mix(in srgb, @window_fg_color 7%, transparent);
  border: 1px solid alpha(@window_fg_color, 0.050);
  border-radius: 24px;
  padding: 6px 8px;
}

.composer-wrap:backdrop {
  background: color-mix(in srgb, @window_fg_color 7%, transparent);
  border: 1px solid alpha(@window_fg_color, 0.050);
  border-radius: 24px;
  padding: 6px 8px;
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
  padding: 0;
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
  min-height: 16px;
}

textview.composer-view.view,
textview.composer-view text {
  min-height: 16px;
}

.send-button {
  -gtk-icon-size: 16px;
  background: transparent;
  background-image: none;
  border-color: transparent;
  border-radius: 999px;
  box-shadow: none;
  min-height: 32px;
  min-width: 32px;
  padding: 0;
}

.send-button:hover {
  background: alpha(@window_fg_color, 0.080);
  border-color: transparent;
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
    SyncActiveWorkspace,
    WorkspaceOpened(Result<WorkspaceDto, String>),
    ThreadCreated {
        workspace_id: String,
        result: Result<ThreadDto, String>,
    },
    CodexThreadsSynced {
        workspace_id: String,
        result: Result<usize, String>,
    },
    CodexTranscriptSynced {
        thread_id: String,
        result: Result<(), String>,
    },
    CodexProfileSet {
        profile_id: String,
        result: Result<(), String>,
    },
    WorkspaceTrustSet {
        workspace_id: String,
        result: Result<(), String>,
    },
    AttachmentsResolved {
        insertion_offset: i32,
        results: Vec<Result<PendingAttachment, String>>,
    },
    AttachmentThumbnailReady {
        file_path: String,
        result: Result<String, String>,
    },
    ViewSnapshotLoaded(Result<ViewSnapshot, String>),
    ThreadHistoryLoaded {
        thread_id: String,
        messages: Vec<MessageDto>,
        complete: bool,
    },
    StreamingMessageUpdated {
        thread_id: String,
        message: MessageDto,
    },
    SelectThread(String),
    OpenThreadTab(String),
    CloseThreadTab(String),
    StartEditMessage {
        thread_id: String,
        message_id: String,
        user_turn_index: usize,
        content: String,
    },
    UpdateEditMessageDraft {
        message_id: String,
        user_turn_index: usize,
        content: String,
    },
    CancelEditMessage(String),
    SubmitEditMessage(String),
    SetCodexProfile(String),
    SetWorkspaceTrust(TrustLevelDto),
    RemoveAttachment(usize),
    Toast(String),
}

#[derive(Clone)]
struct UiEventSender {
    sender: futures_mpsc::UnboundedSender<UiEvent>,
    backlog: Arc<AtomicUsize>,
}

impl UiEventSender {
    fn new(sender: futures_mpsc::UnboundedSender<UiEvent>) -> Self {
        Self {
            sender,
            backlog: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn send(&self, event: UiEvent) -> bool {
        self.backlog.fetch_add(1, Ordering::Relaxed);
        if self.sender.unbounded_send(event).is_ok() {
            true
        } else {
            self.backlog.fetch_sub(1, Ordering::Relaxed);
            false
        }
    }

    fn mark_processed(&self, count: usize) {
        self.backlog.fetch_sub(count, Ordering::Relaxed);
    }

    fn backlog_len(&self) -> usize {
        self.backlog.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
struct PendingAttachment {
    file_name: String,
    file_path: String,
    size_bytes: u64,
    mime_type: Option<String>,
    mention: Option<String>,
    thumbnail_path: Option<String>,
    thumbnail_failed: bool,
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

#[derive(Debug, Clone)]
struct ViewSnapshot {
    requested_workspace_id: Option<String>,
    requested_thread_id: Option<String>,
    workspaces: Vec<WorkspaceDto>,
    active_workspace_id: Option<String>,
    threads: Vec<ThreadDto>,
    active_thread: Option<ThreadDto>,
    messages: Vec<MessageDto>,
    messages_next_cursor: Option<MessageWindowCursorDto>,
    trust_level: TrustLevelDto,
}

#[derive(Debug, Clone, Default)]
struct CachedThreadView {
    thread: Option<ThreadDto>,
    messages: Vec<MessageDto>,
    history_complete: bool,
    scroll_value: Option<f64>,
}

#[derive(Clone)]
struct CachedListRow {
    row: gtk::ListBoxRow,
    signature: String,
}

#[derive(Debug, Clone)]
struct EditingMessageState {
    thread_id: String,
    message_id: String,
    user_turn_index: usize,
    draft: String,
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
    syncing_workspaces: Arc<Mutex<HashSet<String>>>,
    syncing_transcripts: Arc<Mutex<HashSet<String>>>,
    opening_workspaces: Arc<Mutex<HashSet<String>>>,
    creating_threads: Arc<Mutex<HashSet<String>>>,
    setting_codex_profile: Arc<Mutex<bool>>,
    setting_workspace_trusts: Arc<Mutex<HashSet<String>>>,
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
            syncing_workspaces: Arc::new(Mutex::new(HashSet::new())),
            syncing_transcripts: Arc::new(Mutex::new(HashSet::new())),
            opening_workspaces: Arc::new(Mutex::new(HashSet::new())),
            creating_threads: Arc::new(Mutex::new(HashSet::new())),
            setting_codex_profile: Arc::new(Mutex::new(false)),
            setting_workspace_trusts: Arc::new(Mutex::new(HashSet::new())),
        }))
    }

    fn list_workspaces(&self) -> anyhow::Result<Vec<WorkspaceDto>> {
        db::workspaces::list_workspaces(&self.db)
    }

    fn list_threads(&self, workspace_id: &str) -> anyhow::Result<Vec<ThreadDto>> {
        db::threads::list_threads_for_workspace(&self.db, workspace_id)
    }

    fn get_thread(&self, thread_id: &str) -> anyhow::Result<Option<ThreadDto>> {
        db::threads::get_thread(&self.db, thread_id)
    }

    fn get_messages_window(
        &self,
        thread_id: &str,
        cursor: Option<&MessageWindowCursorDto>,
        limit: usize,
    ) -> anyhow::Result<(Vec<MessageDto>, Option<MessageWindowCursorDto>)> {
        let started = Instant::now();
        let window = db::messages::get_thread_messages_window(&self.db, thread_id, cursor, limit)?;
        log_perf(
            "sql.get_thread_messages_window",
            started,
            format!(
                "thread_id={}, rows={}, has_more={}",
                thread_id,
                window.messages.len(),
                window.next_cursor.is_some()
            ),
        );
        Ok((window.messages, window.next_cursor))
    }

    fn load_view_snapshot(
        &self,
        preferred_workspace_id: Option<String>,
        preferred_thread_id: Option<String>,
    ) -> anyhow::Result<ViewSnapshot> {
        let started = Instant::now();
        let requested_workspace_id = preferred_workspace_id.clone();
        let requested_thread_id = preferred_thread_id.clone();
        let workspaces = self.list_workspaces()?;
        let active_workspace_id = preferred_workspace_id
            .filter(|workspace_id| {
                workspaces
                    .iter()
                    .any(|workspace| workspace.id == *workspace_id)
            })
            .or_else(|| workspaces.first().map(|workspace| workspace.id.clone()));

        let Some(workspace_id) = active_workspace_id.clone() else {
            return Ok(ViewSnapshot {
                requested_workspace_id,
                requested_thread_id,
                workspaces,
                active_workspace_id: None,
                threads: Vec::new(),
                active_thread: None,
                messages: Vec::new(),
                messages_next_cursor: None,
                trust_level: TrustLevelDto::Standard,
            });
        };

        let mut threads = self.list_threads(&workspace_id)?;
        if let Some(active_thread_id) = preferred_thread_id.as_ref() {
            if !threads.iter().any(|thread| thread.id == *active_thread_id) {
                if let Some(active_thread) = self.get_thread(active_thread_id)? {
                    if active_thread.workspace_id == workspace_id {
                        threads.insert(0, active_thread);
                    }
                }
            }
        }

        let active_thread_id = preferred_thread_id
            .filter(|thread_id| threads.iter().any(|thread| thread.id == *thread_id))
            .or_else(|| threads.first().map(|thread| thread.id.clone()));
        let active_thread = active_thread_id.as_deref().and_then(|thread_id| {
            threads
                .iter()
                .find(|thread| thread.id == thread_id)
                .cloned()
        });
        let (messages, messages_next_cursor) = match active_thread.as_ref() {
            Some(thread) => {
                self.get_messages_window(&thread.id, None, INITIAL_MESSAGE_WINDOW_LIMIT)?
            }
            None => (Vec::new(), None),
        };
        let trust_level = self.workspace_trust_level(&workspace_id);

        let snapshot = ViewSnapshot {
            requested_workspace_id,
            requested_thread_id,
            workspaces,
            active_workspace_id: Some(workspace_id),
            threads,
            active_thread,
            messages,
            messages_next_cursor,
            trust_level,
        };
        log_perf(
            "sql.load_view_snapshot",
            started,
            format!(
                "workspaces={}, threads={}, messages={}, has_more={}",
                snapshot.workspaces.len(),
                snapshot.threads.len(),
                snapshot.messages.len(),
                snapshot.messages_next_cursor.is_some()
            ),
        );
        Ok(snapshot)
    }

    fn load_view_snapshot_async(
        self: &Arc<Self>,
        preferred_workspace_id: Option<String>,
        preferred_thread_id: Option<String>,
        ui_tx: UiEventSender,
    ) {
        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                backend.load_view_snapshot(preferred_workspace_id, preferred_thread_id)
            })
            .await
            .map_err(|error| anyhow::anyhow!("view snapshot task failed: {error}"))
            .and_then(|result| result)
            .map_err(|error| format!("{error:#}"));

            let _ = ui_tx.send(UiEvent::ViewSnapshotLoaded(result));
        });
    }

    fn load_thread_history_async(
        self: &Arc<Self>,
        thread_id: String,
        cursor: MessageWindowCursorDto,
        ui_tx: UiEventSender,
    ) {
        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let thread_id_for_task = thread_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                backend.load_thread_history(&thread_id_for_task, cursor)
            })
            .await
            .map_err(|error| anyhow::anyhow!("thread history load task failed: {error}"))
            .and_then(|result| result)
            .map_err(|error| format!("{error:#}"));

            match result {
                Ok(messages) => {
                    let _ = ui_tx.send(UiEvent::ThreadHistoryLoaded {
                        thread_id,
                        messages,
                        complete: true,
                    });
                }
                Err(error) => {
                    let _ = ui_tx.send(UiEvent::Toast(error));
                }
            }
        });
    }

    fn load_thread_history(
        &self,
        thread_id: &str,
        mut cursor: MessageWindowCursorDto,
    ) -> anyhow::Result<Vec<MessageDto>> {
        let started = Instant::now();
        let mut pages = Vec::new();
        loop {
            let window = db::messages::get_thread_messages_window(
                &self.db,
                thread_id,
                Some(&cursor),
                BACKGROUND_MESSAGE_PAGE_LIMIT,
            )?;
            let next_cursor = window.next_cursor.clone();
            if !window.messages.is_empty() {
                pages.push(window.messages);
            }
            let Some(next_cursor) = next_cursor else {
                break;
            };
            cursor = next_cursor;
        }

        pages.reverse();
        let messages = pages.into_iter().flatten().collect::<Vec<_>>();
        log_perf(
            "sql.load_thread_history",
            started,
            format!("thread_id={thread_id}, rows={}", messages.len()),
        );
        Ok(messages)
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

    fn open_workspace_async(self: &Arc<Self>, path: PathBuf, ui_tx: UiEventSender) {
        let path_string = path.to_string_lossy().to_string();
        if !self.mark_opening_workspace(&path_string) {
            return;
        }

        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let path_for_task = path_string.clone();
            let backend_for_task = Arc::clone(&backend);
            let result = tokio::task::spawn_blocking(move || {
                backend_for_task.open_workspace(&path_for_task)
            })
            .await
            .map_err(|error| anyhow::anyhow!("workspace open task failed: {error}"))
            .and_then(|result| result)
            .map_err(|error| format!("{error:#}"));

            backend.unmark_opening_workspace(&path_string);
            let _ = ui_tx.send(UiEvent::WorkspaceOpened(result));
        });
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

        let active_profile_id = self.active_codex_profile_id();
        let mut metadata = created.engine_metadata.clone().unwrap_or_else(|| json!({}));
        set_codex_profile_id(&mut metadata, &active_profile_id);
        db::threads::update_engine_metadata(&self.db, &created.id, &metadata)?;

        db::threads::get_thread(&self.db, &created.id)?
            .ok_or_else(|| anyhow::anyhow!("thread not found after creation"))
    }

    fn create_thread_async(self: &Arc<Self>, workspace_id: String, ui_tx: UiEventSender) -> bool {
        if !self.mark_creating_thread(&workspace_id) {
            return false;
        }

        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let workspace_id_for_task = workspace_id.clone();
            let backend_for_task = Arc::clone(&backend);
            let result = tokio::task::spawn_blocking(move || {
                backend_for_task.create_thread(&workspace_id_for_task)
            })
            .await
            .map_err(|error| anyhow::anyhow!("thread creation task failed: {error}"))
            .and_then(|result| result)
            .map_err(|error| format!("{error:#}"));

            backend.unmark_creating_thread(&workspace_id);
            let _ = ui_tx.send(UiEvent::ThreadCreated {
                workspace_id,
                result,
            });
        });
        true
    }

    fn create_thread_and_send_message_async(
        self: &Arc<Self>,
        workspace_id: String,
        message: String,
        attachments: Vec<TurnAttachment>,
        ui_tx: UiEventSender,
    ) -> bool {
        if !self.mark_creating_thread(&workspace_id) {
            return false;
        }

        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let workspace_id_for_task = workspace_id.clone();
            let backend_for_task = Arc::clone(&backend);
            let result = tokio::task::spawn_blocking(move || {
                backend_for_task.create_thread(&workspace_id_for_task)
            })
            .await
            .map_err(|error| anyhow::anyhow!("thread creation task failed: {error}"))
            .and_then(|result| result);

            backend.unmark_creating_thread(&workspace_id);
            match result {
                Ok(thread) => {
                    let thread_id = thread.id.clone();
                    let _ = ui_tx.send(UiEvent::SelectThread(thread_id.clone()));
                    let _ = ui_tx.send(UiEvent::Reload);
                    if let Err(error) = backend
                        .run_message_turn(thread_id.clone(), message, attachments, ui_tx.clone())
                        .await
                    {
                        let _ = ui_tx.send(UiEvent::Toast(format!("{error:#}")));
                        let _ = ui_tx.send(UiEvent::Reload);
                        backend.finish_running(&thread_id);
                    }
                }
                Err(error) => {
                    let _ = ui_tx.send(UiEvent::Toast(format!("{error:#}")));
                }
            }
        });
        true
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

    fn sync_codex_threads_for_workspace_async(
        self: &Arc<Self>,
        workspace_id: String,
        ui_tx: UiEventSender,
    ) {
        if !self.mark_syncing_workspace(&workspace_id) {
            return;
        }

        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let workspace_id_for_task = workspace_id.clone();
            let backend_for_task = Arc::clone(&backend);
            let result = tokio::task::spawn_blocking(move || {
                backend_for_task.sync_codex_threads_for_workspace(&workspace_id_for_task)
            })
            .await
            .map_err(|error| anyhow::anyhow!("Codex thread sync task failed: {error}"))
            .and_then(|result| result)
            .map_err(|error| format!("{error:#}"));

            backend.unmark_syncing_workspace(&workspace_id);
            let _ = ui_tx.send(UiEvent::CodexThreadsSynced {
                workspace_id,
                result,
            });
        });
    }

    fn sync_codex_thread_transcript_if_needed_async(
        self: &Arc<Self>,
        thread_id: String,
        ui_tx: UiEventSender,
    ) {
        if !self.mark_syncing_transcript(&thread_id) {
            return;
        }

        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let result = backend
                .sync_codex_thread_transcript_if_needed(&thread_id)
                .await
                .map_err(|error| format!("{error:#}"));

            backend.unmark_syncing_transcript(&thread_id);
            let _ = ui_tx.send(UiEvent::CodexTranscriptSynced { thread_id, result });
        });
    }

    async fn sync_codex_thread_transcript_if_needed(&self, thread_id: &str) -> anyhow::Result<()> {
        let thread = db::threads::get_thread(&self.db, thread_id)?
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        if thread.engine_id != "codex" || thread.engine_thread_id.is_none() {
            return Ok(());
        }
        if self.is_running(&thread.id) || !codex_transcript_sync_needed(&thread) {
            return Ok(());
        }

        let snapshot = {
            self.set_codex_profile_for_thread(&thread).await?;
            self.engines
                .read_codex_thread_transcript_snapshot(&thread)
                .await
        }?;
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
            .map(|(index, message)| {
                Ok(db::messages::ImportedThreadMessage {
                    role: message.role.as_str().to_string(),
                    text: message.content.clone(),
                    blocks: Some(serde_json::to_value(
                        native_blocks_from_transcript_message(message),
                    )?),
                    created_at: transcript_message_timestamp(snapshot.created_at, index),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

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
        metadata["codexTranscriptSyncVersion"] =
            Value::Number(CODEX_TRANSCRIPT_SYNC_VERSION.into());

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

    fn set_active_codex_profile_async(
        self: &Arc<Self>,
        profile_id: String,
        ui_tx: UiEventSender,
    ) -> bool {
        if !self.mark_setting_codex_profile() {
            return false;
        }

        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let profile_id_for_config = profile_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                let profile = AppConfig::mutate(|config| {
                    config.codex.active_profile_id = profile_id_for_config.clone();
                    config.codex.normalize();
                    runtime_profile_by_id(config, &profile_id_for_config).ok_or_else(|| {
                        anyhow::anyhow!("unknown Codex profile: {profile_id_for_config}")
                    })
                })?;
                let config = AppConfig::load_or_create()?;
                Ok::<_, anyhow::Error>((profile, config))
            })
            .await
            .map_err(|error| anyhow::anyhow!("Codex profile update task failed: {error}"))
            .and_then(|result| result);

            let result = match result {
                Ok((profile, config)) => {
                    let config_result = backend
                        .config
                        .lock()
                        .map(|mut stored| {
                            *stored = config;
                        })
                        .map_err(|_| anyhow::anyhow!("app config lock is poisoned"));
                    match config_result {
                        Ok(()) => backend.engines.set_codex_profile(profile).await,
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            }
            .map_err(|error| format!("{error:#}"));

            backend.unmark_setting_codex_profile();
            let _ = ui_tx.send(UiEvent::CodexProfileSet { profile_id, result });
        });
        true
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

    fn set_workspace_trust_level_async(
        self: &Arc<Self>,
        workspace_id: String,
        trust_level: TrustLevelDto,
        ui_tx: UiEventSender,
    ) -> bool {
        if !self.mark_setting_workspace_trust(&workspace_id) {
            return false;
        }

        let backend = Arc::clone(self);
        self.runtime.spawn(async move {
            let workspace_id_for_task = workspace_id.clone();
            let backend_for_task = Arc::clone(&backend);
            let result = tokio::task::spawn_blocking(move || {
                backend_for_task.set_workspace_trust_level(&workspace_id_for_task, trust_level)
            })
            .await
            .map_err(|error| anyhow::anyhow!("workspace trust update task failed: {error}"))
            .and_then(|result| result)
            .map_err(|error| format!("{error:#}"));

            backend.unmark_setting_workspace_trust(&workspace_id);
            let _ = ui_tx.send(UiEvent::WorkspaceTrustSet {
                workspace_id,
                result,
            });
        });
        true
    }

    fn is_running(&self, thread_id: &str) -> bool {
        self.running
            .lock()
            .map(|running| running.contains_key(thread_id))
            .unwrap_or(false)
    }

    fn is_creating_thread(&self, workspace_id: &str) -> bool {
        self.creating_threads
            .lock()
            .map(|creating| creating.contains(workspace_id))
            .unwrap_or(false)
    }

    fn mark_syncing_workspace(&self, workspace_id: &str) -> bool {
        self.syncing_workspaces
            .lock()
            .map(|mut syncing| syncing.insert(workspace_id.to_string()))
            .unwrap_or(false)
    }

    fn unmark_syncing_workspace(&self, workspace_id: &str) {
        if let Ok(mut syncing) = self.syncing_workspaces.lock() {
            syncing.remove(workspace_id);
        }
    }

    fn mark_syncing_transcript(&self, thread_id: &str) -> bool {
        self.syncing_transcripts
            .lock()
            .map(|mut syncing| syncing.insert(thread_id.to_string()))
            .unwrap_or(false)
    }

    fn unmark_syncing_transcript(&self, thread_id: &str) {
        if let Ok(mut syncing) = self.syncing_transcripts.lock() {
            syncing.remove(thread_id);
        }
    }

    fn mark_opening_workspace(&self, path: &str) -> bool {
        self.opening_workspaces
            .lock()
            .map(|mut opening| opening.insert(path.to_string()))
            .unwrap_or(false)
    }

    fn unmark_opening_workspace(&self, path: &str) {
        if let Ok(mut opening) = self.opening_workspaces.lock() {
            opening.remove(path);
        }
    }

    fn mark_creating_thread(&self, workspace_id: &str) -> bool {
        self.creating_threads
            .lock()
            .map(|mut creating| creating.insert(workspace_id.to_string()))
            .unwrap_or(false)
    }

    fn unmark_creating_thread(&self, workspace_id: &str) {
        if let Ok(mut creating) = self.creating_threads.lock() {
            creating.remove(workspace_id);
        }
    }

    fn mark_setting_codex_profile(&self) -> bool {
        self.setting_codex_profile
            .lock()
            .map(|mut setting| {
                if *setting {
                    false
                } else {
                    *setting = true;
                    true
                }
            })
            .unwrap_or(false)
    }

    fn unmark_setting_codex_profile(&self) {
        if let Ok(mut setting) = self.setting_codex_profile.lock() {
            *setting = false;
        }
    }

    fn mark_setting_workspace_trust(&self, workspace_id: &str) -> bool {
        self.setting_workspace_trusts
            .lock()
            .map(|mut setting| setting.insert(workspace_id.to_string()))
            .unwrap_or(false)
    }

    fn unmark_setting_workspace_trust(&self, workspace_id: &str) {
        if let Ok(mut setting) = self.setting_workspace_trusts.lock() {
            setting.remove(workspace_id);
        }
    }

    fn send_message(
        self: &Arc<Self>,
        thread_id: String,
        message: String,
        attachments: Vec<TurnAttachment>,
        ui_tx: UiEventSender,
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

    fn cancel_turn(self: &Arc<Self>, thread_id: String, ui_tx: UiEventSender) {
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
        user_turn_index: usize,
        message: String,
        ui_tx: UiEventSender,
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
                let target_index = find_resume_target_index(
                    &messages,
                    &message_id,
                    user_turn_index,
                )
                .ok_or_else(|| {
                    anyhow::anyhow!("Message introuvable. Recharge le thread puis reessaie.")
                })?;
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
        ui_tx: UiEventSender,
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
        ui_tx: UiEventSender,
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
        let _ = ui_tx.send(UiEvent::StreamingMessageUpdated {
            thread_id: thread.id.clone(),
            message: streaming_message_snapshot(
                &assistant_message,
                &blocks,
                message_status.clone(),
                &effective_model_id,
            )?,
        });

        let mut last_ui_flush = Instant::now();
        let mut last_db_flush = Instant::now();
        let mut db_dirty = false;
        while let Some(event) = event_rx.recv().await {
            let force_flush = matches!(
                event,
                EngineEvent::TurnCompleted { .. }
                    | EngineEvent::ApprovalRequested { .. }
                    | EngineEvent::ActionStarted { .. }
                    | EngineEvent::ActionCompleted { .. }
                    | EngineEvent::Error { .. }
            );
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
            db_dirty = true;

            if db_dirty && (force_flush || last_db_flush.elapsed() >= STREAM_DB_FLUSH_INTERVAL) {
                self.persist_blocks(&assistant_message.id, &blocks, message_status.clone(), "")?;
                last_db_flush = Instant::now();
                db_dirty = false;
            }

            if force_flush || last_ui_flush.elapsed() >= STREAM_UI_FLUSH_INTERVAL {
                let _ = ui_tx.send(UiEvent::StreamingMessageUpdated {
                    thread_id: thread.id.clone(),
                    message: streaming_message_snapshot(
                        &assistant_message,
                        &blocks,
                        message_status.clone(),
                        &effective_model_id,
                    )?,
                });
                last_ui_flush = Instant::now();
            }
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

        if db_dirty {
            self.persist_blocks(&assistant_message.id, &blocks, message_status.clone(), "")?;
        }
        self.persist_blocks(
            &assistant_message.id,
            &blocks,
            message_status.clone(),
            &effective_model_id,
        )?;
        let _ = ui_tx.send(UiEvent::StreamingMessageUpdated {
            thread_id: thread.id.clone(),
            message: streaming_message_snapshot(
                &assistant_message,
                &blocks,
                message_status.clone(),
                &effective_model_id,
            )?,
        });
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
        _assistant_message_id: &str,
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
                append_or_replace_diff_block(blocks, diff, &diff_scope_label(scope));
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

        Ok(())
    }

    fn persist_blocks(
        &self,
        assistant_message_id: &str,
        blocks: &[NativeContentBlock],
        status: MessageStatusDto,
        model_id: &str,
    ) -> anyhow::Result<()> {
        let started = Instant::now();
        let blocks_json = serde_json::to_string(blocks)?;
        let blocks_json_bytes = blocks_json.len();
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
        log_perf(
            "sql.persist_blocks",
            started,
            format!(
                "message_id={}, blocks={}, bytes={}",
                assistant_message_id,
                blocks.len(),
                blocks_json_bytes
            ),
        );
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
        let engines = Arc::clone(&self.engines);
        self.runtime.spawn(async move {
            engines.shutdown().await;
        });
    }
}

struct AppController {
    backend: Arc<NativeBackend>,
    ui_tx: UiEventSender,
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
    composer_scroll: gtk::ScrolledWindow,
    composer: gtk::TextView,
    send_button: gtk::Button,
    search_entry: gtk::SearchEntry,
    workspaces: RefCell<Vec<WorkspaceDto>>,
    threads: RefCell<Vec<ThreadDto>>,
    active_thread_snapshot: RefCell<Option<ThreadDto>>,
    active_messages: RefCell<Vec<MessageDto>>,
    active_trust_level: RefCell<TrustLevelDto>,
    visible_workspace_ids: RefCell<Vec<String>>,
    visible_thread_ids: RefCell<Vec<String>>,
    workspace_row_cache: RefCell<HashMap<String, CachedListRow>>,
    thread_row_cache: RefCell<HashMap<String, CachedListRow>>,
    message_widget_cache: RefCell<HashMap<String, gtk::Widget>>,
    collapsible_block_expanded: Rc<RefCell<HashMap<String, bool>>>,
    thread_view_cache: RefCell<HashMap<String, CachedThreadView>>,
    thread_tabs_by_workspace: RefCell<HashMap<String, Vec<String>>>,
    syncing_tab_view: Cell<bool>,
    loading_snapshot: Cell<bool>,
    queued_snapshot: Cell<bool>,
    composer_last_height: Cell<i32>,
    composer_last_line_count: Cell<i32>,
    composer_last_send_enabled: Cell<bool>,
    composer_last_running: Cell<bool>,
    render_widgets_created: Cell<usize>,
    runtime_controls_signature: RefCell<String>,
    editing_message: Rc<RefCell<Option<EditingMessageState>>>,
    pending_attachments: RefCell<Vec<PendingAttachment>>,
    snapping_attachment_mention_cursor: Cell<bool>,
    active_workspace_id: RefCell<Option<String>>,
    active_thread_id: RefCell<Option<String>>,
    split_view: adw::OverlaySplitView,
}

impl AppController {
    fn new(app: &adw::Application, backend: Arc<NativeBackend>) -> Rc<Self> {
        let (raw_ui_tx, ui_rx) = futures_mpsc::unbounded::<UiEvent>();
        let ui_tx = UiEventSender::new(raw_ui_tx);

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
        title_box.set_overflow(gtk::Overflow::Hidden);
        title_box.set_size_request(-1, 32);

        let title_label = gtk::Label::new(Some("SupaCodex"));
        title_label.add_css_class("app-title-main");
        title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_label.set_justify(gtk::Justification::Center);
        title_label.set_lines(1);
        title_label.set_max_width_chars(48);
        title_label.set_overflow(gtk::Overflow::Hidden);
        title_label.set_single_line_mode(true);
        title_label.set_width_chars(1);
        title_label.set_wrap(false);
        title_label.set_xalign(0.5);

        let subtitle_label = gtk::Label::new(Some("codex"));
        subtitle_label.add_css_class("app-title-subtitle");
        subtitle_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        subtitle_label.set_justify(gtk::Justification::Center);
        subtitle_label.set_lines(1);
        subtitle_label.set_max_width_chars(42);
        subtitle_label.set_overflow(gtk::Overflow::Hidden);
        subtitle_label.set_single_line_mode(true);
        subtitle_label.set_width_chars(1);
        subtitle_label.set_wrap(false);
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
        profile_button.set_valign(gtk::Align::Center);
        profile_button.set_child(Some(&profile_button_label));

        let permission_button_label = gtk::Label::new(Some("Standard"));
        let permission_button = gtk::MenuButton::new();
        permission_button.add_css_class("mode-pill");
        permission_button.set_tooltip_text(Some("Permissions"));
        permission_button.set_valign(gtk::Align::Center);
        permission_button.set_child(Some(&permission_button_label));

        let header_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header_actions.add_css_class("header-actions");
        header_actions.set_valign(gtk::Align::Center);
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
        composer_wrap.set_vexpand(false);
        composer_wrap.set_valign(gtk::Align::End);
        content.append(&composer_wrap);

        let attachment_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        attachment_bar.add_css_class("attachment-bar");
        attachment_bar.set_halign(gtk::Align::Start);
        attachment_bar.set_hexpand(false);
        attachment_bar.set_visible(false);
        composer_wrap.append(&attachment_bar);

        let composer_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        composer_row.set_vexpand(false);
        composer_row.set_valign(gtk::Align::Center);
        composer_wrap.append(&composer_row);

        let attach_button = gtk::Button::builder()
            .icon_name("mail-attachment-symbolic")
            .tooltip_text("Ajouter une piece jointe")
            .build();
        attach_button.add_css_class("send-button");
        attach_button.set_valign(gtk::Align::Center);
        composer_row.append(&attach_button);

        let composer = gtk::TextView::new();
        composer.add_css_class("composer-view");
        composer.set_wrap_mode(gtk::WrapMode::WordChar);
        composer.set_vexpand(false);
        composer.set_monospace(false);
        composer.set_top_margin(18);
        composer.set_bottom_margin(0);
        composer.set_left_margin(2);
        composer.set_right_margin(2);
        let composer_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(COMPOSER_SINGLE_LINE_HEIGHT)
            .max_content_height(COMPOSER_SINGLE_LINE_HEIGHT)
            .build();
        composer_scroll.add_css_class("composer-scroll");
        composer_scroll.set_hexpand(true);
        composer_scroll.set_vexpand(false);
        composer_scroll.set_valign(gtk::Align::Center);
        composer_scroll.set_propagate_natural_height(false);
        composer_scroll.set_size_request(-1, COMPOSER_SINGLE_LINE_HEIGHT);
        composer_scroll.set_child(Some(&composer));
        composer_row.append(&composer_scroll);

        let send_button = gtk::Button::builder()
            .icon_name("mail-send-symbolic")
            .tooltip_text("Envoyer")
            .build();
        send_button.add_css_class("send-button");
        send_button.set_valign(gtk::Align::Center);
        send_button.set_opacity(0.0);
        send_button.set_sensitive(false);
        composer_row.append(&send_button);

        let controller = Rc::new(Self {
            backend,
            ui_tx,
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
            composer_scroll,
            composer,
            send_button,
            search_entry,
            workspaces: RefCell::new(Vec::new()),
            threads: RefCell::new(Vec::new()),
            active_thread_snapshot: RefCell::new(None),
            active_messages: RefCell::new(Vec::new()),
            active_trust_level: RefCell::new(TrustLevelDto::Standard),
            visible_workspace_ids: RefCell::new(Vec::new()),
            visible_thread_ids: RefCell::new(Vec::new()),
            workspace_row_cache: RefCell::new(HashMap::new()),
            thread_row_cache: RefCell::new(HashMap::new()),
            message_widget_cache: RefCell::new(HashMap::new()),
            collapsible_block_expanded: Rc::new(RefCell::new(HashMap::new())),
            thread_view_cache: RefCell::new(HashMap::new()),
            thread_tabs_by_workspace: RefCell::new(HashMap::new()),
            syncing_tab_view: Cell::new(false),
            loading_snapshot: Cell::new(false),
            queued_snapshot: Cell::new(false),
            composer_last_height: Cell::new(COMPOSER_SINGLE_LINE_HEIGHT),
            composer_last_line_count: Cell::new(1),
            composer_last_send_enabled: Cell::new(false),
            composer_last_running: Cell::new(false),
            render_widgets_created: Cell::new(0),
            runtime_controls_signature: RefCell::new(String::new()),
            editing_message: Rc::new(RefCell::new(None)),
            pending_attachments: RefCell::new(Vec::new()),
            snapping_attachment_mention_cursor: Cell::new(false),
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
                controller.request_create_thread();
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

        let file_drop_target = |weak: std::rc::Weak<AppController>| {
            let drop_target =
                gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
            drop_target.set_propagation_phase(gtk::PropagationPhase::Capture);
            drop_target.set_preload(true);
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
            drop_target
        };
        let text_drop_target = |weak: std::rc::Weak<AppController>| {
            let drop_target = gtk::DropTarget::new(String::static_type(), gdk::DragAction::COPY);
            drop_target.set_propagation_phase(gtk::PropagationPhase::Capture);
            drop_target.set_preload(true);
            drop_target.connect_drop(move |_, value, _, _| {
                let Ok(text) = value.get::<String>() else {
                    return false;
                };
                let paths = paths_from_dropped_text(&text);
                if paths.is_empty() {
                    return false;
                }
                if let Some(controller) = weak.upgrade() {
                    controller.add_attachments(paths);
                    return true;
                }
                false
            });
            drop_target
        };
        controller
            .window
            .add_controller(file_drop_target(Rc::downgrade(&controller)));
        controller
            .window
            .add_controller(text_drop_target(Rc::downgrade(&controller)));
        composer_wrap.add_controller(file_drop_target(Rc::downgrade(&controller)));
        composer_wrap.add_controller(text_drop_target(Rc::downgrade(&controller)));
        controller
            .composer_scroll
            .add_controller(file_drop_target(Rc::downgrade(&controller)));
        controller
            .composer_scroll
            .add_controller(text_drop_target(Rc::downgrade(&controller)));
        controller
            .composer
            .add_controller(file_drop_target(Rc::downgrade(&controller)));
        controller
            .composer
            .add_controller(text_drop_target(Rc::downgrade(&controller)));

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
                controller.render_workspaces();
                controller.render_threads();
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

        let weak = Rc::downgrade(&controller);
        controller
            .composer
            .buffer()
            .connect_mark_set(move |buffer, location, mark| {
                if mark.name().as_deref() != Some("insert") {
                    return;
                }
                if let Some(controller) = weak.upgrade() {
                    controller.snap_cursor_out_of_attachment_mention(buffer, location.offset());
                }
            });

        let key_controller = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&controller);
        key_controller.connect_key_pressed(move |_, key, _, state| {
            let plain_arrow = matches!(key, gdk::Key::Left | gdk::Key::Right)
                && !state.contains(gdk::ModifierType::SHIFT_MASK)
                && !state.contains(gdk::ModifierType::CONTROL_MASK);
            if plain_arrow {
                if let Some(controller) = weak.upgrade() {
                    if controller.handle_attachment_mention_arrow_key(key) {
                        return glib::Propagation::Stop;
                    }
                }
            }
            if matches!(key, gdk::Key::BackSpace | gdk::Key::Delete) {
                if let Some(controller) = weak.upgrade() {
                    if controller.handle_attachment_mention_delete_key(key) {
                        return glib::Propagation::Stop;
                    }
                }
            }
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
                    controller.request_create_thread();
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
        controller
            .messages_scroll
            .vadjustment()
            .connect_value_changed(move |adjustment| {
                if adjustment.value() <= 32.0 {
                    if let Some(controller) = weak.upgrade() {
                        controller.reveal_cached_history();
                    }
                }
            });

        let weak = Rc::downgrade(&controller);
        glib::MainContext::default().spawn_local(async move {
            run_ui_event_loop(weak, ui_rx).await;
        });

        let weak = Rc::downgrade(&controller);
        glib::timeout_add_local(Duration::from_secs(5), move || {
            if let Some(controller) = weak.upgrade() {
                let _ = controller.ui_tx.send(UiEvent::SyncActiveWorkspace);
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
        self.request_view_snapshot();
    }

    fn request_view_snapshot(&self) {
        if self.loading_snapshot.get() {
            self.queued_snapshot.set(true);
            return;
        }

        self.loading_snapshot.set(true);
        self.backend.load_view_snapshot_async(
            self.active_workspace_id.borrow().clone(),
            self.active_thread_id.borrow().clone(),
            self.ui_tx.clone(),
        );
    }

    fn apply_view_snapshot(&self, snapshot: ViewSnapshot) {
        let active_thread_id = snapshot
            .active_thread
            .as_ref()
            .map(|thread| thread.id.clone());
        if let Some(thread) = snapshot.active_thread.as_ref() {
            let mut cache = self.thread_view_cache.borrow_mut();
            let entry = cache.entry(thread.id.clone()).or_default();
            entry.thread = Some(thread.clone());
            entry.messages = merge_messages(&entry.messages, &snapshot.messages);
            entry.history_complete =
                snapshot.messages_next_cursor.is_none() || entry.history_complete;
        }
        *self.workspaces.borrow_mut() = snapshot.workspaces;
        *self.active_workspace_id.borrow_mut() = snapshot.active_workspace_id;
        *self.threads.borrow_mut() = snapshot.threads;
        *self.active_thread_id.borrow_mut() = active_thread_id;
        *self.active_thread_snapshot.borrow_mut() = snapshot.active_thread;
        *self.active_messages.borrow_mut() = snapshot.messages;
        *self.active_trust_level.borrow_mut() = snapshot.trust_level;
    }

    fn remember_active_thread_scroll(&self) {
        let Some(thread_id) = self.active_thread_id.borrow().clone() else {
            return;
        };
        let mut cache = self.thread_view_cache.borrow_mut();
        let entry = cache.entry(thread_id).or_default();
        entry.thread = self.active_thread_snapshot.borrow().clone();
        entry.messages = self.active_messages.borrow().clone();
        entry.scroll_value = Some(self.messages_scroll.vadjustment().value());
    }

    fn active_thread_scroll_value(&self) -> Option<f64> {
        let thread_id = self.active_thread_id.borrow().clone()?;
        self.thread_view_cache
            .borrow()
            .get(&thread_id)
            .and_then(|entry| entry.scroll_value)
    }

    fn restore_cached_thread_view(&self) {
        let Some(thread_id) = self.active_thread_id.borrow().clone() else {
            *self.active_thread_snapshot.borrow_mut() = None;
            self.active_messages.borrow_mut().clear();
            return;
        };
        let cache = self.thread_view_cache.borrow();
        if let Some(entry) = cache.get(&thread_id) {
            *self.active_thread_snapshot.borrow_mut() = entry.thread.clone();
            *self.active_messages.borrow_mut() = visible_messages_for_cache(entry);
        } else {
            *self.active_thread_snapshot.borrow_mut() = None;
            self.active_messages.borrow_mut().clear();
        }
    }

    fn restore_active_thread_scroll_after_render(&self) {
        let Some(scroll_value) = self.active_thread_scroll_value() else {
            return;
        };
        let scroll = self.messages_scroll.clone();
        glib::idle_add_local_once(move || {
            let adjustment = scroll.vadjustment();
            let max_value = (adjustment.upper() - adjustment.page_size()).max(0.0);
            adjustment.set_value(scroll_value.clamp(0.0, max_value));
        });
    }

    fn merge_thread_history(&self, thread_id: &str, messages: Vec<MessageDto>, complete: bool) {
        let mut cache = self.thread_view_cache.borrow_mut();
        let entry = cache.entry(thread_id.to_string()).or_default();
        entry.messages = merge_messages(&entry.messages, &messages);
        entry.history_complete = complete;
    }

    fn reveal_cached_history(&self) {
        let Some(thread_id) = self.active_thread_id.borrow().clone() else {
            return;
        };
        let Some(messages) = self
            .thread_view_cache
            .borrow()
            .get(&thread_id)
            .map(|entry| entry.messages.clone())
        else {
            return;
        };
        if messages.len() <= self.active_messages.borrow().len() {
            return;
        }

        let adjustment = self.messages_scroll.vadjustment();
        let old_upper = adjustment.upper();
        let old_value = adjustment.value();
        *self.active_messages.borrow_mut() = messages;
        self.render_messages();
        let scroll = self.messages_scroll.clone();
        glib::idle_add_local_once(move || {
            let adjustment = scroll.vadjustment();
            let delta = adjustment.upper() - old_upper;
            adjustment.set_value((old_value + delta).max(0.0));
        });
    }

    fn apply_streaming_message_update(&self, thread_id: &str, message: MessageDto) {
        {
            let mut cache = self.thread_view_cache.borrow_mut();
            let entry = cache.entry(thread_id.to_string()).or_default();
            upsert_message(&mut entry.messages, message.clone());
            if entry.thread.is_none() {
                entry.thread = self.active_thread_snapshot.borrow().clone();
            }
        }

        if self.active_thread_id.borrow().as_deref() != Some(thread_id) {
            return;
        }

        upsert_message(&mut self.active_messages.borrow_mut(), message.clone());
        if let Some(thread) = self.active_thread_snapshot.borrow_mut().as_mut() {
            thread.status = ThreadStatusDto::Streaming;
        }
        self.refresh_message_widget(&message);
        self.sync_composer_state();
    }

    fn snapshot_is_stale(&self, snapshot: &ViewSnapshot) -> bool {
        let current_workspace_id = self.active_workspace_id.borrow().clone();
        let current_thread_id = self.active_thread_id.borrow().clone();
        snapshot_request_is_stale(
            snapshot.requested_workspace_id.as_deref(),
            snapshot.requested_thread_id.as_deref(),
            current_workspace_id.as_deref(),
            current_thread_id.as_deref(),
        )
    }

    fn sync_active_workspace(&self) {
        let Some(workspace_id) = self.active_workspace_id.borrow().clone() else {
            return;
        };
        self.backend
            .sync_codex_threads_for_workspace_async(workspace_id, self.ui_tx.clone());
    }

    fn render_all(&self) {
        let started = Instant::now();
        self.render_workspaces();
        self.render_threads();
        self.render_thread_tabs();
        self.render_messages();
        self.sync_composer_state();
        log_perf(
            "ui.render_all",
            started,
            format!(
                "workspaces={}, threads={}, messages={}",
                self.workspaces.borrow().len(),
                self.threads.borrow().len(),
                self.active_messages.borrow().len()
            ),
        );
    }

    fn render_workspaces(&self) {
        let active_id = self.active_workspace_id.borrow().clone();
        let query = self.search_entry.text().to_string().to_lowercase();
        let mut visible_ids = Vec::new();
        let mut desired_rows = Vec::new();

        for (index, workspace) in self.workspaces.borrow().iter().enumerate() {
            if !query.is_empty()
                && !workspace.name.to_lowercase().contains(&query)
                && !workspace.root_path.to_lowercase().contains(&query)
            {
                continue;
            }
            visible_ids.push(workspace.id.clone());
            let is_active = active_id.as_deref() == Some(workspace.id.as_str());
            let signature = format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                workspace.id, workspace.name, workspace.root_path, is_active
            );
            let row = {
                let mut cache = self.workspace_row_cache.borrow_mut();
                let entry = cache
                    .entry(workspace.id.clone())
                    .or_insert_with(|| CachedListRow {
                        row: gtk::ListBoxRow::new(),
                        signature: String::new(),
                    });
                if entry.signature != signature {
                    entry.row.set_selectable(false);
                    entry.row.set_activatable(true);
                    entry.row.set_widget_name(&format!("workspace-row-{index}"));
                    let content = row_box(
                        "folder-symbolic",
                        &workspace.name,
                        Some(&workspace.root_path),
                    );
                    content.add_css_class("workspace-row");
                    if is_active {
                        content.add_css_class("active");
                    }
                    entry.row.set_child(Some(&content));
                    entry.signature = signature;
                }
                entry.row.clone()
            };
            desired_rows.push(row);
        }

        self.workspace_row_cache
            .borrow_mut()
            .retain(|workspace_id, _| visible_ids.iter().any(|id| id == workspace_id));
        clear_list_box(&self.workspace_list);
        for row in desired_rows {
            self.workspace_list.append(&row);
        }
        *self.visible_workspace_ids.borrow_mut() = visible_ids;
    }

    fn render_threads(&self) {
        let active_id = self.active_thread_id.borrow().clone();
        let query = self.search_entry.text().to_string().to_lowercase();
        let mut visible_ids = Vec::new();
        let mut desired_rows = Vec::new();

        for thread in self.threads.borrow().iter() {
            if !query.is_empty() && !thread.title.to_lowercase().contains(&query) {
                continue;
            }
            visible_ids.push(thread.id.clone());
            let is_active = active_id.as_deref() == Some(thread.id.as_str());
            let is_running = self.backend.is_running(&thread.id);
            let signature = format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                thread.id,
                thread.title,
                thread.message_count,
                thread.last_activity_at,
                thread.status.as_str(),
                is_active || is_running
            );
            let row = {
                let mut cache = self.thread_row_cache.borrow_mut();
                if cache
                    .get(&thread.id)
                    .is_some_and(|entry| entry.signature == signature)
                {
                    cache.get(&thread.id).map(|entry| entry.row.clone())
                } else {
                    let row = gtk::ListBoxRow::new();
                    row.set_selectable(false);
                    row.set_activatable(true);
                    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    content.add_css_class("thread-row");
                    if is_active {
                        content.add_css_class("active");
                    }

                    let dot = gtk::Box::new(gtk::Orientation::Vertical, 0);
                    dot.add_css_class("status-dot");
                    dot.add_css_class(status_class(&thread.status));
                    dot.set_valign(gtk::Align::Center);
                    content.append(&dot);

                    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
                    labels.set_hexpand(true);
                    let title_text = single_line_text(&thread.title);
                    let title = gtk::Label::new(Some(&title_text));
                    title.add_css_class("row-title");
                    title.set_xalign(0.0);
                    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    title.set_lines(1);
                    title.set_max_width_chars(24);
                    title.set_overflow(gtk::Overflow::Hidden);
                    title.set_single_line_mode(true);
                    title.set_width_chars(1);
                    title.set_wrap(false);
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

                    if is_running {
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
                    cache.insert(
                        thread.id.clone(),
                        CachedListRow {
                            row: row.clone(),
                            signature,
                        },
                    );
                    Some(row)
                }
            };
            if let Some(row) = row {
                desired_rows.push(row);
            }
        }

        self.thread_row_cache
            .borrow_mut()
            .retain(|thread_id, _| visible_ids.iter().any(|id| id == thread_id));
        clear_list_box(&self.thread_list);
        for row in desired_rows {
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
                self.active_thread_snapshot
                    .borrow()
                    .as_ref()
                    .filter(|thread| thread.id == thread_id)
                    .map(|thread| thread.title.clone())
            })
            .unwrap_or_else(|| "Thread".to_string());
        let title = title.trim();
        if title.is_empty() {
            "Thread".to_string()
        } else {
            single_line_text(title)
        }
    }

    fn render_messages(&self) {
        let started = Instant::now();
        self.render_widgets_created.set(0);
        let stick_to_bottom = self.messages_scroll_is_near_bottom();
        clear_box(&self.messages_box);
        let Some(thread_id) = self.active_thread_id.borrow().clone() else {
            self.render_empty("Aucun thread selectionne");
            self.title_label.set_text("SupaCodex");
            self.title_label.set_tooltip_text(None);
            self.subtitle_label.set_text("codex");
            self.title_label.queue_draw();
            self.subtitle_label.queue_draw();
            self.window.queue_draw();
            self.render_runtime_controls(None);
            return;
        };

        let Some(thread) = self.active_thread_snapshot.borrow().clone() else {
            self.render_empty("Chargement du thread...");
            self.render_runtime_controls(None);
            return;
        };
        if thread.id != thread_id {
            self.render_empty("Chargement du thread...");
            self.render_runtime_controls(None);
            return;
        };

        let editing_active_thread = self
            .editing_message
            .borrow()
            .as_ref()
            .is_some_and(|state| state.thread_id == thread.id);
        if !editing_active_thread
            && !self.backend.is_running(&thread.id)
            && codex_transcript_sync_needed(&thread)
        {
            self.backend.sync_codex_thread_transcript_if_needed_async(
                thread.id.clone(),
                self.ui_tx.clone(),
            );
        }

        let header_title = single_line_text(thread.title.trim());
        let header_title = if header_title.is_empty() {
            "Thread".to_string()
        } else {
            header_title
        };
        self.title_label.set_text(&header_title);
        self.title_label.set_tooltip_text(Some(&header_title));
        self.subtitle_label.set_text(&format!(
            "{} - {} - {} tokens",
            thread.engine_id, thread.model_id, thread.total_tokens
        ));
        self.title_label.queue_draw();
        self.subtitle_label.queue_draw();
        self.window.queue_draw();
        self.render_runtime_controls(Some(&thread));
        self.send_button
            .set_icon_name(if self.backend.is_running(&thread.id) {
                "process-stop-symbolic"
            } else {
                "mail-send-symbolic"
            });

        let messages = self.active_messages.borrow().clone();
        if messages.is_empty() {
            self.render_empty("Pret a demarrer une conversation.");
        } else {
            let mut user_turn_index = 0usize;
            for message in messages {
                let message_user_turn_index = if message.role == "user" {
                    let index = user_turn_index;
                    user_turn_index += 1;
                    Some(index)
                } else {
                    None
                };
                self.render_message(&thread, &message, message_user_turn_index);
            }
            if stick_to_bottom {
                self.scroll_messages_to_bottom();
            } else if self.active_thread_scroll_value().is_none() {
                self.scroll_messages_to_bottom();
            } else {
                self.restore_active_thread_scroll_after_render();
            }
        }
        log_perf(
            "ui.render_messages",
            started,
            format!(
                "messages={}, widgets_created={}",
                self.active_messages.borrow().len(),
                self.render_widgets_created.get()
            ),
        );
    }

    fn render_empty(&self, text: &str) {
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 10);
        empty.add_css_class("empty-state");
        self.render_widgets_created
            .set(self.render_widgets_created.get().saturating_add(2));
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

    fn messages_scroll_is_near_bottom(&self) -> bool {
        let adjustment = self.messages_scroll.vadjustment();
        let distance = adjustment.upper() - adjustment.page_size() - adjustment.value();
        distance <= 48.0
    }

    fn scroll_messages_to_bottom(&self) {
        let scroll = self.messages_scroll.clone();
        glib::idle_add_local_once(move || {
            scroll_scrolled_window_to_bottom(&scroll);
            settle_scrolled_window_to_bottom(
                scroll,
                MESSAGE_SCROLL_SETTLE_PASSES,
                MESSAGE_SCROLL_SETTLE_INTERVAL,
            );
        });
    }

    fn render_runtime_controls(&self, thread: Option<&ThreadDto>) {
        let active_profile_id = self.backend.active_codex_profile_id();
        let profiles = self.backend.codex_profiles();
        let workspace_id = thread
            .map(|thread| thread.workspace_id.clone())
            .or_else(|| self.active_workspace_id.borrow().clone());
        let trust_level = if workspace_id.is_some() {
            self.active_trust_level.borrow().clone()
        } else {
            TrustLevelDto::Standard
        };
        let signature = format!(
            "{}\u{1f}{:?}\u{1f}{}",
            active_profile_id,
            workspace_id,
            trust_level.as_str()
        );
        if self.runtime_controls_signature.borrow().as_str() == signature {
            return;
        }
        *self.runtime_controls_signature.borrow_mut() = signature;

        let active_profile_label = profiles
            .iter()
            .find(|profile| profile.id == active_profile_id)
            .map(|profile| display_codex_profile_name(profile))
            .unwrap_or_else(|| "Codex".to_string());
        self.profile_button_label.set_text(&active_profile_label);
        self.profile_button.set_popover(Some(
            &self.build_profile_popover(&profiles, &active_profile_id),
        ));
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

    fn editing_state_for_message(
        &self,
        thread_id: &str,
        message: &MessageDto,
        user_turn_index: Option<usize>,
    ) -> Option<EditingMessageState> {
        let state = self.editing_message.borrow().clone()?;
        if state.thread_id != thread_id || message.role != "user" {
            return None;
        }
        if state.message_id == message.id || user_turn_index == Some(state.user_turn_index) {
            Some(state)
        } else {
            None
        }
    }

    fn render_message(
        &self,
        thread: &ThreadDto,
        message: &MessageDto,
        user_turn_index: Option<usize>,
    ) {
        let Some(widget) = self.build_message_widget(thread, message, user_turn_index) else {
            return;
        };
        self.message_widget_cache
            .borrow_mut()
            .insert(message.id.clone(), widget.clone());
        self.messages_box.append(&widget);
    }

    fn build_message_widget(
        &self,
        thread: &ThreadDto,
        message: &MessageDto,
        user_turn_index: Option<usize>,
    ) -> Option<gtk::Widget> {
        self.render_widgets_created
            .set(self.render_widgets_created.get().saturating_add(1));
        let is_user = message.role == "user";
        let mut editing = self.editing_state_for_message(&thread.id, message, user_turn_index);
        if let Some(editing_state) = editing.as_mut() {
            if editing_state.message_id != message.id {
                editing_state.message_id = message.id.clone();
                if let Some(current) = self.editing_message.borrow_mut().as_mut() {
                    if current.thread_id == thread.id
                        && current.user_turn_index == editing_state.user_turn_index
                    {
                        current.message_id = message.id.clone();
                    }
                }
            }
        }
        let is_editing = editing.is_some();
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
            return None;
        }

        let outer = gtk::Box::new(gtk::Orientation::Vertical, 3);
        outer.set_hexpand(true);
        outer.set_margin_start(if is_user {
            if is_editing {
                96
            } else {
                160
            }
        } else {
            0
        });
        outer.set_margin_end(if is_user { 0 } else { 160 });
        outer.set_halign(if is_editing {
            gtk::Align::Fill
        } else if is_user {
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
        if is_editing {
            card.add_css_class("message-editing");
            card.set_hexpand(true);
        }
        outer.append(&card);

        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toolbar.add_css_class("message-toolbar");
        let author = gtk::Label::new(Some(if is_user { "Vous" } else { &thread.engine_id }));
        author.add_css_class("message-author");
        author.set_xalign(0.0);
        author.set_hexpand(true);
        toolbar.append(&author);
        if is_user && !is_editing {
            let edit_button = gtk::Button::builder()
                .icon_name("document-edit-symbolic")
                .tooltip_text("Modifier et reprendre depuis ce message")
                .build();
            edit_button.add_css_class("message-edit-button");
            edit_button.set_has_frame(false);
            let ui_tx = self.ui_tx.clone();
            let thread_id = thread.id.clone();
            let message_id = message.id.clone();
            let user_turn_index = user_turn_index.unwrap_or_default();
            let content = message_plain_text(message).unwrap_or_default();
            edit_button.connect_clicked(move |_| {
                let _ = ui_tx.send(UiEvent::StartEditMessage {
                    thread_id: thread_id.clone(),
                    message_id: message_id.clone(),
                    user_turn_index,
                    content: content.clone(),
                });
            });
            toolbar.append(&edit_button);
        }
        card.append(&toolbar);

        if let Some(editing_state) = editing {
            self.render_inline_message_editor(&card, editing_state);
        } else if let Some(status_text) = empty_status_text {
            let pending = gtk::Label::new(Some(status_text));
            pending.add_css_class("dim-label");
            pending.set_xalign(0.0);
            card.append(&pending);
        } else if !has_visible_blocks {
            card.append(&message_label(&fallback_text));
        } else {
            for (block_index, block) in blocks.into_iter().enumerate() {
                self.render_block(thread, message, &card, block_index, block);
            }
        }

        Some(outer.upcast::<gtk::Widget>())
    }

    fn refresh_message_widget(&self, message: &MessageDto) {
        let Some(thread) = self.active_thread_snapshot.borrow().clone() else {
            return;
        };
        let user_turn_index = self.user_turn_index_for_message(&message.id);
        let Some(widget) = self.build_message_widget(&thread, message, user_turn_index) else {
            return;
        };
        let stick_to_bottom = self.messages_scroll_is_near_bottom();
        let previous = self
            .message_widget_cache
            .borrow_mut()
            .insert(message.id.clone(), widget.clone());
        if let Some(previous) = previous.filter(|widget| widget.parent().is_some()) {
            let previous_sibling = previous.prev_sibling();
            self.messages_box.remove(&previous);
            self.messages_box
                .insert_child_after(&widget, previous_sibling.as_ref());
        } else {
            self.messages_box.append(&widget);
        }
        if stick_to_bottom {
            self.scroll_messages_to_bottom();
        }
    }

    fn user_turn_index_for_message(&self, message_id: &str) -> Option<usize> {
        let mut user_turn_index = 0usize;
        for message in self.active_messages.borrow().iter() {
            if message.id == message_id {
                return (message.role == "user").then_some(user_turn_index);
            }
            if message.role == "user" {
                user_turn_index += 1;
            }
        }
        None
    }

    fn render_inline_message_editor(&self, parent: &gtk::Box, state: EditingMessageState) {
        let editor = gtk::TextView::new();
        editor.add_css_class("composer-view");
        editor.add_css_class("message-edit-view");
        editor.set_wrap_mode(gtk::WrapMode::WordChar);
        editor.set_vexpand(false);
        editor.set_monospace(false);
        editor.set_top_margin(6);
        editor.set_bottom_margin(6);
        editor.set_left_margin(6);
        editor.set_right_margin(6);
        editor.buffer().set_text(&state.draft);

        let buffer = editor.buffer();
        let ui_tx = self.ui_tx.clone();
        let message_id = state.message_id.clone();
        let user_turn_index = state.user_turn_index;
        buffer.connect_changed(move |buffer| {
            let content = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string();
            let _ = ui_tx.send(UiEvent::UpdateEditMessageDraft {
                message_id: message_id.clone(),
                user_turn_index,
                content,
            });
        });

        let key_controller = gtk::EventControllerKey::new();
        let ui_tx = self.ui_tx.clone();
        let message_id = state.message_id.clone();
        key_controller.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gdk::Key::Escape {
                let _ = ui_tx.send(UiEvent::CancelEditMessage(message_id.clone()));
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Return && modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
                let _ = ui_tx.send(UiEvent::SubmitEditMessage(message_id.clone()));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        editor.add_controller(key_controller);

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(92)
            .max_content_height(240)
            .build();
        scroller.add_css_class("message-edit-scroll");
        scroller.set_hexpand(true);
        scroller.set_child(Some(&editor));
        parent.append(&scroller);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.add_css_class("message-edit-actions");
        actions.set_halign(gtk::Align::End);

        let cancel = gtk::Button::with_label("Annuler");
        let ui_tx = self.ui_tx.clone();
        let message_id = state.message_id.clone();
        cancel.connect_clicked(move |_| {
            let _ = ui_tx.send(UiEvent::CancelEditMessage(message_id.clone()));
        });
        actions.append(&cancel);

        let resume = gtk::Button::with_label("Reprendre");
        resume.add_css_class("suggested-action");
        let ui_tx = self.ui_tx.clone();
        let message_id = state.message_id.clone();
        resume.connect_clicked(move |_| {
            let _ = ui_tx.send(UiEvent::SubmitEditMessage(message_id.clone()));
        });
        actions.append(&resume);
        parent.append(&actions);

        let editor_for_focus = editor.clone();
        glib::idle_add_local_once(move || {
            editor_for_focus.grab_focus();
            let buffer = editor_for_focus.buffer();
            let end = buffer.end_iter();
            buffer.place_cursor(&end);
        });
    }

    fn render_block(
        &self,
        thread: &ThreadDto,
        message: &MessageDto,
        parent: &gtk::Box,
        block_index: usize,
        block: NativeContentBlock,
    ) {
        match block {
            NativeContentBlock::Text { content, .. } => {
                if !content.trim().is_empty() {
                    parent.append(&message_label(&content));
                }
            }
            NativeContentBlock::Thinking { content } => {
                if !content.trim().is_empty() {
                    let key = collapsible_block_key(&message.id, block_index, "thinking");
                    let card = self.collapsible_text_card(
                        key,
                        "Reflexion",
                        None,
                        &content,
                        "reasoning-block",
                        Some("reasoning-text"),
                    );
                    parent.append(&card);
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
                let title = changes_title(&scope);
                let key = collapsible_block_key(&message.id, block_index, "diff");
                let card = self.collapsible_code_card(key, &title, Some("Diff du code"), &diff);
                parent.append(&card);
            }
            NativeContentBlock::Action {
                action_id,
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
                    if let Some(result) = result.as_ref() {
                        body = result
                            .output
                            .clone()
                            .or_else(|| result.error.clone())
                            .unwrap_or_else(|| "Termine".to_string());
                    }
                }
                parent.append(&code_card(&title, &body));

                if let Some(diff) = result
                    .as_ref()
                    .and_then(|result| result.diff.as_deref())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    let key = collapsible_block_key(
                        &message.id,
                        block_index,
                        &format!("action-diff-{action_id}"),
                    );
                    let card = self.collapsible_code_card(key, "Changements", Some(&summary), diff);
                    parent.append(&card);
                }
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

    fn collapsible_text_card(
        &self,
        state_key: String,
        title: &str,
        subtitle: Option<&str>,
        body: &str,
        block_class: &str,
        body_class: Option<&str>,
    ) -> gtk::Box {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let label = message_label(body);
        if let Some(body_class) = body_class {
            label.add_css_class(body_class);
        }
        content.append(&label);
        self.collapsible_block_card(
            state_key,
            title,
            subtitle,
            content.upcast::<gtk::Widget>(),
            block_class,
        )
    }

    fn collapsible_code_card(
        &self,
        state_key: String,
        title: &str,
        subtitle: Option<&str>,
        body: &str,
    ) -> gtk::Box {
        self.collapsible_block_card(
            state_key,
            title,
            subtitle,
            code_output_content(body).upcast::<gtk::Widget>(),
            "changes-block",
        )
    }

    fn collapsible_block_card(
        &self,
        state_key: String,
        title: &str,
        subtitle: Option<&str>,
        child: gtk::Widget,
        block_class: &str,
    ) -> gtk::Box {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("block-card");
        card.add_css_class("collapsible-block");
        card.add_css_class(block_class);

        let expanded = self
            .collapsible_block_expanded
            .borrow()
            .get(&state_key)
            .copied()
            .unwrap_or(true);

        let header_button = gtk::Button::new();
        header_button.add_css_class("collapsible-toggle");
        header_button.set_has_frame(false);
        header_button.set_halign(gtk::Align::Fill);
        header_button.set_hexpand(true);

        let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        header_row.add_css_class("collapsible-title-row");
        header_row.set_halign(gtk::Align::Fill);
        header_row.set_hexpand(true);

        let icon = gtk::Image::from_icon_name(if expanded {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        });
        icon.add_css_class("collapsible-chevron");
        icon.set_pixel_size(12);
        icon.set_valign(gtk::Align::Start);
        header_row.append(&icon);

        let header = collapsible_header(title, subtitle);
        header.set_hexpand(true);
        header_row.append(&header);
        header_button.set_child(Some(&header_row));
        card.append(&header_button);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.add_css_class("collapsible-content");
        content.append(&child);

        let revealer = gtk::Revealer::new();
        revealer.set_transition_type(gtk::RevealerTransitionType::None);
        revealer.set_reveal_child(expanded);
        revealer.set_child(Some(&content));
        card.append(&revealer);

        let expanded_state = Rc::clone(&self.collapsible_block_expanded);
        let revealer_for_toggle = revealer.clone();
        let icon_for_toggle = icon.clone();
        header_button.connect_clicked(move |_| {
            let next = !revealer_for_toggle.property::<bool>("reveal-child");
            revealer_for_toggle.set_reveal_child(next);
            icon_for_toggle.set_icon_name(Some(if next {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            }));
            expanded_state.borrow_mut().insert(state_key.clone(), next);
        });
        card
    }

    fn submit_or_cancel(&self) {
        let active_thread_id = self.active_thread_id.borrow().clone();
        if let Some(thread_id) = active_thread_id.as_ref() {
            if self.backend.is_running(thread_id) {
                self.backend
                    .cancel_turn(thread_id.clone(), self.ui_tx.clone());
                return;
            }
        }

        let workspace_id = self.active_workspace_id.borrow().clone().or_else(|| {
            self.workspaces
                .borrow()
                .first()
                .map(|workspace| workspace.id.clone())
        });
        let Some(workspace_id) = workspace_id else {
            return;
        };

        if active_thread_id.is_none() && self.backend.is_creating_thread(&workspace_id) {
            self.toast("Creation du thread en cours.".to_string());
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

        if let Some(thread_id) = active_thread_id {
            self.backend
                .send_message(thread_id, message, attachments, self.ui_tx.clone());
        } else if !self.backend.create_thread_and_send_message_async(
            workspace_id,
            message,
            attachments,
            self.ui_tx.clone(),
        ) {
            self.toast("Creation du thread en cours.".to_string());
            return;
        }

        buffer.set_text("");
        self.pending_attachments.borrow_mut().clear();
        self.render_attachment_bar();
        self.composer.grab_focus();
    }

    fn request_create_thread(&self) {
        let Some(workspace_id) = self.active_workspace_id.borrow().clone().or_else(|| {
            self.workspaces
                .borrow()
                .first()
                .map(|workspace| workspace.id.clone())
        }) else {
            return;
        };

        if !self
            .backend
            .create_thread_async(workspace_id, self.ui_tx.clone())
        {
            self.toast("Creation du thread en cours.".to_string());
        } else {
            *self.editing_message.borrow_mut() = None;
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
        self.remember_active_thread_scroll();
        *self.editing_message.borrow_mut() = None;
        *self.active_workspace_id.borrow_mut() = Some(workspace_id);
        *self.active_thread_id.borrow_mut() = None;
        *self.active_thread_snapshot.borrow_mut() = None;
        self.active_messages.borrow_mut().clear();
        self.render_all();
        self.request_view_snapshot();
        self.sync_active_workspace();
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
        if self.active_thread_id.borrow().as_deref() != Some(thread_id.as_str()) {
            self.remember_active_thread_scroll();
            *self.editing_message.borrow_mut() = None;
        }
        *self.active_thread_id.borrow_mut() = Some(thread_id);
        self.restore_cached_thread_view();
        self.render_all();
        self.request_view_snapshot();
        self.composer.grab_focus();
    }

    fn refocus_window_after_native_dialog(&self) {
        let window = self.window.clone();
        glib::idle_add_local_once(move || {
            window.present();
        });
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
                if let Some(controller) = weak.upgrade() {
                    controller.refocus_window_after_native_dialog();
                    if let Ok(file) = result {
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
                let Some(controller) = weak.upgrade() else {
                    return;
                };
                controller.refocus_window_after_native_dialog();
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
                controller.add_attachments(paths);
            },
        );
    }

    fn add_attachments(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }

        let mut seen = self
            .pending_attachments
            .borrow()
            .iter()
            .map(|attachment| attachment.file_path.clone())
            .collect::<HashSet<_>>();
        let paths = paths
            .into_iter()
            .filter(|path| seen.insert(path.to_string_lossy().to_string()))
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return;
        }

        let insertion_offset = self.composer.buffer().cursor_position();
        let ui_tx = self.ui_tx.clone();
        self.backend.runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                paths
                    .into_iter()
                    .filter_map(|path| match resolve_pending_attachment(path) {
                        Ok(Some(attachment)) => Some(Ok(attachment)),
                        Ok(None) => None,
                        Err(error) => Some(Err(error)),
                    })
                    .collect::<Vec<_>>()
            })
            .await;

            match result {
                Ok(attachments) => {
                    let _ = ui_tx.send(UiEvent::AttachmentsResolved {
                        insertion_offset,
                        results: attachments,
                    });
                }
                Err(error) => {
                    let _ = ui_tx.send(UiEvent::Toast(format!(
                        "Impossible d'ajouter les pieces jointes: {error}"
                    )));
                }
            }
        });
    }

    fn queue_attachment_thumbnail(&self, attachment: &PendingAttachment) {
        if !is_image_attachment(attachment) {
            return;
        }

        let file_path = attachment.file_path.clone();
        let ui_tx = self.ui_tx.clone();
        self.backend.runtime.spawn(async move {
            let task_file_path = file_path.clone();
            let result =
                tokio::task::spawn_blocking(move || build_attachment_thumbnail(&task_file_path))
                    .await
                    .map_err(|error| format!("Generation du thumbnail interrompue: {error}"))
                    .and_then(|result| result);
            let _ = ui_tx.send(UiEvent::AttachmentThumbnailReady { file_path, result });
        });
    }

    fn render_attachment_bar(&self) {
        clear_box(&self.attachment_bar);
        let attachments = self.pending_attachments.borrow().clone();
        self.attachment_bar.set_visible(!attachments.is_empty());

        for (index, attachment) in attachments.iter().enumerate() {
            if is_image_attachment(attachment) {
                let chip = gtk::Overlay::new();
                chip.add_css_class("attachment-image-chip");
                chip.set_halign(gtk::Align::Start);
                chip.set_hexpand(false);
                chip.set_valign(gtk::Align::Start);
                chip.set_vexpand(false);
                chip.set_size_request(ATTACHMENT_PREVIEW_WIDTH, ATTACHMENT_PREVIEW_HEIGHT);
                chip.set_overflow(gtk::Overflow::Hidden);
                let preview = attachment_preview_widget(attachment);
                chip.set_child(Some(&preview));

                let meta = gtk::Box::new(gtk::Orientation::Vertical, 0);
                meta.add_css_class("attachment-image-meta");
                meta.set_halign(gtk::Align::Start);
                meta.set_valign(gtk::Align::End);
                let title = gtk::Label::new(Some(&attachment.file_name));
                title.add_css_class("attachment-image-title");
                title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                title.set_max_width_chars(10);
                title.set_tooltip_text(Some(&attachment.file_name));
                title.set_xalign(0.0);
                let size = gtk::Label::new(Some(&format_size(attachment.size_bytes)));
                size.add_css_class("attachment-image-size");
                size.set_xalign(0.0);
                meta.append(&title);
                meta.append(&size);
                chip.add_overlay(&meta);

                let remove = attachment_remove_button(index, &self.ui_tx);
                chip.add_overlay(&remove);
                self.attachment_bar.append(&chip);
            } else {
                let chip = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                chip.add_css_class("attachment-chip");
                chip.set_halign(gtk::Align::Start);
                chip.set_hexpand(false);

                let icon = gtk::Image::from_icon_name("text-x-generic-symbolic");
                icon.set_pixel_size(16);
                chip.append(&icon);

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

                chip.append(&attachment_remove_button(index, &self.ui_tx));
                self.attachment_bar.append(&chip);
            }
        }
    }

    fn insert_attachment_mentions(&self, insertion_offset: i32, mentions: &[String]) {
        if mentions.is_empty() {
            return;
        }

        let buffer = self.composer.buffer();
        let offset = insertion_offset.clamp(0, buffer.char_count());
        let iter = buffer.iter_at_offset(offset);
        let before = buffer.text(&buffer.start_iter(), &iter, false).to_string();
        let after = buffer.text(&iter, &buffer.end_iter(), false).to_string();
        let text = attachment_mention_insert_text(&before, &after, mentions);

        let mut insert_iter = buffer.iter_at_offset(offset);
        let tag = attachment_mention_tag(&buffer);
        buffer.insert(&mut insert_iter, &text);
        apply_attachment_mention_tags(&buffer, offset, &text, mentions, &tag);
        buffer.place_cursor(&insert_iter);
        self.composer.grab_focus();
    }

    fn remove_attachment_mention(&self, attachment: &PendingAttachment) {
        let Some(mention) = attachment.mention.as_deref() else {
            return;
        };

        let buffer = self.composer.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        let Some(byte_start) = text.find(mention) else {
            return;
        };

        let byte_end = byte_start + mention.len();
        let mut char_start = text[..byte_start].chars().count();
        let mut char_end = text[..byte_end].chars().count();
        let chars = text.chars().collect::<Vec<_>>();
        if char_end < chars.len() && chars[char_end].is_whitespace() {
            char_end += 1;
        } else if char_start > 0 && chars[char_start - 1].is_whitespace() {
            char_start -= 1;
        }

        let mut start = buffer.iter_at_offset(char_start as i32);
        let mut end = buffer.iter_at_offset(char_end as i32);
        buffer.delete(&mut start, &mut end);
    }

    fn handle_attachment_mention_arrow_key(&self, key: gdk::Key) -> bool {
        if !self.has_attachment_mentions() {
            return false;
        }
        let buffer = self.composer.buffer();
        if buffer.selection_bounds().is_some() {
            return false;
        }

        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        let attachments = self.pending_attachments.borrow().clone();
        let ranges = attachment_mention_ranges(&text, &attachments);
        if ranges.is_empty() {
            return false;
        }

        let cursor_offset = buffer.cursor_position();
        let target = match key {
            gdk::Key::Right => ranges
                .iter()
                .find(|range| {
                    cursor_offset == range.start
                        || (cursor_offset > range.start && cursor_offset < range.end)
                })
                .map(|range| range.end),
            gdk::Key::Left => ranges
                .iter()
                .find(|range| {
                    cursor_offset == range.end
                        || (cursor_offset > range.start && cursor_offset < range.end)
                })
                .map(|range| range.start),
            _ => None,
        };

        let Some(target) = target else {
            return false;
        };
        self.snapping_attachment_mention_cursor.set(true);
        let iter = buffer.iter_at_offset(target);
        buffer.place_cursor(&iter);
        self.snapping_attachment_mention_cursor.set(false);
        true
    }

    fn handle_attachment_mention_delete_key(&self, key: gdk::Key) -> bool {
        if !self.has_attachment_mentions() {
            return false;
        }
        let buffer = self.composer.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        let attachments = self.pending_attachments.borrow().clone();
        let ranges = attachment_mention_ranges(&text, &attachments);
        if ranges.is_empty() {
            return false;
        }

        let mut target_mentions = Vec::new();
        let mut selection_delete_range = None;
        if let Some((selection_start, selection_end)) = buffer.selection_bounds() {
            let start = selection_start.offset().min(selection_end.offset());
            let end = selection_start.offset().max(selection_end.offset());
            if start != end {
                let overlapping = ranges
                    .iter()
                    .filter(|range| start < range.end && end > range.start)
                    .collect::<Vec<_>>();
                if !overlapping.is_empty() {
                    let delete_start = overlapping
                        .iter()
                        .map(|range| range.start)
                        .chain(std::iter::once(start))
                        .min()
                        .unwrap_or(start);
                    let delete_end = overlapping
                        .iter()
                        .map(|range| range.end)
                        .chain(std::iter::once(end))
                        .max()
                        .unwrap_or(end);
                    target_mentions.extend(overlapping.iter().map(|range| range.mention.clone()));
                    selection_delete_range = Some((delete_start, delete_end));
                }
            }
        }

        if target_mentions.is_empty() {
            let cursor_offset = buffer.cursor_position();
            let range = match key {
                gdk::Key::BackSpace => ranges.iter().find(|range| {
                    cursor_offset == range.end
                        || (cursor_offset > range.start && cursor_offset <= range.end)
                }),
                gdk::Key::Delete => ranges.iter().find(|range| {
                    cursor_offset == range.start
                        || (cursor_offset >= range.start && cursor_offset < range.end)
                }),
                _ => None,
            };
            if let Some(range) = range {
                target_mentions.push(range.mention.clone());
            }
        }

        if target_mentions.is_empty() {
            return false;
        }

        target_mentions.sort();
        target_mentions.dedup();
        let mut changed = false;
        if let Some((delete_start, delete_end)) = selection_delete_range {
            for mention in target_mentions {
                let _ = self.remove_pending_attachment_by_mention(&mention);
            }
            let mut start = buffer.iter_at_offset(delete_start);
            let mut end = buffer.iter_at_offset(delete_end);
            buffer.delete(&mut start, &mut end);
            changed = true;
        } else {
            for mention in target_mentions {
                if let Some(attachment) = self.remove_pending_attachment_by_mention(&mention) {
                    self.remove_attachment_mention(&attachment);
                    changed = true;
                }
            }
        }

        if changed {
            self.render_attachment_bar();
            self.sync_composer_state();
        }
        changed
    }

    fn remove_pending_attachment_by_mention(&self, mention: &str) -> Option<PendingAttachment> {
        let mut attachments = self.pending_attachments.borrow_mut();
        let index = attachments
            .iter()
            .position(|attachment| attachment.mention.as_deref() == Some(mention))?;
        Some(attachments.remove(index))
    }

    fn prune_attachments_missing_mentions(&self, text: &str) -> bool {
        let mut attachments = self.pending_attachments.borrow_mut();
        let previous_len = attachments.len();
        attachments.retain(|attachment| {
            attachment
                .mention
                .as_deref()
                .map(|mention| text.contains(mention))
                .unwrap_or(true)
        });
        attachments.len() != previous_len
    }

    fn snap_cursor_out_of_attachment_mention(&self, buffer: &gtk::TextBuffer, offset: i32) {
        if self.snapping_attachment_mention_cursor.get() {
            return;
        }
        if !self.has_attachment_mentions() {
            return;
        }

        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        let attachments = self.pending_attachments.borrow().clone();
        let ranges = attachment_mention_ranges(&text, &attachments);
        let Some(range) = ranges
            .iter()
            .find(|range| offset > range.start && offset < range.end)
        else {
            return;
        };

        let distance_to_start = offset - range.start;
        let distance_to_end = range.end - offset;
        let target = if distance_to_start <= distance_to_end {
            range.start
        } else {
            range.end
        };

        self.snapping_attachment_mention_cursor.set(true);
        let target_iter = buffer.iter_at_offset(target);
        buffer.place_cursor(&target_iter);
        self.snapping_attachment_mention_cursor.set(false);
    }

    fn open_workspace_path(&self, path: &Path) {
        self.backend
            .open_workspace_async(path.to_path_buf(), self.ui_tx.clone());
    }

    fn toggle_sidebar(&self) {
        self.split_view
            .set_show_sidebar(!self.split_view.shows_sidebar());
    }

    fn sync_composer_state(&self) {
        let started = Instant::now();
        let active_thread_id = self.active_thread_id.borrow().clone();
        let running = active_thread_id
            .as_ref()
            .is_some_and(|thread_id| self.backend.is_running(thread_id));
        let buffer = self.composer.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        if self.has_attachment_mentions() && self.prune_attachments_missing_mentions(text.as_str())
        {
            self.render_attachment_bar();
        }
        let has_text = text.as_str().chars().any(|ch| !ch.is_whitespace());
        let has_attachments = !self.pending_attachments.borrow().is_empty();
        let show_send_button = running || has_text || has_attachments;
        let composer_height = composer_height_for_text(text.as_str());
        let composer_line_count = composer_line_count(text.as_str());

        if self.composer_last_running.replace(running) != running {
            self.send_button.set_icon_name(if running {
                "process-stop-symbolic"
            } else {
                "mail-send-symbolic"
            });
            self.send_button.set_tooltip_text(Some(if running {
                "Annuler la generation"
            } else {
                "Envoyer"
            }));
        }
        if self.composer_last_send_enabled.replace(show_send_button) != show_send_button {
            self.send_button
                .set_opacity(if show_send_button { 1.0 } else { 0.0 });
            self.send_button.set_sensitive(show_send_button);
        }
        if self.composer_last_line_count.replace(composer_line_count) != composer_line_count {
            self.composer
                .set_top_margin(if composer_line_count <= 1 { 18 } else { 8 });
            self.composer
                .set_bottom_margin(if composer_line_count <= 1 { 0 } else { 8 });
        }
        if self.composer_last_height.replace(composer_height) != composer_height {
            self.composer_scroll.set_min_content_height(composer_height);
            self.composer_scroll.set_max_content_height(composer_height);
            self.composer_scroll.set_size_request(-1, composer_height);
        }
        log_perf(
            "ui.sync_composer_state",
            started,
            format!(
                "chars={}, height={}, send_enabled={}",
                text.len(),
                composer_height,
                show_send_button
            ),
        );
    }

    fn has_attachment_mentions(&self) -> bool {
        self.pending_attachments
            .borrow()
            .iter()
            .any(|attachment| attachment.mention.is_some())
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
            self.remember_active_thread_scroll();
            *self.editing_message.borrow_mut() = None;
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
            self.restore_cached_thread_view();
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

    fn drain_ui_events(
        &self,
        first_event: UiEvent,
        ui_rx: &mut futures_mpsc::UnboundedReceiver<UiEvent>,
    ) {
        let started = Instant::now();
        let mut events = Vec::with_capacity(16);
        events.push(first_event);
        while let Ok(event) = ui_rx.try_recv() {
            events.push(event);
        }
        let processed = events.len();
        let mut reload_requested = false;
        for event in events {
            match event {
                UiEvent::Reload => {
                    reload_requested = true;
                }
                UiEvent::SyncActiveWorkspace => {
                    self.sync_active_workspace();
                }
                UiEvent::WorkspaceOpened(result) => match result {
                    Ok(workspace) => {
                        *self.editing_message.borrow_mut() = None;
                        *self.active_workspace_id.borrow_mut() = Some(workspace.id);
                        *self.active_thread_id.borrow_mut() = None;
                        *self.active_thread_snapshot.borrow_mut() = None;
                        self.active_messages.borrow_mut().clear();
                        self.render_all();
                        self.request_view_snapshot();
                        self.sync_active_workspace();
                    }
                    Err(error) => self.toast(error),
                },
                UiEvent::ThreadCreated {
                    workspace_id,
                    result,
                } => match result {
                    Ok(thread) => {
                        if self.active_workspace_id.borrow().as_deref()
                            == Some(workspace_id.as_str())
                        {
                            *self.editing_message.borrow_mut() = None;
                            *self.active_thread_id.borrow_mut() = Some(thread.id);
                            *self.active_thread_snapshot.borrow_mut() = None;
                            self.active_messages.borrow_mut().clear();
                            self.render_all();
                            self.request_view_snapshot();
                            self.composer.grab_focus();
                        }
                    }
                    Err(error) => self.toast(error),
                },
                UiEvent::CodexThreadsSynced {
                    workspace_id,
                    result,
                } => {
                    if let Err(error) = result {
                        log::warn!(
                            "failed to sync Codex threads for workspace {workspace_id}: {error}"
                        );
                    }
                    if self.active_workspace_id.borrow().as_deref() == Some(workspace_id.as_str()) {
                        self.request_view_snapshot();
                    }
                }
                UiEvent::CodexTranscriptSynced { thread_id, result } => {
                    if let Err(error) = result {
                        log::warn!(
                            "failed to sync Codex transcript for thread {thread_id}: {error}"
                        );
                    } else if self.active_thread_id.borrow().as_deref() == Some(thread_id.as_str())
                    {
                        self.request_view_snapshot();
                    }
                }
                UiEvent::CodexProfileSet { profile_id, result } => match result {
                    Ok(()) => {
                        log::debug!("active Codex profile set to {profile_id}");
                        self.render_all();
                        self.sync_active_workspace();
                    }
                    Err(error) => self.toast(error),
                },
                UiEvent::WorkspaceTrustSet {
                    workspace_id,
                    result,
                } => match result {
                    Ok(()) => {
                        log::debug!("workspace trust level updated for {workspace_id}");
                        self.request_view_snapshot();
                    }
                    Err(error) => self.toast(error),
                },
                UiEvent::AttachmentsResolved {
                    insertion_offset,
                    results,
                } => {
                    let mut changed = false;
                    let mut mentions = Vec::new();
                    let mut thumbnails_to_queue = Vec::new();
                    let mut attachments = self.pending_attachments.borrow_mut();
                    let mut next_image_index = attachments
                        .iter()
                        .filter(|attachment| is_image_attachment(attachment))
                        .count()
                        + 1;
                    for result in results {
                        match result {
                            Ok(mut attachment) => {
                                if attachments
                                    .iter()
                                    .any(|candidate| candidate.file_path == attachment.file_path)
                                {
                                    continue;
                                }
                                if is_image_attachment(&attachment) {
                                    let mention = format!("[Image #{next_image_index}]");
                                    next_image_index += 1;
                                    attachment.mention = Some(mention.clone());
                                    mentions.push(mention);
                                    thumbnails_to_queue.push(attachment.clone());
                                }
                                attachments.push(attachment);
                                changed = true;
                            }
                            Err(error) => self.toast(error),
                        }
                    }
                    drop(attachments);
                    if changed {
                        self.insert_attachment_mentions(insertion_offset, &mentions);
                        self.render_attachment_bar();
                        for attachment in thumbnails_to_queue {
                            self.queue_attachment_thumbnail(&attachment);
                        }
                        self.sync_composer_state();
                    }
                }
                UiEvent::AttachmentThumbnailReady { file_path, result } => {
                    let mut attachments = self.pending_attachments.borrow_mut();
                    let Some(attachment) = attachments
                        .iter_mut()
                        .find(|attachment| attachment.file_path == file_path)
                    else {
                        continue;
                    };
                    match result {
                        Ok(thumbnail_path) => {
                            attachment.thumbnail_path = Some(thumbnail_path);
                            attachment.thumbnail_failed = false;
                        }
                        Err(error) => {
                            log::warn!(
                                "failed to build attachment thumbnail for {file_path}: {error}"
                            );
                            attachment.thumbnail_failed = true;
                        }
                    }
                    drop(attachments);
                    self.render_attachment_bar();
                }
                UiEvent::ViewSnapshotLoaded(result) => {
                    self.loading_snapshot.set(false);
                    match result {
                        Ok(snapshot) => {
                            if self.snapshot_is_stale(&snapshot) {
                                self.queued_snapshot.set(true);
                            } else {
                                let had_workspace = self.active_workspace_id.borrow().is_some();
                                let history_cursor = snapshot.messages_next_cursor.clone();
                                let history_thread_id = snapshot
                                    .active_thread
                                    .as_ref()
                                    .map(|thread| thread.id.clone());
                                self.apply_view_snapshot(snapshot);
                                self.render_all();
                                if let (Some(thread_id), Some(cursor)) =
                                    (history_thread_id, history_cursor)
                                {
                                    self.backend.load_thread_history_async(
                                        thread_id,
                                        cursor,
                                        self.ui_tx.clone(),
                                    );
                                }
                                if !had_workspace {
                                    self.sync_active_workspace();
                                }
                            }
                        }
                        Err(error) => self.toast(error),
                    }
                    if self.queued_snapshot.replace(false) {
                        self.request_view_snapshot();
                    }
                }
                UiEvent::ThreadHistoryLoaded {
                    thread_id,
                    messages,
                    complete,
                } => {
                    self.merge_thread_history(&thread_id, messages, complete);
                }
                UiEvent::StreamingMessageUpdated { thread_id, message } => {
                    self.apply_streaming_message_update(&thread_id, message);
                }
                UiEvent::SelectThread(thread_id) => {
                    if self.active_thread_id.borrow().as_deref() != Some(thread_id.as_str()) {
                        self.remember_active_thread_scroll();
                        *self.editing_message.borrow_mut() = None;
                    }
                    *self.active_thread_id.borrow_mut() = Some(thread_id);
                    self.restore_cached_thread_view();
                    self.render_all();
                    self.request_view_snapshot();
                }
                UiEvent::OpenThreadTab(thread_id) => {
                    self.open_thread_tab(&thread_id);
                }
                UiEvent::CloseThreadTab(thread_id) => {
                    self.close_thread_tab(&thread_id);
                }
                UiEvent::StartEditMessage {
                    thread_id,
                    message_id,
                    user_turn_index,
                    content,
                } => {
                    *self.editing_message.borrow_mut() = Some(EditingMessageState {
                        thread_id,
                        message_id,
                        user_turn_index,
                        draft: content,
                    });
                    self.render_messages();
                }
                UiEvent::UpdateEditMessageDraft {
                    message_id,
                    user_turn_index,
                    content,
                } => {
                    if let Some(state) = self.editing_message.borrow_mut().as_mut() {
                        if state.message_id == message_id
                            || state.user_turn_index == user_turn_index
                        {
                            state.draft = content;
                        }
                    }
                }
                UiEvent::CancelEditMessage(message_id) => {
                    let should_cancel = self
                        .editing_message
                        .borrow()
                        .as_ref()
                        .is_some_and(|state| state.message_id == message_id);
                    if should_cancel {
                        *self.editing_message.borrow_mut() = None;
                        self.render_messages();
                    }
                }
                UiEvent::SubmitEditMessage(message_id) => {
                    let editing = self
                        .editing_message
                        .borrow()
                        .as_ref()
                        .filter(|state| state.message_id == message_id)
                        .cloned();
                    let Some(editing) = editing else {
                        continue;
                    };
                    if editing.draft.trim().is_empty() {
                        self.toast("Le message modifie ne peut pas etre vide.".to_string());
                        continue;
                    }

                    *self.editing_message.borrow_mut() = None;
                    self.render_messages();
                    self.backend.edit_and_resume(
                        editing.thread_id,
                        editing.message_id,
                        editing.user_turn_index,
                        editing.draft,
                        self.ui_tx.clone(),
                    );
                }
                UiEvent::SetCodexProfile(profile_id) => {
                    if !self
                        .backend
                        .set_active_codex_profile_async(profile_id, self.ui_tx.clone())
                    {
                        self.toast("Changement de profil deja en cours.".to_string());
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
                            if !self.backend.set_workspace_trust_level_async(
                                workspace_id,
                                trust_level,
                                self.ui_tx.clone(),
                            ) {
                                self.toast("Changement de permissions deja en cours.".to_string());
                            }
                        }
                        None => self.toast("Aucun projet actif.".to_string()),
                    }
                }
                UiEvent::RemoveAttachment(index) => {
                    let mut attachments = self.pending_attachments.borrow_mut();
                    let removed = if index < attachments.len() {
                        Some(attachments.remove(index))
                    } else {
                        None
                    };
                    drop(attachments);
                    if let Some(attachment) = removed {
                        self.remove_attachment_mention(&attachment);
                    }
                    self.render_attachment_bar();
                    self.sync_composer_state();
                }
                UiEvent::Toast(message) => self.toast(message),
            }
        }

        if reload_requested {
            self.request_view_snapshot();
        }
        self.ui_tx.mark_processed(processed);
        log_perf(
            "ui.drain_events",
            started,
            format!("events={}, backlog={}", processed, self.ui_tx.backlog_len()),
        );
    }

    fn toast(&self, message: String) {
        self.toast_overlay.add_toast(adw::Toast::new(&message));
    }
}

async fn run_ui_event_loop(
    weak: std::rc::Weak<AppController>,
    mut ui_rx: futures_mpsc::UnboundedReceiver<UiEvent>,
) {
    while let Some(event) = ui_rx.next().await {
        let Some(controller) = weak.upgrade() else {
            break;
        };
        controller.drain_ui_events(event, &mut ui_rx);
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

fn single_line_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn attachment_mention_insert_text(before: &str, after: &str, mentions: &[String]) -> String {
    let mut text = mentions.join(" ");
    if !before.is_empty()
        && !before
            .chars()
            .last()
            .is_some_and(|char| char.is_whitespace())
    {
        text.insert(0, ' ');
    }
    if after.is_empty()
        || !after
            .chars()
            .next()
            .is_some_and(|char| char.is_whitespace())
    {
        text.push(' ');
    }
    text
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttachmentMentionRange {
    mention: String,
    start: i32,
    end: i32,
}

fn attachment_mention_ranges(
    text: &str,
    attachments: &[PendingAttachment],
) -> Vec<AttachmentMentionRange> {
    let mut ranges = attachments
        .iter()
        .filter_map(|attachment| {
            let mention = attachment.mention.as_deref()?;
            let byte_start = text.find(mention)?;
            let start = text[..byte_start].chars().count() as i32;
            let end = start + mention.chars().count() as i32;
            Some(AttachmentMentionRange {
                mention: mention.to_string(),
                start,
                end,
            })
        })
        .collect::<Vec<_>>();
    ranges.sort_by_key(|range| range.start);
    ranges
}

fn attachment_mention_tag(buffer: &gtk::TextBuffer) -> gtk::TextTag {
    if let Some(tag) = buffer.tag_table().lookup(ATTACHMENT_MENTION_TAG_NAME) {
        return tag;
    }

    let background = gdk::RGBA::new(0.12, 0.36, 0.72, 0.44);
    let foreground = gdk::RGBA::new(0.66, 0.86, 1.0, 1.0);
    let tag = gtk::TextTag::builder()
        .name(ATTACHMENT_MENTION_TAG_NAME)
        .background_rgba(&background)
        .background_set(true)
        .foreground_rgba(&foreground)
        .foreground_set(true)
        .editable(false)
        .editable_set(true)
        .weight(700)
        .weight_set(true)
        .scale(0.92)
        .scale_set(true)
        .build();
    buffer.tag_table().add(&tag);
    tag
}

fn apply_attachment_mention_tags(
    buffer: &gtk::TextBuffer,
    insertion_offset: i32,
    inserted_text: &str,
    mentions: &[String],
    tag: &gtk::TextTag,
) {
    let mut search_byte_offset = 0;
    for mention in mentions {
        let Some(relative_byte_start) = inserted_text[search_byte_offset..].find(mention) else {
            continue;
        };
        let byte_start = search_byte_offset + relative_byte_start;
        let byte_end = byte_start + mention.len();
        let start_offset = insertion_offset + inserted_text[..byte_start].chars().count() as i32;
        let end_offset = insertion_offset + inserted_text[..byte_end].chars().count() as i32;
        let start = buffer.iter_at_offset(start_offset);
        let end = buffer.iter_at_offset(end_offset);
        buffer.apply_tag(tag, &start, &end);
        search_byte_offset = byte_end;
    }
}

fn composer_height_for_text(value: &str) -> i32 {
    let line_count = composer_line_count(value);
    let height =
        COMPOSER_SINGLE_LINE_HEIGHT + (line_count.saturating_sub(1) * COMPOSER_LINE_HEIGHT);
    height.min(COMPOSER_MAX_HEIGHT)
}

fn composer_line_count(value: &str) -> i32 {
    value.bytes().filter(|byte| *byte == b'\n').count() as i32 + 1
}

fn snapshot_request_is_stale(
    requested_workspace_id: Option<&str>,
    requested_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    current_thread_id: Option<&str>,
) -> bool {
    requested_workspace_id != current_workspace_id || requested_thread_id != current_thread_id
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
    use super::{
        append_or_replace_diff_block, attachment_mention_insert_text, attachment_mention_ranges,
        compact_title, find_resume_target_index, paths_from_dropped_text, single_line_text,
        snapshot_request_is_stale, NativeContentBlock, PendingAttachment,
    };
    use crate::models::{MessageDto, MessageStatusDto};

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

    #[test]
    fn single_line_text_collapses_title_whitespace() {
        assert_eq!(
            single_line_text("  first line\nsecond\tline  "),
            "first line second line"
        );
    }

    #[test]
    fn attachment_mention_insert_text_preserves_readable_spacing() {
        assert_eq!(
            attachment_mention_insert_text("Comme ici : ", "suite", &["[Image #1]".to_string()]),
            "[Image #1] "
        );
        assert_eq!(
            attachment_mention_insert_text("Comme ici", "suite", &["[Image #1]".to_string()]),
            " [Image #1] "
        );
        assert_eq!(
            attachment_mention_insert_text("", "", &["[Image #1]".to_string()]),
            "[Image #1] "
        );
    }

    #[test]
    fn attachment_mention_ranges_track_tag_offsets() {
        let attachments = vec![PendingAttachment {
            file_name: "image.png".to_string(),
            file_path: "/tmp/image.png".to_string(),
            size_bytes: 12,
            mime_type: Some("image/png".to_string()),
            mention: Some("[Image #1]".to_string()),
            thumbnail_path: None,
            thumbnail_failed: false,
        }];

        assert_eq!(
            attachment_mention_ranges("avant [Image #1] apres", &attachments),
            vec![super::AttachmentMentionRange {
                mention: "[Image #1]".to_string(),
                start: 6,
                end: 16,
            }]
        );
    }

    #[test]
    fn paths_from_dropped_text_accepts_plain_paths_and_file_uris() {
        let temp_path =
            std::env::temp_dir().join(format!("supacodex-drop-test-{}", uuid::Uuid::new_v4()));
        std::fs::write(&temp_path, b"drop").expect("write temp file");
        let uri = format!("file://{}", temp_path.to_string_lossy());

        let paths = paths_from_dropped_text(&format!(
            "# comment\n{}\n{}\nrelative.txt\n",
            temp_path.to_string_lossy(),
            uri
        ));

        assert_eq!(paths, vec![temp_path.clone()]);
        std::fs::remove_file(temp_path).ok();
    }

    #[test]
    fn snapshot_request_staleness_tracks_workspace_and_thread_selection() {
        assert!(!snapshot_request_is_stale(
            Some("workspace-a"),
            Some("thread-a"),
            Some("workspace-a"),
            Some("thread-a")
        ));
        assert!(snapshot_request_is_stale(
            Some("workspace-a"),
            Some("thread-a"),
            Some("workspace-b"),
            Some("thread-a")
        ));
        assert!(snapshot_request_is_stale(
            Some("workspace-a"),
            Some("thread-a"),
            Some("workspace-a"),
            Some("thread-b")
        ));
        assert!(snapshot_request_is_stale(
            None,
            None,
            Some("workspace-a"),
            Some("thread-a")
        ));
    }

    #[test]
    fn resume_target_index_uses_message_id_when_present() {
        let messages = vec![
            test_message("user-a", "user"),
            test_message("assistant-a", "assistant"),
            test_message("user-b", "user"),
        ];

        assert_eq!(find_resume_target_index(&messages, "user-b", 0), Some(2));
    }

    #[test]
    fn resume_target_index_falls_back_to_user_turn_index() {
        let messages = vec![
            test_message("new-user-a", "user"),
            test_message("new-assistant-a", "assistant"),
            test_message("new-user-b", "user"),
        ];

        assert_eq!(
            find_resume_target_index(&messages, "old-user-b", 1),
            Some(2)
        );
    }

    #[test]
    fn diff_updates_replace_existing_scope_snapshot() {
        let mut blocks = Vec::new();
        append_or_replace_diff_block(&mut blocks, "diff --git a/a b/a\n+one", "turn");
        append_or_replace_diff_block(&mut blocks, "diff --git a/a b/a\n+two", "turn");

        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            NativeContentBlock::Diff { diff, scope } => {
                assert_eq!(scope, "turn");
                assert!(diff.contains("+two"));
                assert!(!diff.contains("+one"));
            }
            other => panic!("expected diff block, got {other:?}"),
        }
    }

    #[test]
    fn empty_diff_update_removes_existing_scope_snapshot() {
        let mut blocks = Vec::new();
        append_or_replace_diff_block(&mut blocks, "diff --git a/a b/a\n+one", "turn");
        append_or_replace_diff_block(&mut blocks, "", "turn");

        assert!(blocks.is_empty());
    }

    fn test_message(id: &str, role: &str) -> MessageDto {
        MessageDto {
            id: id.to_string(),
            thread_id: "thread-a".to_string(),
            role: role.to_string(),
            content: Some(id.to_string()),
            blocks: None,
            turn_engine_id: None,
            turn_model_id: None,
            turn_reasoning_effort: None,
            schema_version: 1,
            status: MessageStatusDto::Completed,
            token_usage: None,
            created_at: "2026-05-24 00:00:00".to_string(),
        }
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

fn resolve_pending_attachment(path: PathBuf) -> Result<Option<PendingAttachment>, String> {
    let path_string = path.to_string_lossy().to_string();
    let metadata =
        fs::metadata(&path).map_err(|_| format!("Fichier introuvable: {path_string}"))?;
    if !metadata.is_file() {
        return Ok(None);
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("fichier")
        .to_string();
    Ok(Some(PendingAttachment {
        mime_type: guess_mime_type(&path),
        file_name,
        file_path: path_string,
        size_bytes: metadata.len(),
        mention: None,
        thumbnail_path: None,
        thumbnail_failed: false,
    }))
}

fn paths_from_dropped_text(text: &str) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            let path = if line.starts_with("file://") {
                gio::File::for_uri(line).path()
            } else {
                let path = PathBuf::from(line);
                path.is_absolute().then_some(path)
            }?;

            let key = path.to_string_lossy().to_string();
            (path.exists() && seen.insert(key)).then_some(path)
        })
        .collect()
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

fn build_attachment_thumbnail(file_path: &str) -> Result<String, String> {
    let pixbuf = gdk_pixbuf::Pixbuf::from_file_at_scale(
        file_path,
        ATTACHMENT_PREVIEW_WIDTH,
        ATTACHMENT_PREVIEW_HEIGHT,
        true,
    )
    .map_err(|error| format!("Impossible de lire l'image: {error}"))?;

    let thumbnail_dir = std::env::temp_dir().join("supacodex-thumbnails");
    fs::create_dir_all(&thumbnail_dir)
        .map_err(|error| format!("Impossible de preparer le cache thumbnail: {error}"))?;
    let thumbnail_path = thumbnail_dir.join(format!("{}.png", uuid::Uuid::new_v4()));
    pixbuf
        .savev(&thumbnail_path, "png", &[])
        .map_err(|error| format!("Impossible d'ecrire le thumbnail: {error}"))?;
    Ok(thumbnail_path.to_string_lossy().to_string())
}

fn attachment_preview_widget(attachment: &PendingAttachment) -> gtk::Widget {
    if let Some(thumbnail_path) = attachment.thumbnail_path.as_deref() {
        let picture = gtk::Picture::for_filename(thumbnail_path);
        picture.add_css_class("attachment-thumb");
        picture.set_halign(gtk::Align::Fill);
        picture.set_hexpand(false);
        picture.set_valign(gtk::Align::Fill);
        picture.set_vexpand(false);
        picture.set_size_request(ATTACHMENT_PREVIEW_WIDTH, ATTACHMENT_PREVIEW_HEIGHT);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        return picture.upcast::<gtk::Widget>();
    }

    if attachment.thumbnail_failed {
        return attachment_preview_fallback_widget();
    }

    let loading = gtk::Box::new(gtk::Orientation::Vertical, 0);
    loading.add_css_class("attachment-preview-loading");
    loading.set_halign(gtk::Align::Fill);
    loading.set_hexpand(false);
    loading.set_valign(gtk::Align::Fill);
    loading.set_vexpand(false);
    loading.set_size_request(ATTACHMENT_PREVIEW_WIDTH, ATTACHMENT_PREVIEW_HEIGHT);
    let spinner = gtk::Spinner::new();
    spinner.set_halign(gtk::Align::Center);
    spinner.set_valign(gtk::Align::Center);
    spinner.set_hexpand(true);
    spinner.set_vexpand(true);
    spinner.start();
    loading.append(&spinner);
    loading.upcast::<gtk::Widget>()
}

fn attachment_preview_fallback_widget() -> gtk::Widget {
    let fallback = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fallback.add_css_class("attachment-preview-fallback");
    fallback.set_halign(gtk::Align::Fill);
    fallback.set_hexpand(false);
    fallback.set_valign(gtk::Align::Fill);
    fallback.set_vexpand(false);
    fallback.set_size_request(ATTACHMENT_PREVIEW_WIDTH, ATTACHMENT_PREVIEW_HEIGHT);
    let icon = gtk::Image::from_icon_name("image-x-generic-symbolic");
    icon.set_pixel_size(24);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    icon.set_hexpand(true);
    icon.set_vexpand(true);
    fallback.append(&icon);
    fallback.upcast::<gtk::Widget>()
}

fn attachment_remove_button(index: usize, ui_tx: &UiEventSender) -> gtk::Button {
    let remove = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Retirer")
        .build();
    remove.add_css_class("attachment-remove-button");
    remove.set_halign(gtk::Align::End);
    remove.set_valign(gtk::Align::Start);
    remove.set_has_frame(false);
    let ui_tx = ui_tx.clone();
    remove.connect_clicked(move |_| {
        let _ = ui_tx.send(UiEvent::RemoveAttachment(index));
    });
    remove
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

fn collapsible_header(title: &str, subtitle: Option<&str>) -> gtk::Box {
    let header = gtk::Box::new(gtk::Orientation::Vertical, 1);
    header.add_css_class("collapsible-header");

    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("block-title");
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.set_max_width_chars(88);
    header.append(&title_label);

    if let Some(subtitle) = subtitle.filter(|value| !value.trim().is_empty()) {
        let subtitle_label = gtk::Label::new(Some(subtitle.trim()));
        subtitle_label.add_css_class("block-subtitle");
        subtitle_label.set_xalign(0.0);
        subtitle_label.set_wrap(true);
        subtitle_label.set_max_width_chars(88);
        header.append(&subtitle_label);
    }

    header
}

fn code_card(title: &str, body: &str) -> gtk::Box {
    let card = block_card(title, None);
    let body = body.trim();
    if !body.is_empty() {
        card.append(&code_output_content(body));
    }
    card
}

fn code_output_content(body: &str) -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let body = body.trim();
    if body.is_empty() {
        return content;
    }

    const PREVIEW_CHARS: usize = 2400;
    const HARD_LIMIT_CHARS: usize = 12000;
    let is_large = body.chars().count() > PREVIEW_CHARS;
    let full_text = Rc::new(truncate_display(body, HARD_LIMIT_CHARS));
    let preview_text = if is_large {
        truncate_display(body, PREVIEW_CHARS)
    } else {
        full_text.as_ref().clone()
    };
    let output = gtk::Label::new(Some(&preview_text));
    output.add_css_class("code-output");
    output.set_xalign(0.0);
    output.set_wrap(false);
    output.set_max_width_chars(96);
    output.set_selectable(true);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(48)
        .max_content_height(if is_large { 240 } else { 140 })
        .build();
    scroller.set_child(Some(&output));
    content.append(&scroller);

    if is_large {
        let button = gtk::Button::with_label("Voir plus");
        button.add_css_class("runtime-option");
        button.set_halign(gtk::Align::Start);
        let expanded = Rc::new(Cell::new(false));
        let output_for_toggle = output.clone();
        let full_text_for_toggle = Rc::clone(&full_text);
        let preview_for_toggle = preview_text;
        let expanded_for_toggle = Rc::clone(&expanded);
        button.connect_clicked(move |button| {
            let next = !expanded_for_toggle.get();
            expanded_for_toggle.set(next);
            if next {
                output_for_toggle.set_text(&full_text_for_toggle);
                button.set_label("Reduire");
            } else {
                output_for_toggle.set_text(&preview_for_toggle);
                button.set_label("Voir plus");
            }
        });
        content.append(&button);
    }

    content
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

fn scroll_scrolled_window_to_bottom(scroll: &gtk::ScrolledWindow) {
    let adjustment = scroll.vadjustment();
    let max_value = (adjustment.upper() - adjustment.page_size()).max(0.0);
    adjustment.set_value(max_value);
}

fn settle_scrolled_window_to_bottom(
    scroll: gtk::ScrolledWindow,
    remaining_passes: u8,
    interval: Duration,
) {
    if remaining_passes == 0 {
        return;
    }

    glib::timeout_add_local(interval, move || {
        scroll_scrolled_window_to_bottom(&scroll);
        settle_scrolled_window_to_bottom(
            scroll.clone(),
            remaining_passes.saturating_sub(1),
            interval,
        );
        glib::ControlFlow::Break
    });
}

fn parse_blocks(message: &MessageDto) -> Vec<NativeContentBlock> {
    message
        .blocks
        .as_ref()
        .and_then(|blocks| serde_json::from_value::<Vec<NativeContentBlock>>(blocks.clone()).ok())
        .unwrap_or_default()
}

fn native_blocks_from_transcript_message(
    message: &ThreadTranscriptMessage,
) -> Vec<NativeContentBlock> {
    if message.blocks.is_empty() {
        return vec![NativeContentBlock::Text {
            content: message.content.clone(),
            plan_mode: None,
            is_steer: None,
        }];
    }

    message
        .blocks
        .iter()
        .map(|block| match block {
            ThreadTranscriptBlock::Text { content } => NativeContentBlock::Text {
                content: content.clone(),
                plan_mode: None,
                is_steer: None,
            },
            ThreadTranscriptBlock::Thinking { content } => NativeContentBlock::Thinking {
                content: content.clone(),
            },
            ThreadTranscriptBlock::Diff { diff, scope } => NativeContentBlock::Diff {
                diff: diff.clone(),
                scope: scope.clone(),
            },
        })
        .collect()
}

fn streaming_message_snapshot(
    base: &MessageDto,
    blocks: &[NativeContentBlock],
    status: MessageStatusDto,
    model_id: &str,
) -> anyhow::Result<MessageDto> {
    let mut message = base.clone();
    message.blocks = Some(serde_json::to_value(blocks)?);
    message.status = status;
    if !model_id.trim().is_empty() {
        message.turn_model_id = Some(model_id.to_string());
    }
    Ok(message)
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

fn visible_messages_for_cache(cache: &CachedThreadView) -> Vec<MessageDto> {
    if cache.history_complete || cache.messages.len() <= INITIAL_MESSAGE_WINDOW_LIMIT {
        cache.messages.clone()
    } else {
        cache
            .messages
            .iter()
            .rev()
            .take(INITIAL_MESSAGE_WINDOW_LIMIT)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

fn upsert_message(messages: &mut Vec<MessageDto>, message: MessageDto) {
    if let Some(existing) = messages
        .iter_mut()
        .find(|candidate| candidate.id == message.id)
    {
        *existing = message;
    } else {
        messages.push(message);
        messages.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
    }
}

fn merge_messages(existing: &[MessageDto], incoming: &[MessageDto]) -> Vec<MessageDto> {
    let mut merged = existing.to_vec();
    for message in incoming {
        upsert_message(&mut merged, message.clone());
    }
    merged
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

fn find_resume_target_index(
    messages: &[MessageDto],
    message_id: &str,
    user_turn_index: usize,
) -> Option<usize> {
    messages
        .iter()
        .position(|candidate| candidate.id == message_id)
        .or_else(|| {
            messages
                .iter()
                .enumerate()
                .filter(|(_, candidate)| candidate.role == "user")
                .nth(user_turn_index)
                .map(|(index, _)| index)
        })
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

fn append_or_replace_diff_block(blocks: &mut Vec<NativeContentBlock>, diff: &str, scope: &str) {
    let existing_index = blocks.iter().rposition(|block| {
        matches!(
            block,
            NativeContentBlock::Diff {
                scope: existing_scope,
                ..
            } if existing_scope == scope
        )
    });

    if diff.trim().is_empty() {
        if let Some(index) = existing_index {
            blocks.remove(index);
        }
        return;
    }

    if let Some(index) = existing_index {
        if let Some(NativeContentBlock::Diff {
            diff: existing_diff,
            ..
        }) = blocks.get_mut(index)
        {
            *existing_diff = diff.to_string();
        }
    } else {
        blocks.push(NativeContentBlock::Diff {
            diff: diff.to_string(),
            scope: scope.to_string(),
        });
    }
}

fn diff_scope_label(scope: &DiffScope) -> String {
    match scope {
        DiffScope::Turn => "turn".to_string(),
        DiffScope::File => "file".to_string(),
        DiffScope::Workspace => "workspace".to_string(),
    }
}

fn changes_title(scope: &str) -> String {
    if scope.trim().is_empty() {
        "Changements".to_string()
    } else {
        format!("Changements ({})", scope.trim())
    }
}

fn collapsible_block_key(message_id: &str, block_index: usize, kind: &str) -> String {
    format!("{message_id}:{block_index}:{kind}")
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
    let sync_version = metadata
        .get("codexTranscriptSyncVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if sync_version < CODEX_TRANSCRIPT_SYNC_VERSION {
        return true;
    }

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

fn log_perf(label: &str, started: Instant, detail: impl AsRef<str>) {
    let elapsed = started.elapsed();
    if elapsed >= PERF_WARN_THRESHOLD {
        log::info!("perf.{label}: {:?}; {}", elapsed, detail.as_ref());
    } else {
        log::debug!("perf.{label}: {:?}; {}", elapsed, detail.as_ref());
    }
}
