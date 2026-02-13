use ratatui::style::Color;

use dodo::cli::SortBy;

// ── Color Palette (Catppuccin Mocha-inspired) ────────────────────────

pub(super) const BG_SURFACE: Color = Color::Rgb(49, 50, 68);
pub(super) const FG_TEXT: Color = Color::Rgb(205, 214, 244);
pub(super) const FG_SUBTEXT: Color = Color::Rgb(166, 173, 200);
pub(super) const FG_OVERLAY: Color = Color::Rgb(108, 112, 134);
pub(super) const ACCENT_BLUE: Color = Color::Rgb(137, 180, 250);
pub(super) const ACCENT_GREEN: Color = Color::Rgb(166, 227, 161);
pub(super) const ACCENT_YELLOW: Color = Color::Rgb(249, 226, 175);
pub(super) const ACCENT_RED: Color = Color::Rgb(243, 139, 168);
pub(super) const ACCENT_MAUVE: Color = Color::Rgb(203, 166, 247);
pub(super) const ACCENT_TEAL: Color = Color::Rgb(148, 226, 213);
pub(super) const ACCENT_PEACH: Color = Color::Rgb(250, 179, 135);

pub(super) const SORT_MODES: [SortBy; 3] = [SortBy::Created, SortBy::Modified, SortBy::Title];
pub(super) const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

pub(super) const EDIT_FIELD_LABELS: [&str; 9] = [
    "Title", "Project", "Context", "Tags", "Estimate", "Deadline", "Scheduled", "Priority", "Notes",
];

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ConfigFieldType {
    Boolean,
    String,
    Sensitive,
    Number,
}

pub(super) const CONFIG_FIELD_COUNT: usize = 18;

pub(super) const CONFIG_FIELD_LABELS: [&str; CONFIG_FIELD_COUNT] = [
    "Sync Enabled", "Turso URL", "Turso Token", "Sync Interval",
    "Backup Enabled", "Endpoint", "Bucket", "Prefix",
    "Access Key", "Secret Key", "Region", "Schedule Days", "Max Backups",
    "Week Start",
    "Sound", "Sound Interval", "Default View", "Default Est.",
];

pub(super) const CONFIG_FIELD_HINTS: [&str; CONFIG_FIELD_COUNT] = [
    "Toggle sync on/off",
    "libsql://mydb.turso.io",
    "Your Turso auth token",
    "Minutes between auto-syncs (default: 10)",
    "Toggle backup on/off",
    "https://s3.example.com",
    "my-bucket",
    "dodo/ (optional, default: dodo/)",
    "S3 access key",
    "S3 secret key",
    "us-east-1 (optional, not needed for R2/MinIO)",
    "Days between backups (default: 7)",
    "Max backups to keep (default: 10)",
    "sunday or monday (default: sunday)",
    "Play bell sound on timer and completion",
    "Minutes between timer dings (default: 10)",
    "panes, daily, weekly, or calendar (default: panes)",
    "Default estimate in minutes (default: 60)",
];

pub(super) const TOAST_DURATION_SECS: u64 = 5;
pub(super) const TOAST_ERROR_DURATION_SECS: u64 = 8;

pub(super) const CONFIG_FIELD_TYPES: [ConfigFieldType; CONFIG_FIELD_COUNT] = [
    ConfigFieldType::Boolean, ConfigFieldType::String, ConfigFieldType::Sensitive, ConfigFieldType::Number,
    ConfigFieldType::Boolean, ConfigFieldType::String, ConfigFieldType::String, ConfigFieldType::String,
    ConfigFieldType::Sensitive, ConfigFieldType::Sensitive, ConfigFieldType::String,
    ConfigFieldType::Number, ConfigFieldType::Number,
    ConfigFieldType::String,
    ConfigFieldType::Boolean, ConfigFieldType::Number, ConfigFieldType::String, ConfigFieldType::Number,
];
