use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::runtime_env;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub ui: UiConfig,
    pub debug: DebugConfig,
    pub power: PowerConfig,
    pub codex: CodexConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub theme: String,
    pub default_engine: String,
    pub default_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_accelerated_rendering: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_notifications: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_notifications: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_sound: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub sidebar_width: u32,
    pub git_panel_width: u32,
    pub font_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugConfig {
    pub persist_engine_event_logs: bool,
    pub max_action_output_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerConfig {
    pub keep_awake_enabled: bool,
    pub prevent_display_sleep: bool,
    pub prevent_screen_saver: bool,
    pub ac_only_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_threshold: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_duration_secs: Option<u64>,
    pub prevent_closed_display_sleep: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CodexConfig {
    pub active_profile_id: String,
    pub profiles: Vec<CodexProfileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexProfileConfig {
    pub id: String,
    pub name: String,
    pub codex_home: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            default_engine: "codex".to_string(),
            default_model: "gpt-5.3-codex".to_string(),
            locale: None,
            terminal_accelerated_rendering: None,
            chat_notifications: None,
            terminal_notifications: None,
            notification_sound: None,
        }
    }
}

impl AppConfig {
    /// Resolve the configured notification sound name.
    /// Returns `None` if explicitly set to `"none"`, the stored value if set,
    /// or the platform default (`"Glass"` on macOS) otherwise.
    pub fn notification_sound(&self) -> Option<&str> {
        match self.general.notification_sound.as_deref() {
            Some("none") => None,
            Some(name) => Some(name),
            None => default_notification_sound(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            sidebar_width: 260,
            git_panel_width: 380,
            font_size: 13,
        }
    }
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            persist_engine_event_logs: false,
            max_action_output_chars: 20_000,
        }
    }
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            keep_awake_enabled: false,
            prevent_display_sleep: false,
            prevent_screen_saver: false,
            ac_only_mode: false,
            battery_threshold: None,
            session_duration_secs: None,
            prevent_closed_display_sleep: false,
        }
    }
}

fn default_codex_home_path() -> String {
    runtime_env::home_dir()
        .map(|home| home.join(".codex").to_string_lossy().to_string())
        .unwrap_or_else(|| ".codex".to_string())
}

fn default_codex_profile() -> CodexProfileConfig {
    CodexProfileConfig {
        id: "default".to_string(),
        name: "Default (.codex)".to_string(),
        codex_home: default_codex_home_path(),
    }
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            active_profile_id: "default".to_string(),
            profiles: vec![default_codex_profile()],
        }
    }
}

impl CodexConfig {
    pub fn normalize(&mut self) {
        let mut normalized = Vec::new();
        let mut seen_ids = std::collections::BTreeSet::new();
        let mut seen_paths = std::collections::BTreeSet::new();

        for profile in self.profiles.drain(..) {
            let id = profile.id.trim();
            let name = profile.name.trim();
            let codex_home = profile.codex_home.trim();
            if id.is_empty()
                || name.is_empty()
                || codex_home.is_empty()
                || !seen_ids.insert(id.to_string())
            {
                continue;
            }
            let canonical_home = canonicalize_codex_home_or_original(codex_home);
            if !seen_paths.insert(canonical_home) {
                continue;
            }
            normalized.push(CodexProfileConfig {
                id: id.to_string(),
                name: name.to_string(),
                codex_home: codex_home.to_string(),
            });
        }

        if let Some(default_profile) = normalized
            .iter_mut()
            .find(|profile| profile.id == "default")
        {
            if default_profile.name.trim().is_empty() {
                default_profile.name = "Default (.codex)".to_string();
            }
            if default_profile.codex_home.trim().is_empty() {
                default_profile.codex_home = default_codex_home_path();
            }
        } else {
            normalized.insert(0, default_codex_profile());
        }

        if normalized.is_empty() {
            normalized.push(default_codex_profile());
        }

        for profile in discover_local_codex_profiles() {
            let canonical_home = canonicalize_codex_home_or_original(&profile.codex_home);
            if !seen_ids.insert(profile.id.clone()) || !seen_paths.insert(canonical_home) {
                continue;
            }
            normalized.push(profile);
        }

        if !normalized
            .iter()
            .any(|profile| profile.id == self.active_profile_id)
        {
            self.active_profile_id = normalized[0].id.clone();
        }

        self.profiles = normalized;
    }

    pub fn active_profile(&self) -> &CodexProfileConfig {
        self.profile_by_id(&self.active_profile_id)
            .unwrap_or_else(|| {
                self.profiles
                    .first()
                    .expect("codex profiles should never be empty")
            })
    }

    pub fn profile_by_id(&self, profile_id: &str) -> Option<&CodexProfileConfig> {
        self.profiles
            .iter()
            .find(|profile| profile.id == profile_id)
    }
}

