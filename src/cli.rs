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
    #[command(visible_alias = "a")]
    Add(AddArgs),

    /// List tasks
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Start/stop timer on a task (no args = pause running task)
    #[command(visible_alias = "s")]
    Start(StartArgs),

    /// Complete the running task
    #[command(visible_alias = "d")]
    Done,

    /// Show running task status
    #[command(visible_alias = "st")]
    Status,

    /// Delete a task
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),

    /// Edit a task's metadata
    #[command(visible_alias = "e")]
    Edit(EditArgs),

    /// Add or view notes on a task
    #[command(visible_alias = "n")]
    Note(NoteArgs),

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
    /// Filters: area (today/week/long/done), +project, @context, #tag
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,

    /// Sort order
    #[arg(long, value_enum, default_value = "created")]
    pub sort: SortBy,

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
pub struct RemoveArgs {
    /// Task to delete (numeric ID or fuzzy text)
    #[arg(trailing_var_arg = true, required = true)]
    pub task: Vec<String>,
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
