use clap::{Parser, Subcommand, ValueEnum};

pub use crate::task::Area;

#[derive(Parser)]
#[command(name = "dodo")]
#[command(about = "Keyboard-first todo + time tracker CLI")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show help information
    #[command(visible_alias = "h")]
    Help,

    /// Add a new task
    #[command(visible_alias = "a")]
    Add(AddArgs),

    /// List tasks
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Start/stop timer on a task (no args = pause running task)
    #[command(visible_alias = "s")]
    Start(StartArgs),

    /// Complete a task (default: running task)
    #[command(visible_alias = "d")]
    Done(DoneArgs),

    /// Show running task status
    #[command(visible_alias = "st")]
    Status,

    /// Delete a task
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),

    /// Move a task to a different area
    #[command(visible_alias = "mv")]
    Move(MoveArgs),

    /// Edit a task's metadata
    #[command(visible_alias = "e")]
    Edit(EditArgs),

    /// Add or view notes on a task
    #[command(visible_alias = "n")]
    Note(NoteArgs),

    /// Manage recurring tasks
    #[command(visible_alias = "rec")]
    Recurring(RecurringArgs),

    /// View or update configuration
    #[command(visible_alias = "cfg")]
    Config(ConfigArgs),

    /// Show productivity reports
    #[command(visible_alias = "rp")]
    Report(ReportArgs),

    /// Manage Turso sync
    Sync(SyncArgs),

    /// Manage S3 backups
    Backup(BackupArgs),

    /// Send email digest
    Email(EmailArgs),

    /// Update dodo to the latest version
    #[command(visible_alias = "up")]
    Update,

    /// Open TUI
    #[command(visible_alias = "t")]
    Tui,
}

#[derive(Parser)]
pub struct AddArgs {
    /// Task title and inline notation (+project @context #tag ~estimate ^deadline =scheduled !)
    #[arg(trailing_var_arg = true, required = true)]
    pub title: Vec<String>,

    /// Focus area
    #[arg(long, value_enum, default_value = "today")]
    pub area: Area,

    /// Project tag (+project)
    #[arg(long)]
    pub project: Option<String>,

    /// Context tag (@context)
    #[arg(long)]
    pub context: Option<String>,

    /// Time estimate in minutes
    #[arg(long)]
    pub estimate: Option<i64>,

    /// Deadline date (YYYY-MM-DD)
    #[arg(long)]
    pub deadline: Option<String>,

    /// Scheduled date (YYYY-MM-DD)
    #[arg(long)]
    pub scheduled: Option<String>,

    /// Tags (comma-separated)
    #[arg(long)]
    pub tags: Option<String>,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Filters: area (today/week/long/done), +project, @context, #tag, !! (priority), ^<3d (deadline), =<1w (scheduled)
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,

    /// Sort order
    #[arg(long, value_enum, default_value = "created")]
    pub sort: SortBy,

    /// Sort in descending order
    #[arg(long)]
    pub desc: bool,

    /// Filter by project
    #[arg(short, long)]
    pub project: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq)]
pub enum SortBy {
    /// Sort by creation date (newest first)
    Created,
    /// Sort by last modified date (newest first)
    Modified,
    /// Sort by area (Long → Week → Today → Done)
    Area,
    /// Sort alphabetically by title
    Title,
}

#[derive(Parser)]
pub struct StartArgs {
    /// Task to start (numeric ID or fuzzy text). No args = pause running task.
    #[arg(trailing_var_arg = true)]
    pub task: Vec<String>,
}

#[derive(Parser)]
pub struct DoneArgs {
    /// Task to complete (numeric ID or fuzzy text). Omit to complete running task.
    #[arg(trailing_var_arg = true)]
    pub task: Vec<String>,

    /// Reopen a completed task
    #[arg(long)]
    pub undo: bool,
}

#[derive(Parser)]
pub struct MoveArgs {
    /// Task to move (numeric ID or fuzzy text)
    #[arg(trailing_var_arg = true, required = true)]
    pub task: Vec<String>,

    /// Target area
    #[arg(long, value_enum)]
    pub to: Area,
}

#[derive(Parser)]
pub struct RemoveArgs {
    /// Task to delete (numeric ID or fuzzy text)
    #[arg(trailing_var_arg = true, required = true)]
    pub task: Vec<String>,

    /// For recurring instances: skip generating next instance (truncate series)
    #[arg(long, default_value_t = false)]
    pub series: bool,
}

#[derive(Parser)]
pub struct EditArgs {
    /// Task identifier and notation tokens to update
    #[arg(trailing_var_arg = true, required = true)]
    pub args: Vec<String>,

    /// Change focus area
    #[arg(long, value_enum)]
    pub area: Option<Area>,
}

#[derive(Parser)]
pub struct NoteArgs {
    /// Task to add note to (numeric ID or fuzzy text)
    #[arg(trailing_var_arg = true, required = true)]
    pub task: Vec<String>,

