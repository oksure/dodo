use anyhow::Result;
use chrono::NaiveDate;
use clap::Parser;
use colored::Colorize;
use std::io::{self, BufRead};

mod tui;

use dodo::cli::{Area as CliArea, Cli, Commands};
use dodo::db::Database;
use dodo::notation::parse_notation;
use dodo::task::{Area, Task, TaskStatus};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize database
    let db = Database::new()?;

    match cli.command {
        None => {
            tui::run_tui(&db)?;
        }
        Some(Commands::Help) => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
        }
        Some(Commands::Add(args)) => {
            let raw_input = args.title.join(" ");
            let parsed = parse_notation(&raw_input);

            // Inline notation takes precedence over flags
            let project = parsed.project.or(args.project);
            let context = if !parsed.contexts.is_empty() {
                Some(parsed.contexts.join(","))
            } else {
                args.context
            };
            let estimate = parsed.estimate_minutes.or(args.estimate).or(Some(60));
            let deadline = parsed.deadline.or_else(|| {
                args.deadline
                    .as_ref()
                    .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            });
            let scheduled = parsed
                .scheduled
                .or_else(|| {
                    args.scheduled
                        .as_ref()
                        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                })
                .or_else(|| Some(chrono::Local::now().date_naive()));
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
                &title, args.area, project, context, estimate, deadline, scheduled, tags, priority,
            )?;
            println!("Added: {} [#{}]", title, num_id);
        }
        Some(Commands::List(args)) => {
            // Parse positional args for filters
            let mut filter_area: Option<CliArea> = None;
            let mut filter_project: Option<String> = args.project.clone();
            let mut filter_context: Option<String> = None;
            let mut filter_tag: Option<String> = None;

            for arg in &args.args {
                if let Some(proj) = arg.strip_prefix('+') {
                    filter_project = Some(proj.to_string());
                } else if let Some(ctx) = arg.strip_prefix('@') {
                    filter_context = Some(ctx.to_string());
                } else if let Some(tag) = arg.strip_prefix('#') {
                    filter_tag = Some(tag.to_string());
                } else {
                    // Try area name
                    match arg.to_lowercase().as_str() {
                        "today" => filter_area = Some(CliArea::Today),
                        "week" => filter_area = Some(CliArea::ThisWeek),
                        "long" => filter_area = Some(CliArea::LongTerm),
                        "done" => filter_area = Some(CliArea::Completed),
                        _ => {}
                    }
                }
            }

            if let Some(ref project) = filter_project {
                let tasks = db.list_tasks_by_project(project, args.sort)?;
                let tasks_ref: Vec<&Task> = tasks.iter().collect();
                let tasks = apply_filters(&tasks_ref, filter_area, None, filter_context.as_deref(), filter_tag.as_deref());
                if tasks.is_empty() {
                    println!("No tasks found.");
                } else {
                    for task in &tasks {
                        print_task_colored(task);
                    }
                }
            } else if let Some(area) = filter_area {
                let tasks = db.list_tasks_sorted(Some(area), args.sort)?;
                let tasks_ref: Vec<&Task> = tasks.iter().collect();
                let tasks = apply_filters(&tasks_ref, None, None, filter_context.as_deref(), filter_tag.as_deref());
                if tasks.is_empty() {
                    println!("No tasks found.");
                } else {
                    for task in &tasks {
                        print_task_colored(task);
                    }
                }
            } else {
                // No area specified: show all four groups
                let all = db.list_all_tasks(args.sort)?;
                if all.is_empty() {
                    println!("No tasks found.");
                } else {
                    let all_ref: Vec<&Task> = all.iter().collect();
                    let filtered = apply_filters(&all_ref, None, None, filter_context.as_deref(), filter_tag.as_deref());

                    let mut today = vec![];
                    let mut week = vec![];
                    let mut long = vec![];
                    let mut done = vec![];
                    for task in &filtered {
                        match task.effective_area() {
                            Area::Today => today.push(*task),
                            Area::ThisWeek => week.push(*task),
                            Area::LongTerm => long.push(*task),
                            Area::Completed => done.push(*task),
                        }
                    }

                    let sections: Vec<(&str, Vec<&Task>)> = vec![
                        ("TODAY", today),
                        ("THIS WEEK", week),
                        ("LONG TERM", long),
                        ("DONE", done),
                    ];

                    let mut first = true;
                    for (label, tasks) in &sections {
                        if tasks.is_empty() {
                            continue;
                        }
                        if !first {
                            println!();
                        }
                        first = false;
                        println!(
                            "{}",
                            format!("--- {} ({}) ---", label, tasks.len())
                                .cyan()
                                .bold()
                        );
                        let limit = if *label == "DONE" { 5 } else { tasks.len() };
                        for task in tasks.iter().take(limit) {
                            print_task_colored(task);
                        }
                        if *label == "DONE" && tasks.len() > 5 {
                            println!("  {} {}", "...".dimmed(), format!("and {} more", tasks.len() - 5).dimmed());
                        }
                    }
                }
            }
        }
        Some(Commands::Start(args)) => {
            if args.task.is_empty() {
                db.pause_timer()?;
                println!("Timer paused.");
            } else {
                let query = args.task.join(" ");
                let (title, num_id) = db.start_timer(&query)?;
                println!("Started: {} [#{}]", title, num_id);
            }
        }
        Some(Commands::Done) => {
            if let Some((task, duration)) = db.complete_task()? {
                println!("Completed: {} ({})", task, format_duration(duration));
            } else {
                println!("No running task to complete.");
            }
        }
        Some(Commands::Status) => {
            if let Some((task, elapsed)) = db.get_running_task()? {
                println!(
                    "{} {} ({})",
                    "Running:".green().bold(),
                    task,
                    format_duration(elapsed).green()
                );
            } else {
                println!("No task running.");
            }
        }
        Some(Commands::Remove(args)) => {
            let query = args.task.join(" ");
            let (title, num_id) = db.delete_task(&query)?;
            println!("Deleted: {} [#{}]", title, num_id);
        }
        Some(Commands::Edit(args)) => {
            let raw_input = args.args.join(" ");
            let parsed = parse_notation(&raw_input);

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
        Some(Commands::Note(args)) => {
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
                let (title, notes) = db.get_task_notes(&query)?;
                println!("Notes for: {}", title);
                if let Some(text) = notes {
                    println!("{}", text);
                }

                println!("Enter note (Ctrl+D to finish):");
                let stdin = io::stdin();
                let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();

                if !lines.is_empty() {
                    let text = lines.join("\n");
                    db.append_note(&query, &text)?;
                    println!("Note added to: {}", title);
                }
            }
        }
        Some(Commands::Tui) => {
            tui::run_tui(&db)?;
        }
    }

    Ok(())
}

