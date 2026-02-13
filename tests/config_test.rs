use dodo::config::{BackupConfig, Config, PreferencesConfig, SyncConfig, WeekStart};

// ── Default values ──────────────────────────────────────────────────

#[test]
fn config_default_has_sync_disabled() {
    let config = Config::default();
    assert!(!config.sync.enabled);
    assert!(config.sync.turso_url.is_none());
    assert!(config.sync.turso_token.is_none());
}

#[test]
fn config_default_has_backup_disabled() {
    let config = Config::default();
    assert!(!config.backup.enabled);
    assert!(config.backup.endpoint.is_none());
    assert!(config.backup.bucket.is_none());
    assert!(config.backup.access_key.is_none());
    assert!(config.backup.secret_key.is_none());
    assert!(config.backup.region.is_none());
}

#[test]
fn backup_default_prefix() {
    let config: BackupConfig = toml::from_str("").unwrap();
    assert_eq!(config.prefix, "dodo/");
}

#[test]
fn backup_default_schedule_days() {
    let config: BackupConfig = toml::from_str("").unwrap();
    assert_eq!(config.schedule_days, 7);
}

#[test]
fn backup_default_max_backups() {
    let config: BackupConfig = toml::from_str("").unwrap();
    assert_eq!(config.max_backups, 10);
}

// ── TOML parsing ────────────────────────────────────────────────────

#[test]
fn parse_full_config() {
    let toml = r#"
[sync]
enabled = true
turso_url = "libsql://db.turso.io"
turso_token = "secret123"

[backup]
enabled = true
endpoint = "https://s3.example.com"
bucket = "my-bucket"
prefix = "backups/"
access_key = "AKID"
secret_key = "SKEY"
region = "eu-west-1"
schedule_days = 3
max_backups = 5
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert!(config.sync.enabled);
    assert_eq!(config.sync.turso_url.as_deref(), Some("libsql://db.turso.io"));
    assert_eq!(config.sync.turso_token.as_deref(), Some("secret123"));
    assert!(config.backup.enabled);
    assert_eq!(config.backup.endpoint.as_deref(), Some("https://s3.example.com"));
    assert_eq!(config.backup.bucket.as_deref(), Some("my-bucket"));
    assert_eq!(config.backup.prefix, "backups/");
    assert_eq!(config.backup.access_key.as_deref(), Some("AKID"));
    assert_eq!(config.backup.secret_key.as_deref(), Some("SKEY"));
    assert_eq!(config.backup.region.as_deref(), Some("eu-west-1"));
    assert_eq!(config.backup.schedule_days, 3);
    assert_eq!(config.backup.max_backups, 5);
}

#[test]
fn parse_sync_only_config() {
    let toml = r#"
[sync]
enabled = true
turso_url = "libsql://db.turso.io"
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert!(config.sync.enabled);
    assert!(!config.backup.enabled);
    assert_eq!(config.backup.prefix, "dodo/");
}

#[test]
fn parse_backup_only_config() {
    let toml = r#"
[backup]
enabled = true
endpoint = "https://s3.example.com"
bucket = "b"
access_key = "ak"
secret_key = "sk"
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert!(!config.sync.enabled);
    assert!(config.backup.enabled);
    assert_eq!(config.backup.prefix, "dodo/");
    assert_eq!(config.backup.schedule_days, 7);
}

#[test]
fn parse_empty_config() {
    let config: Config = toml::from_str("").unwrap();
    assert!(!config.sync.enabled);
    assert!(!config.backup.enabled);
}

#[test]
fn parse_config_ignores_unknown_fields() {
    let toml = r#"
[sync]
enabled = false
unknown_field = "ignored"

[some_other_section]
key = "value"
"#;
    // serde default is deny_unknown_fields off, so this should work
    // unless strict mode is on — in which case we'd need to fix config.rs
    let result: Result<Config, _> = toml::from_str(toml);
    // If this fails, it means config.rs is too strict with unknown fields
    assert!(result.is_ok(), "Config should ignore unknown fields");
}

// ── SyncConfig::is_ready() ──────────────────────────────────────────