    /// Clear all notes
    #[arg(long)]
    pub clear: bool,

    /// Show notes without prompting for new input
    #[arg(long)]
    pub show: bool,

    /// Delete a specific note line (1-indexed)
    #[arg(long)]
    pub delete_line: Option<usize>,

    /// Edit a specific note line (1-indexed, replacement read from stdin)
    #[arg(long)]
    pub edit_line: Option<usize>,
}

#[derive(Parser)]
pub struct RecurringArgs {
    #[command(subcommand)]
    pub action: Option<RecurringAction>,
}

#[derive(Subcommand)]
pub enum RecurringAction {
    /// Add a new recurring template (use *pattern for recurrence)
    Add(RecurringAddArgs),

    /// Edit a recurring template
    Edit(RecurringEditArgs),

    /// Delete a recurring template
    Delete(RecurringQueryArgs),

    /// Pause a recurring template
    Pause(RecurringQueryArgs),

    /// Resume a paused recurring template
    Resume(RecurringQueryArgs),

    /// Generate missing instances for all active templates
    Generate,

    /// View completion history for a template
    History(RecurringQueryArgs),
}

#[derive(Parser)]
pub struct RecurringAddArgs {
    /// Template title and inline notation (+project @context #tag ~estimate *pattern)
    #[arg(trailing_var_arg = true, required = true)]
    pub title: Vec<String>,
}

#[derive(Parser)]
pub struct RecurringEditArgs {
    /// Template identifier and notation tokens to update
    #[arg(trailing_var_arg = true, required = true)]
    pub args: Vec<String>,
}

#[derive(Parser)]
pub struct RecurringQueryArgs {
    /// Template to operate on (numeric ID or fuzzy text)
    #[arg(trailing_var_arg = true, required = true)]
    pub query: Vec<String>,
}

#[derive(Parser)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub action: Option<SyncAction>,
}

#[derive(Subcommand)]
pub enum SyncAction {
    /// Show sync status
    Status,
    /// Enable Turso sync (interactive setup)
    Enable,
    /// Disable Turso sync
    Disable,
    /// Trigger a sync now
    Now,
}

#[derive(Parser)]
pub struct BackupArgs {
    #[command(subcommand)]
    pub action: Option<BackupAction>,
}

#[derive(Subcommand)]
pub enum BackupAction {
    /// List available backups
    List,
    /// Restore from a backup
    Restore(BackupRestoreArgs),
    /// Delete a specific backup
    Delete(BackupDeleteArgs),
}

#[derive(Parser)]
pub struct BackupRestoreArgs {
    /// Restore "latest" or pick interactively
    #[arg(default_value = "latest")]
    pub target: String,
}

#[derive(Parser)]
pub struct BackupDeleteArgs {
    /// Backup name to delete
    #[arg(required = true)]
    pub name: String,
}

#[derive(Parser)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: Option<ConfigAction>,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    /// Print config file path
    Path,
}

#[derive(Parser)]
pub struct ReportArgs {
    /// Time range
    #[arg(value_enum, default_value = "month")]
    pub range: ReportRange,
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum ReportRange {
    Day,
    Week,
    Month,
    Year,
    All,
}

impl ReportRange {
    pub fn label(self) -> &'static str {
        match self {
            ReportRange::Day => "DAY",
            ReportRange::Week => "WEEK",
            ReportRange::Month => "MONTH",
            ReportRange::Year => "YEAR",
            ReportRange::All => "ALL",
        }
    }

    pub fn date_range(self) -> (String, String) {
        let today = crate::today();
        let to = (today + chrono::Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Utc)
            .unwrap()
            .to_rfc3339();
        let from_date = match self {
            ReportRange::Day => today,
            ReportRange::Week => today - chrono::Duration::days(7),
            ReportRange::Month => today - chrono::Duration::days(30),
            ReportRange::Year => today - chrono::Duration::days(365),
            ReportRange::All => chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
        };
        let from = from_date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Utc)
            .unwrap()
            .to_rfc3339();
        (from, to)
    }

    pub fn next(self) -> Self {
        match self {
            ReportRange::Day => ReportRange::Week,
            ReportRange::Week => ReportRange::Month,
            ReportRange::Month => ReportRange::Year,
            ReportRange::Year => ReportRange::All,
            ReportRange::All => ReportRange::Day,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ReportRange::Day => ReportRange::All,
            ReportRange::Week => ReportRange::Day,
            ReportRange::Month => ReportRange::Week,
            ReportRange::Year => ReportRange::Month,
            ReportRange::All => ReportRange::Year,
        }
    }
}

#[derive(Parser)]
pub struct EmailArgs {
    #[command(subcommand)]
    pub action: EmailAction,
}

#[derive(Subcommand)]
pub enum EmailAction {
    /// Send today's digest email now
    Digest,
    /// Show crontab entry for scheduling
    Cron,
    /// Send a test email to verify configuration
    Test,
}
