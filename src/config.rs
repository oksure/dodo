use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeekStart {
    Sunday,
    Monday,
}

impl Default for WeekStart {
    fn default() -> Self {
        WeekStart::Sunday
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferencesConfig {
    #[serde(default)]
    pub week_start: WeekStart,
    #[serde(default = "default_true")]
    pub sound_enabled: bool,
    #[serde(default = "default_sound_interval")]
    pub timer_sound_interval: u32,
    #[serde(default = "default_view_str")]
    pub default_view: String,
    #[serde(default = "default_estimate")]
    pub default_estimate: u32,
}

impl Default for PreferencesConfig {
    fn default() -> Self {
        Self {
            week_start: WeekStart::default(),
            sound_enabled: default_true(),
            timer_sound_interval: default_sound_interval(),
            default_view: default_view_str(),
            default_estimate: default_estimate(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_sound_interval() -> u32 {
    10
}

fn default_view_str() -> String {
    "panes".to_string()
}

fn default_estimate() -> u32 {
    60
}

fn default_digest_time() -> String {
    "07:00".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    #[serde(default)]
    pub enabled: bool,
    pub api_key: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_digest_time")]
    pub digest_time: String,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: None,
            from: None,
            to: None,
            digest_time: default_digest_time(),
        }
    }
}

impl EmailConfig {
    pub fn is_ready(&self) -> bool {
        self.enabled && self.api_key.is_some() && self.from.is_some() && self.to.is_some()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub backup: BackupConfig,
    #[serde(default)]
    pub preferences: PreferencesConfig,
    #[serde(default)]
    pub email: EmailConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub enabled: bool,
    pub turso_url: Option<String>,
    pub turso_token: Option<String>,
    #[serde(default = "default_sync_interval")]
    pub sync_interval: u32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            turso_url: None,
            turso_token: None,
            sync_interval: default_sync_interval(),
        }
    }
}

fn default_sync_interval() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    #[serde(default)]
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    #[serde(default = "default_prefix")]
    pub prefix: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub region: Option<String>,
    #[serde(default = "default_schedule_days")]
    pub schedule_days: u32,
    #[serde(default = "default_max_backups")]
    pub max_backups: u32,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            bucket: None,
            prefix: default_prefix(),
            access_key: None,
            secret_key: None,
            region: None,
            schedule_days: default_schedule_days(),
            max_backups: default_max_backups(),
        }
    }
}

fn default_prefix() -> String {
    "dodo/".to_string()
}

fn default_schedule_days() -> u32 {
    7
}

fn default_max_backups() -> u32 {
    10
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .context("Could not find config directory")?;
        Ok(dir.join("dodo").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config: {}", path.display()))?;
        let mut config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config: {}", path.display()))?;

        // Env var fallbacks
        if config.sync.turso_token.is_none() {
            if let Ok(val) = std::env::var("DODO_TURSO_TOKEN") {
                config.sync.turso_token = Some(val);
            }
        }
        if config.backup.access_key.is_none() {
            if let Ok(val) = std::env::var("DODO_S3_ACCESS_KEY") {
                config.backup.access_key = Some(val);
            }
        }
        if config.backup.secret_key.is_none() {
            if let Ok(val) = std::env::var("DODO_S3_SECRET_KEY") {
                config.backup.secret_key = Some(val);
            }
        }
        if config.email.api_key.is_none() {
            if let Ok(val) = std::env::var("DODO_RESEND_API_KEY") {
                config.email.api_key = Some(val);
            }
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write config: {}", path.display()))?;
        Ok(())
    }
}

impl SyncConfig {
    pub fn is_ready(&self) -> bool {
        self.enabled && self.turso_url.is_some() && self.turso_token.is_some()
    }
}

impl BackupConfig {
    pub fn is_ready(&self) -> bool {
        self.enabled
            && self.endpoint.is_some()
            && self.bucket.is_some()
            && self.access_key.is_some()
            && self.secret_key.is_some()
    }
}