#[test]
fn sync_not_ready_when_disabled() {
    let config = SyncConfig {
        enabled: false,
        turso_url: Some("url".into()),
        turso_token: Some("token".into()),
        ..Default::default()
    };
    assert!(!config.is_ready());
}

#[test]
fn sync_not_ready_without_url() {
    let config = SyncConfig {
        enabled: true,
        turso_url: None,
        turso_token: Some("token".into()),
        ..Default::default()
    };
    assert!(!config.is_ready());
}

#[test]
fn sync_not_ready_without_token() {
    let config = SyncConfig {
        enabled: true,
        turso_url: Some("url".into()),
        turso_token: None,
        ..Default::default()
    };
    assert!(!config.is_ready());
}

#[test]
fn sync_ready_when_all_set() {
    let config = SyncConfig {
        enabled: true,
        turso_url: Some("url".into()),
        turso_token: Some("token".into()),
        ..Default::default()
    };
    assert!(config.is_ready());
}

// ── BackupConfig::is_ready() ────────────────────────────────────────

#[test]
fn backup_not_ready_when_disabled() {
    let config = BackupConfig {
        enabled: false,
        endpoint: Some("ep".into()),
        bucket: Some("b".into()),
        access_key: Some("ak".into()),
        secret_key: Some("sk".into()),
        ..Default::default()
    };
    assert!(!config.is_ready());
}

#[test]
fn backup_not_ready_without_endpoint() {
    let config = BackupConfig {
        enabled: true,
        endpoint: None,
        bucket: Some("b".into()),
        access_key: Some("ak".into()),
        secret_key: Some("sk".into()),
        ..Default::default()
    };
    assert!(!config.is_ready());
}

#[test]
fn backup_not_ready_without_bucket() {
    let config = BackupConfig {
        enabled: true,
        endpoint: Some("ep".into()),
        bucket: None,
        access_key: Some("ak".into()),
        secret_key: Some("sk".into()),
        ..Default::default()
    };
    assert!(!config.is_ready());
}

#[test]
fn backup_not_ready_without_access_key() {
    let config = BackupConfig {
        enabled: true,
        endpoint: Some("ep".into()),
        bucket: Some("b".into()),
        access_key: None,
        secret_key: Some("sk".into()),
        ..Default::default()
    };
    assert!(!config.is_ready());
}

#[test]
fn backup_not_ready_without_secret_key() {
    let config = BackupConfig {
        enabled: true,
        endpoint: Some("ep".into()),
        bucket: Some("b".into()),
        access_key: Some("ak".into()),
        secret_key: None,
        ..Default::default()
    };
    assert!(!config.is_ready());
}

#[test]
fn backup_ready_when_all_set() {
    let config = BackupConfig {
        enabled: true,
        endpoint: Some("ep".into()),
        bucket: Some("b".into()),
        access_key: Some("ak".into()),
        secret_key: Some("sk".into()),
        ..Default::default()
    };
    assert!(config.is_ready());
}

#[test]
fn backup_ready_without_optional_region() {
    let config = BackupConfig {
        enabled: true,
        endpoint: Some("ep".into()),
        bucket: Some("b".into()),
        access_key: Some("ak".into()),
        secret_key: Some("sk".into()),
        region: None,
        ..Default::default()
    };
    assert!(config.is_ready());
}

// ── Config round-trip (serialize + deserialize) ─────────────────────

#[test]
fn config_serialize_deserialize_roundtrip() {
    let config = Config {
        sync: SyncConfig {
            enabled: true,
            turso_url: Some("libsql://test.turso.io".into()),
            turso_token: Some("tok".into()),
            ..Default::default()
        },
        backup: BackupConfig {
            enabled: true,
            endpoint: Some("https://s3.example.com".into()),
            bucket: Some("bkt".into()),
            prefix: "custom/".into(),
            access_key: Some("AK".into()),
            secret_key: Some("SK".into()),
            region: Some("us-west-2".into()),
            schedule_days: 14,
            max_backups: 20,
        },
        preferences: PreferencesConfig::default(),
    };

    let serialized = toml::to_string_pretty(&config).unwrap();
    let deserialized: Config = toml::from_str(&serialized).unwrap();

    assert_eq!(deserialized.sync.enabled, config.sync.enabled);
    assert_eq!(deserialized.sync.turso_url, config.sync.turso_url);
    assert_eq!(deserialized.backup.prefix, "custom/");
    assert_eq!(deserialized.backup.schedule_days, 14);
    assert_eq!(deserialized.backup.max_backups, 20);
}

