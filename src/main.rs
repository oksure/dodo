use anyhow::Result;
use clap::Parser;

mod tui;

use dodo::cli::{Cli, Commands};
use dodo::db::Database;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize database
    let db = Database::new()?;

    match cli.command {
        Commands::Add(args) => {
            let num_id = db.add_task(&args.title, args.area, args.project, args.context)?;
            println!("Added: {} [#{}]", args.title, num_id);
        }
        Commands::List(args) => {
            let tasks = db.list_tasks(args.area)?;
            if tasks.is_empty() {
                println!("No tasks found.");
            } else {
                for task in tasks {
                    println!("{}", task);
                }
            }
        }
        Commands::Start(args) => {
            db.start_timer(&args.task)?;
            println!("Started timer for: {}", args.task);
        }
        Commands::Pause => {
            db.pause_timer()?;
            println!("Timer paused.");
        }
        Commands::Done => {
            if let Some((task, duration)) = db.complete_task()? {
                println!("Completed: {} ({})", task, format_duration(duration));
            } else {
                println!("No running task to complete.");
            }
        }
        Commands::Status => {
            if let Some((task, elapsed)) = db.get_running_task()? {
                println!("Running: {} ({})", task, format_duration(elapsed));
            } else {
                println!("No task running.");
            }
        }
        Commands::Remove(args) => {
            db.delete_task(&args.task)?;
            println!("Deleted: {}", args.task);
        }
        Commands::Tui => {
            tui::run_tui(&db)?;
        }
    }

    Ok(())
}

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}