fn canonicalize_codex_home_or_original(path: &str) -> String {
    fs::canonicalize(path)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.trim().to_string())
}

fn discover_local_codex_profiles() -> Vec<CodexProfileConfig> {
    let Some(home_dir) = runtime_env::home_dir() else {
        return Vec::new();
    };

    let entries = match fs::read_dir(home_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with(".codex") || file_name == ".codex" || file_name == ".codex-shared"
        {
            continue;
        }

        let path = entry.path();
        if !path.is_dir() || !path.join("state_5.sqlite").is_file() {
            continue;
        }

        let id = file_name.trim_start_matches('.').trim().to_string();
        if id.is_empty() {
            continue;
        }

        profiles.push(CodexProfileConfig {
            id: id.clone(),
            name: file_name.to_string(),
            codex_home: path.to_string_lossy().to_string(),
        });
    }

    profiles.sort_by(|left, right| left.id.cmp(&right.id));
    profiles
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            ui: UiConfig::default(),
            debug: DebugConfig::default(),
            power: PowerConfig::default(),
            codex: CodexConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn terminal_accelerated_rendering_enabled(&self) -> bool {
        self.general.terminal_accelerated_rendering.unwrap_or(true)
    }

    pub fn chat_notifications_enabled(&self) -> bool {
        self.general.chat_notifications.unwrap_or(false)
    }

    pub fn terminal_notifications_enabled(&self) -> bool {
        self.general.terminal_notifications.unwrap_or(false)
    }

    pub fn load_or_create() -> anyhow::Result<Self> {
        let _guard = lock_config()?;
        Self::load_or_create_unlocked()
    }

    #[allow(dead_code)]
    pub fn save(&self) -> anyhow::Result<()> {
        let _guard = lock_config()?;
        self.save_unlocked()
    }

    pub fn mutate<T>(f: impl FnOnce(&mut Self) -> anyhow::Result<T>) -> anyhow::Result<T> {
        let _guard = lock_config()?;
        let mut config = Self::load_or_create_unlocked()?;
        let result = f(&mut config)?;
        config.save_unlocked()?;
        Ok(result)
    }

    fn load_or_create_unlocked() -> anyhow::Result<Self> {
        runtime_env::migrate_legacy_app_data_dir()
            .context("failed to migrate legacy app data dir")?;
        let path = Self::path();

        if !path.exists() {
            let mut config = Self::default();
            config.codex.normalize();
            config.save_unlocked()?;
            return Ok(config);
        }

        let raw = fs::read_to_string(&path)?;
        let mut config = toml::from_str::<Self>(&raw).unwrap_or_default();
        config.codex.normalize();
        Ok(config)
    }

    fn save_unlocked(&self) -> anyhow::Result<()> {
        let mut normalized = self.clone();
        normalized.codex.normalize();
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let raw = toml::to_string_pretty(&normalized)?;
        let temp_path = path.with_extension("toml.tmp");
        fs::write(&temp_path, raw)?;
        replace_file(&temp_path, &path)?;
        Ok(())
    }

    pub fn path() -> PathBuf {
        runtime_env::app_data_dir().join("config.toml")
    }
}

fn default_notification_sound() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        return Some("Glass");
    }

    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn config_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_config() -> anyhow::Result<MutexGuard<'static, ()>> {
    config_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("config lock poisoned"))
}