// ── PreferencesConfig ───────────────────────────────────────────────

#[test]
fn preferences_default_week_start_sunday() {
    let config = Config::default();
    assert_eq!(config.preferences.week_start, WeekStart::Sunday);
}

#[test]
fn preferences_parse_week_start_monday() {
    let toml = r#"
[preferences]
week_start = "monday"
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.preferences.week_start, WeekStart::Monday);
}

#[test]
fn preferences_parse_week_start_sunday() {
    let toml = r#"
[preferences]
week_start = "sunday"
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.preferences.week_start, WeekStart::Sunday);
}

#[test]
fn preferences_missing_defaults_to_sunday() {
    let toml = "";
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.preferences.week_start, WeekStart::Sunday);
}

#[test]
fn preferences_roundtrip() {
    let config = Config {
        sync: SyncConfig::default(),
        backup: BackupConfig::default(),
        preferences: PreferencesConfig {
            week_start: WeekStart::Monday,
            ..Default::default()
        },
    };
    let serialized = toml::to_string_pretty(&config).unwrap();
    let deserialized: Config = toml::from_str(&serialized).unwrap();
    assert_eq!(deserialized.preferences.week_start, WeekStart::Monday);
}

// ── New preferences fields ──────────────────────────────────────────

#[test]
fn preferences_default_sound_enabled() {
    let config = Config::default();
    assert!(config.preferences.sound_enabled);
}

#[test]
fn preferences_default_timer_sound_interval() {
    let config = Config::default();
    assert_eq!(config.preferences.timer_sound_interval, 10);
}

#[test]
fn preferences_default_view_panes() {
    let config = Config::default();
    assert_eq!(config.preferences.default_view, "panes");
}

#[test]
fn preferences_default_estimate_60() {
    let config = Config::default();
    assert_eq!(config.preferences.default_estimate, 60);
}

#[test]
fn preferences_parse_sound_disabled() {
    let toml = r#"
[preferences]
sound_enabled = false
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert!(!config.preferences.sound_enabled);
}

#[test]
fn preferences_parse_timer_sound_interval() {
    let toml = r#"
[preferences]
timer_sound_interval = 5
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.preferences.timer_sound_interval, 5);
}

#[test]
fn preferences_parse_default_view() {
    let toml = r#"
[preferences]
default_view = "daily"
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.preferences.default_view, "daily");
}

#[test]
fn preferences_parse_default_estimate() {
    let toml = r#"
[preferences]
default_estimate = 30
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.preferences.default_estimate, 30);
}

#[test]
fn preferences_missing_new_fields_get_defaults() {
    let toml = r#"
[preferences]
week_start = "monday"
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.preferences.week_start, WeekStart::Monday);
    assert!(config.preferences.sound_enabled);
    assert_eq!(config.preferences.timer_sound_interval, 10);
    assert_eq!(config.preferences.default_view, "panes");
    assert_eq!(config.preferences.default_estimate, 60);
}

#[test]
fn preferences_full_roundtrip() {
    let config = Config {
        sync: SyncConfig::default(),
        backup: BackupConfig::default(),
        preferences: PreferencesConfig {
            week_start: WeekStart::Monday,
            sound_enabled: false,
            timer_sound_interval: 15,
            default_view: "calendar".to_string(),
            default_estimate: 45,
        },
    };
    let serialized = toml::to_string_pretty(&config).unwrap();
    let deserialized: Config = toml::from_str(&serialized).unwrap();
    assert_eq!(deserialized.preferences.week_start, WeekStart::Monday);
    assert!(!deserialized.preferences.sound_enabled);
    assert_eq!(deserialized.preferences.timer_sound_interval, 15);
    assert_eq!(deserialized.preferences.default_view, "calendar");
    assert_eq!(deserialized.preferences.default_estimate, 45);
}
