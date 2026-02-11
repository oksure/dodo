use anyhow::Result;
use chrono::NaiveDate;
use clap::Parser;
use std::io::{self, BufRead};

mod tui;

use dodo::cli::{Cli, Commands};
use dodo::db::Database;
use dodo::notation::parse_notation;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize database
    let db = Database::new()?;

    match cli.command {
        Commands::Add(args) => {
            let raw_input = args.title.join(" ");
            let parsed = parse_notation(&raw_input);

            // Inline notation takes precedence over flags
            let project = parsed.project.or(args.project);
            let context = if !parsed.contexts.is_empty() {
                Some(parsed.contexts.join(","))
            } else {
                args.context
            };
            let estimate = parsed.estimate_minutes.or(args.estimate);
            let deadline = parsed.deadline.or_else(|| {
                args.deadline.as_ref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            });
            let scheduled = parsed.scheduled.or_else(|| {
                args.scheduled.as_ref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            });
            let tags = if !parsed.tags.is_empty() {
                Some(parsed.tags.join(","))
            } else {
                args.tags
            };
            let priority = parsed.priority;

            let title = if parsed.title.is_empty() {
                raw_input.clone()
            } else {
                parsed.title
            };

            let num_id = db.add_task(
                &title,
                args.area,
                project,
                context,
                estimate,
                deadline,
                scheduled,
                tags,
                priority,
            )?;
            println!("Added: {} [#{}]", title, num_id);
        }
        Commands::List(args) => {
            let tasks = db.list_tasks_sorted(args.area, args.sort)?;
            if tasks.is_empty() {
                println!("No tasks found.");
            } else {
                for task in tasks {
                    println!("{}", task);
                }
            }
        }
        Commands::Start(args) => {
            let query = args.task.join(" ");
            db.start_timer(&query)?;
            println!("Started timer for: {}", query);
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
            let query = args.task.join(" ");
            db.delete_task(&query)?;
            println!("Deleted: {}", query);
        }
        Commands::Edit(args) => {
            let raw_input = args.args.join(" ");
            let parsed = parse_notation(&raw_input);

            // The title part (after tokens extracted) is the task identifier
            let task_query = if parsed.title.is_empty() {
                anyhow::bail!("Edit requires a task identifier (numeric ID or text)");
            } else {
                parsed.title.clone()
            };

            if !parsed.has_updates() && args.area.is_none() {
                anyhow::bail!("No changes specified. Use notation tokens (+project @context #tag ~estimate ^deadline =scheduled !) or --area flag.");
            }

            let title = db.update_task_fields(&task_query, &parsed, args.area)?;
            println!("Updated: {}", title);
        }
        Commands::Note(args) => {
            let query = args.task.join(" ");

            if args.clear {
                let title = db.clear_notes(&query)?;
                println!("Cleared notes for: {}", title);
            } else if args.show {
                let (title, notes) = db.get_task_notes(&query)?;
                println!("Notes for: {}", title);
                match notes {
                    Some(text) => println!("{}", text),
                    None => println!("(no notes)"),
                }
            } else {
                // Show existing notes
                let (title, notes) = db.get_task_notes(&query)?;
                println!("Notes for: {}", title);
                if let Some(text) = notes {
                    println!("{}", text);
                }

                // Read new note from stdin
                println!("Enter note (Ctrl+D to finish):");
                let stdin = io::stdin();
                let lines: Vec<String> = stdin.lock().lines()
                    .map_while(Result::ok)
                    .collect();

                if !lines.is_empty() {
                    let text = lines.join("\n");
                    db.append_note(&query, &text)?;
                    println!("Note added to: {}", title);
                }
            }
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