#[cfg(test)]
pub(crate) fn app_data_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn replace_file(temp_path: &std::path::Path, path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        // Windows does not support atomic rename-over-existing. Use a backup
        // strategy: rename the existing file to .bak, rename the new file into
        // place, then remove .bak.  A crash between steps 1 and 2 leaves the
        // .bak file as a recoverable copy.
        if path.exists() {
            let backup = path.with_extension("toml.bak");
            // Clean up any stale backup from a prior interrupted save.
            let _ = fs::remove_file(&backup);
            match fs::rename(path, &backup) {
                Ok(()) => {
                    if let Err(error) = fs::rename(temp_path, path) {
                        // Restore the backup so the original config is preserved.
                        let _ = fs::rename(&backup, path);
                        return Err(error);
                    }
                    let _ = fs::remove_file(&backup);
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    // File vanished between exists() and rename — proceed.
                }
                Err(error) => return Err(error),
            }
        }
    }

    fs::rename(temp_path, path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::AppConfig;
    use uuid::Uuid;

    const APP_DATA_ENV_VARS: [&str; 4] = ["HOME", "USERPROFILE", "LOCALAPPDATA", "APPDATA"];

    fn with_temp_app_data_env<T>(f: impl FnOnce() -> T) -> T {
        let _guard = super::app_data_env_lock()
            .lock()
            .expect("env lock poisoned");
        let previous: Vec<(&str, Option<std::ffi::OsString>)> = APP_DATA_ENV_VARS
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect();
        let root =
            std::env::temp_dir().join(format!("supacodex-app-config-home-{}", Uuid::new_v4()));
        let local_app_data = root.join("AppData").join("Local");
        let roaming_app_data = root.join("AppData").join("Roaming");
        fs::create_dir_all(&local_app_data).expect("temp local app data should exist");
        fs::create_dir_all(&roaming_app_data).expect("temp roaming app data should exist");
        std::env::set_var("HOME", &root);
        std::env::set_var("USERPROFILE", &root);
        std::env::set_var("LOCALAPPDATA", &local_app_data);
        std::env::set_var("APPDATA", &roaming_app_data);
        let result = f();
        for (key, value) in previous {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        let _ = fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn missing_locale_field_uses_none() {
        let raw = r#"
[general]
theme = "dark"
default_engine = "codex"
default_model = "gpt-5.3-codex"

[ui]
sidebar_width = 260
git_panel_width = 380
font_size = 13

[debug]
persist_engine_event_logs = false
max_action_output_chars = 20000
"#;

        let config = toml::from_str::<AppConfig>(raw).expect("config should deserialize");

        assert_eq!(config.general.locale, None);
        assert!(!config.power.keep_awake_enabled);
        assert_eq!(config.general.terminal_accelerated_rendering, None);
        assert_eq!(config.general.terminal_notifications, None);
        assert!(!config.power.prevent_display_sleep);
        assert!(!config.power.prevent_screen_saver);
        assert!(!config.power.ac_only_mode);
        assert_eq!(config.power.battery_threshold, None);
        assert_eq!(config.power.session_duration_secs, None);
        assert!(!config.power.prevent_closed_display_sleep);
    }

    #[test]
    fn default_config_omits_optional_general_fields_from_toml() {
        let raw = toml::to_string_pretty(&AppConfig::default()).expect("config should serialize");

        assert!(!raw.contains("locale"));
        assert!(raw.contains("[power]"));
        assert!(raw.contains("keep_awake_enabled = false"));
        assert!(!raw.contains("terminal_accelerated_rendering"));
        assert!(!raw.contains("terminal_notifications"));
    }

    #[test]
    fn save_overwrites_existing_config() {
        with_temp_app_data_env(|| {
            let mut config = AppConfig::default();
            config.general.locale = Some("en".to_string());
            config.save().expect("initial config save should succeed");

            let mut updated = AppConfig::load_or_create().expect("config should reload");
            updated.general.locale = Some("pt-BR".to_string());
            updated.power.keep_awake_enabled = true;
            updated.save().expect("updated config save should succeed");

            let saved = AppConfig::load_or_create().expect("config should reload after overwrite");
            assert_eq!(saved.general.locale.as_deref(), Some("pt-BR"));
            assert!(saved.power.keep_awake_enabled);
        });
    }

    #[test]
    fn terminal_accelerated_rendering_defaults_to_enabled() {
        let config = AppConfig::default();

        assert!(config.terminal_accelerated_rendering_enabled());
    }

    #[test]
    fn terminal_notifications_default_to_disabled() {
        let config = AppConfig::default();

        assert!(!config.terminal_notifications_enabled());
    }

    #[test]
    fn new_power_fields_serialize_roundtrip() {
        let mut config = AppConfig::default();
        config.power.prevent_display_sleep = true;
        config.power.prevent_screen_saver = true;
        config.power.ac_only_mode = true;
        config.power.battery_threshold = Some(20);
        config.power.session_duration_secs = Some(3600);
        config.power.prevent_closed_display_sleep = true;

        let raw = toml::to_string_pretty(&config).expect("config should serialize");
        let loaded = toml::from_str::<AppConfig>(&raw).expect("config should deserialize");

        assert!(loaded.power.prevent_display_sleep);
        assert!(loaded.power.prevent_screen_saver);
        assert!(loaded.power.ac_only_mode);
        assert_eq!(loaded.power.battery_threshold, Some(20));
        assert_eq!(loaded.power.session_duration_secs, Some(3600));
        assert!(loaded.power.prevent_closed_display_sleep);
    }

    #[test]
    fn config_without_optional_power_fields_loads() {
        let raw = r#"
[general]
theme = "dark"
default_engine = "codex"
default_model = "gpt-5.3-codex"

[ui]
sidebar_width = 260
git_panel_width = 380
font_size = 13

[debug]
persist_engine_event_logs = false
max_action_output_chars = 20000

[power]
keep_awake_enabled = true
"#;

        let config = toml::from_str::<AppConfig>(raw).expect("config should deserialize");

        assert!(config.power.keep_awake_enabled);
        assert!(!config.power.prevent_display_sleep);
        assert!(!config.power.prevent_screen_saver);
        assert!(!config.power.ac_only_mode);
        assert_eq!(config.power.battery_threshold, None);
        assert_eq!(config.power.session_duration_secs, None);
        assert!(!config.power.prevent_closed_display_sleep);
    }
}
