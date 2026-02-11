use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "dodo")]
#[command(about = "Keyboard-first todo + time tracker CLI")]
#[command(version = "0.1.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add a new task
    #[command(alias = "a")]
    Add(AddArgs),

    /// List tasks
    #[command(alias = "ls")]
    List(ListArgs),

    /// Start timer on a task
    #[command(alias = "s")]
    Start(StartArgs),

    /// Pause current timer
    #[command(alias = "p")]
    Pause,

    /// Complete the running task
    #[command(alias = "d")]
    Done,

    /// Show running task status
    #[command(alias = "st")]
    Status,

    /// Delete a task
    #[command(alias = "rm")]
    Remove(RemoveArgs),

    /// Open TUI
    #[command(alias = "t")]
    Tui,
}

#[derive(Parser)]
pub struct AddArgs {
    /// Task title
    pub title: String,

    /// Focus area
    #[arg(long, value_enum, default_value = "today")]
    pub area: Area,

    /// Project tag (+project)
    #[arg(long)]
    pub project: Option<String>,

    /// Context tag (@context)
    #[arg(long)]
    pub context: Option<String>,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Filter by area
    #[arg(value_enum)]
    pub area: Option<Area>,
}

#[derive(Parser)]
pub struct StartArgs {
    /// Task to start (fuzzy matched)
    pub task: String,
}

#[derive(Parser)]
pub struct RemoveArgs {
    /// Task to delete (fuzzy matched)
    pub task: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Area {
    #[value(name = "long")]
    LongTerm,
    #[value(name = "week")]
    ThisWeek,
    #[value(name = "today")]
    Today,
    #[value(name = "done")]
    Completed,
}

impl From<Area> for String {
    fn from(area: Area) -> String {
        match area {
            Area::LongTerm => "LongTerm".to_string(),
            Area::ThisWeek => "ThisWeek".to_string(),
            Area::Today => "Today".to_string(),
            Area::Completed => "Completed".to_string(),
        }
    }
}