fn apply_filters<'a>(
    tasks: &[&'a Task],
    _area: Option<CliArea>,
    _project: Option<&str>,
    context: Option<&str>,
    tag: Option<&str>,
) -> Vec<&'a Task> {
    tasks
        .iter()
        .filter(|task| {
            if let Some(ctx_filter) = context {
                match &task.context {
                    Some(c) => {
                        if !c
                            .split(',')
                            .any(|x| x.trim().eq_ignore_ascii_case(ctx_filter))
                        {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            if let Some(tag_filter) = tag {
                match &task.tags {
                    Some(t) => {
                        if !t
                            .split(',')
                            .any(|x| x.trim().eq_ignore_ascii_case(tag_filter))
                        {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            true
        })
        .copied()
        .collect()
}

fn print_task_colored(task: &Task) {
    let today = chrono::Local::now().date_naive();
    let seven_days = today + chrono::Duration::days(7);

    let num = task
        .num_id
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());

    let status_icon = match task.status {
        TaskStatus::Pending => " ",
        TaskStatus::Running => "\u{25B6}",
        TaskStatus::Paused => "\u{23F8}",
        TaskStatus::Done => "\u{2713}",
    };

    let notes_indicator = match &task.notes {
        Some(n) if !n.is_empty() => " *",
        _ => "",
    };

    // Number + status
    let num_str = format!("[{}] [{}]", num, status_icon);
    let num_colored = match task.status {
        TaskStatus::Running => num_str.green().bold().to_string(),
        TaskStatus::Paused => num_str.yellow().to_string(),
        TaskStatus::Done => num_str.dimmed().to_string(),
        TaskStatus::Pending => num_str.normal().to_string(),
    };

    // Area
    let area_str = task.area_str();

    // Title (with overdue check)
    let is_overdue = task.status != TaskStatus::Done
        && task.deadline.is_some_and(|dl| dl < today);
    let title_colored = if task.status == TaskStatus::Running {
        task.title.green().bold().to_string()
    } else if task.status == TaskStatus::Done {
        task.title.dimmed().to_string()
    } else if is_overdue {
        task.title.red().bold().to_string()
    } else {
        task.title.clone()
    };

    // Metadata parts
    let mut meta_parts: Vec<String> = vec![];

    if let Some(p) = task.priority {
        if p > 0 {
            let bangs = "!".repeat(p.clamp(1, 4) as usize);
            let colored = match p {
                4 => bangs.red().bold().to_string(),
                3 => bangs.red().to_string(),
                2 => bangs.yellow().to_string(),
                _ => bangs.dimmed().to_string(),
            };
            meta_parts.push(colored);
        }
    }
    if let Some(ref p) = task.project {
        meta_parts.push(format!("+{}", p).magenta().to_string());
    }
    if let Some(ref c) = task.context {
        for ctx in c.split(',') {
            let ctx = ctx.trim();
            if !ctx.is_empty() {
                meta_parts.push(format!("@{}", ctx).cyan().to_string());
            }
        }
    }
    if let Some(ref t) = task.tags {
        for tag in t.split(',') {
            let tag = tag.trim();
            if !tag.is_empty() {
                meta_parts.push(format!("#{}", tag).dimmed().to_string());
            }
        }
    }
    if let Some(est) = task.estimate_minutes {
        meta_parts.push(format!("~{}", format_estimate(est)).dimmed().to_string());
    }
    if let Some(ref dl) = task.deadline {
        let dl_str = format!("^{}", dl.format("%b%d"));
        let colored = if *dl < today {
            dl_str.red().bold().to_string()
        } else if *dl <= seven_days {
            dl_str.yellow().to_string()
        } else {
            dl_str.dimmed().to_string()
        };
        meta_parts.push(colored);
    }
    if let Some(ref sc) = task.scheduled {
        meta_parts.push(format!("={}", sc.format("%b%d")).cyan().to_string());
    }

    let meta = if meta_parts.is_empty() {
        String::new()
    } else {
        format!(" {}", meta_parts.join(" "))
    };

    // Time
    let time_str = task.display_time();
    let time_colored = if !time_str.is_empty() {
        let elapsed = task.elapsed_seconds.unwrap_or(0);
        match task.estimate_minutes {
            Some(est) if elapsed > est * 60 => time_str.red().to_string(),
            Some(est) if elapsed > est * 45 => time_str.yellow().to_string(),
            _ => time_str.green().to_string(),
        }
    } else {
        String::new()
    };

    let running_tag = if task.status == TaskStatus::Running {
        " [running]".green().bold().to_string()
    } else {
        String::new()
    };

    println!(
        "{} {} {}{}{}{}{}",
        num_colored, area_str, title_colored, notes_indicator, meta, time_colored, running_tag
    );
}

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

fn format_estimate(minutes: i64) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 && mins > 0 {
        format!("{}h{}m", hours, mins)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        format!("{}m", mins)
    }
}
