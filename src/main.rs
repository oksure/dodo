use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::Parser;
use colored::Colorize;
use std::io::{self, BufRead};

mod tui;
mod update;

use dodo::backup;
use dodo::cli::{BackupAction, Cli, Commands, EmailAction, RecurringAction, SyncAction};
use dodo::config::Config;
use dodo::db::Database;
use dodo::notation::{parse_notation, prepare_task};
use dodo::task::{format_estimate, Area, Task, TaskStatus};

const DONE_DISPLAY_LIMIT: usize = 5;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load config before database init (needed for sync + backup-age check)
    let config = Config::load().unwrap_or_default();

    // Initialize timezone from config (must happen before any date computation)
    dodo::init_timezone(config.preferences.timezone.as_deref());

    // Recover from any interrupted sync migration before opening DB
    Database::recover_interrupted_migration()?;

    // Initialize database (always local — sync is background-only)
    let _ = Database::clean_sync_metadata(); // clean up old replica metadata from dodo.db
    let db = Database::new()?;

    // Startup backup-age check — auto-backup silently if overdue
    if let Ok(Some(_)) = backup::check_backup_age(&config.backup) {
        if let Err(e) = backup::create_backup(&config.backup) {
            eprintln!("Auto-backup failed: {}", e);
        }
    }

    match cli.command {
        None => tui::run_tui(&db),
        Some(Commands::Help) => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
        Some(Commands::Add(args)) => cmd_add(&db, args),
        Some(Commands::List(args)) => cmd_list(&db, args),
        Some(Commands::Start(args)) => cmd_start(&db, args),
        Some(Commands::Done(args)) => cmd_done(&db, args),
        Some(Commands::Status) => cmd_status(&db),
        Some(Commands::Remove(args)) => cmd_remove(&db, args),
        Some(Commands::Move(args)) => cmd_move(&db, args),
        Some(Commands::Edit(args)) => cmd_edit(&db, args),
        Some(Commands::Note(args)) => cmd_note(&db, args),
        Some(Commands::Recurring(args)) => cmd_recurring(&db, args),
        Some(Commands::Config(args)) => cmd_config(args),
        Some(Commands::Report(args)) => cmd_report(&db, args),
        Some(Commands::Sync(args)) => cmd_sync(&db, args),
        Some(Commands::Backup(args)) => cmd_backup(args),
        Some(Commands::Email(args)) => cmd_email(&db, args),
        Some(Commands::Update) => update::check_update(),
        Some(Commands::Tui) => tui::run_tui(&db),
    }
}

fn cmd_add(db: &Database, args: dodo::cli::AddArgs) -> Result<()> {
    let raw_input = args.title.join(" ");
    let mut prep = prepare_task(&raw_input);

    // CLI flag overrides (notation tokens take priority via prepare_task,
    // but CLI flags fill in when notation didn't provide a value)
    if prep.project.is_none() {
        prep.project = args.project;
    }
    if prep.context.is_none() {
        prep.context = args.context;
    }
    if let Some(est) = args.estimate {
        if prep.estimate_minutes == Some(60) {
            // Only override if prepare_task used the default
            prep.estimate_minutes = Some(est);
        }
    }
    if prep.deadline.is_none() {
        prep.deadline = args
            .deadline
            .as_ref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    }
    if prep.scheduled == Some(dodo::today()) {
        // Only override scheduled if prepare_task used the default (today)
        if let Some(sched) = args
            .scheduled
            .as_ref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        {
            prep.scheduled = Some(sched);
        }
    }
    if prep.tags.is_none() {
        prep.tags = args.tags;
    }

    let num_id = db.add_task(
        &prep.title,
        args.area,
        prep.project,
        prep.context,
        prep.estimate_minutes,
        prep.deadline,
        prep.scheduled,
        prep.tags,
        prep.priority,
    )?;
    println!("Added: {} [#{}]", prep.title, num_id);
    Ok(())
}

fn cmd_list(db: &Database, args: dodo::cli::ListArgs) -> Result<()> {
    let today = dodo::today();
    let mut filter_area: Option<Area> = None;
    let mut filter_project: Option<String> = args.project.clone();
    let mut filter_context: Option<String> = None;
    let mut filter_tag: Option<String> = None;
    let mut filter_priority: Option<i64> = None;
    let mut filter_deadline_days: Option<i64> = None;
    let mut filter_scheduled_days: Option<i64> = None;

    for arg in &args.args {
        if let Some(proj) = arg.strip_prefix('+') {
            filter_project = Some(proj.to_string());
        } else if let Some(ctx) = arg.strip_prefix('@') {
            filter_context = Some(ctx.to_string());
        } else if let Some(tag) = arg.strip_prefix('#') {
            filter_tag = Some(tag.to_string());
        } else if let Some(rest) = arg.strip_prefix("^<") {
            filter_deadline_days = dodo::notation::parse_filter_days(rest);
        } else if let Some(rest) = arg.strip_prefix("=<") {
            filter_scheduled_days = dodo::notation::parse_filter_days(rest);
        } else if arg.starts_with('!') && arg.chars().all(|c| c == '!') && !arg.is_empty() {
            filter_priority = Some(arg.len() as i64);
        } else {
            match arg.to_lowercase().as_str() {
                "today" => filter_area = Some(Area::Today),
                "week" => filter_area = Some(Area::ThisWeek),
                "long" => filter_area = Some(Area::LongTerm),
                "done" => filter_area = Some(Area::Completed),
                _ => {}
            }
        }
    }

    let load_and_filter = |tasks: Vec<Task>| -> Vec<Task> {
        let tasks_ref: Vec<&Task> = tasks.iter().collect();
        let filtered = apply_filters(&tasks_ref, filter_context.as_deref(), filter_tag.as_deref());
        let mut result: Vec<Task> = filtered
            .into_iter()
            .filter(|task| {
                if let Some(min_pri) = filter_priority {
                    if task.priority.unwrap_or(0) < min_pri {
                        return false;
                    }
                }
                if let Some(days) = filter_deadline_days {
                    let cutoff = today + chrono::Duration::days(days);
                    match task.deadline {
                        Some(dl) if dl <= cutoff => {}
                        _ => return false,
                    }
                }
                if let Some(days) = filter_scheduled_days {
                    let cutoff = today + chrono::Duration::days(days);
                    match task.scheduled {
                        Some(sc) if sc <= cutoff => {}
                        _ => return false,
                    }
                }
                true
            })
            .cloned()
            .collect();
        if args.desc {
            result.reverse();
        }
        result
    };

    if let Some(ref project) = filter_project {
        let tasks = db.list_tasks_by_project(project, args.sort)?;
        let tasks = load_and_filter(tasks);
        if tasks.is_empty() {
            println!("No tasks found.");
        } else {
            for task in &tasks {
                print_task_colored(task);
            }
        }
    } else if let Some(area) = filter_area {
        let tasks = db.list_tasks_sorted(Some(area), args.sort)?;
        let tasks = load_and_filter(tasks);
        if tasks.is_empty() {
            println!("No tasks found.");
        } else {
            for task in &tasks {
                print_task_colored(task);
            }
        }
    } else {
        let all = db.list_all_tasks(args.sort)?;
        if all.is_empty() {
            println!("No tasks found.");
        } else {
            let filtered = load_and_filter(all);

            let mut today_tasks = vec![];
            let mut week = vec![];
            let mut long = vec![];
            let mut done = vec![];
            for task in &filtered {
                match task.effective_area() {
                    Area::Today => today_tasks.push(task),
                    Area::ThisWeek => week.push(task),
                    Area::LongTerm => long.push(task),
                    Area::Completed => done.push(task),
                }
            }

            let sections: Vec<(&str, Vec<&Task>)> = vec![
                ("TODAY", today_tasks),
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
                    format!("--- {} ({}) ---", label, tasks.len()).cyan().bold()
                );
                let limit = if *label == "DONE" {
                    DONE_DISPLAY_LIMIT
                } else {
                    tasks.len()
                };
                for task in tasks.iter().take(limit) {
                    print_task_colored(task);
                }
                if *label == "DONE" && tasks.len() > DONE_DISPLAY_LIMIT {
                    println!(
                        "  {} {}",
                        "...".dimmed(),
                        format!("and {} more", tasks.len() - DONE_DISPLAY_LIMIT).dimmed()
                    );
                }
            }
        }
    }
    Ok(())
}

fn cmd_start(db: &Database, args: dodo::cli::StartArgs) -> Result<()> {
    if args.task.is_empty() {
        db.pause_timer()?;
        println!("Timer paused.");
    } else {
        let query = args.task.join(" ");
        let (title, num_id) = db.start_timer(&query)?;
        println!("Started: {} [#{}]", title, num_id);
    }
    Ok(())
}

fn cmd_done(db: &Database, args: dodo::cli::DoneArgs) -> Result<()> {
    let query = args.task.join(" ");

    if args.undo {
        if query.is_empty() {
            anyhow::bail!("--undo requires a task identifier (numeric ID or text)");
        }
        let task = db.resolve_done_task(&query)?;
        db.uncomplete_task_by_id(&task.id)?;
        let num = task
            .num_id
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into());
        println!("Reopened: {} [#{}]", task.title, num);
    } else if !query.is_empty() {
        let task = db.resolve_task(&query)?;
        db.complete_task_by_id(&task.id)?;
        let num = task
            .num_id
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into());
        println!("Completed: {} [#{}]", task.title, num);
    } else {
        if let Some((task, duration)) = db.complete_task()? {
            println!("Completed: {} ({})", task, format_duration(duration));
        } else {
            println!("No running task to complete.");
        }
    }
    Ok(())
}

fn cmd_status(db: &Database) -> Result<()> {
    if let Some((task, elapsed, _estimate)) = db.get_running_task()? {
        println!(
            "{} {} ({})",
            "Running:".green().bold(),
            task,
            format_duration(elapsed).green()
        );
    } else {
        println!("No task running.");
    }
    Ok(())
}

fn cmd_remove(db: &Database, args: dodo::cli::RemoveArgs) -> Result<()> {
    let query = args.task.join(" ");
    let task = db.resolve_task(&query)?;
    let title = task.title.clone();
    let num_id = task.num_id.unwrap_or(0);

    if task.template_id.is_some() && !args.series {
        // Recurring instance: generate next occurrence before deleting (matches TUI Backspace)
        if let Err(e) = db.complete_recurring_instance(&task.id) {
            eprintln!("Warning: failed to generate next recurring instance: {e}");
        }
        db.delete_task_by_id(&task.id)?;
        println!("Skipped recurring instance [#{}] {}; next occurrence generated.", num_id, title);
    } else {
        db.delete_task_by_id(&task.id)?;
        if task.template_id.is_some() {
            println!("Removed recurring instance [#{}] {}; series NOT extended (--series flag).", num_id, title);
        } else {
            println!("Deleted: {} [#{}]", title, num_id);
        }
    }

    Ok(())
}

fn cmd_move(db: &Database, args: dodo::cli::MoveArgs) -> Result<()> {
    let query = args.task.join(" ");
    let task = db.resolve_task(&query)?;
    let date = args.to.to_scheduled_date();
    db.update_task_scheduled(&task.id, date)?;
    let num = task
        .num_id
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    println!("Moved: {} [#{}] → {}", task.title, num, args.to.as_str());
    Ok(())
}

fn cmd_edit(db: &Database, args: dodo::cli::EditArgs) -> Result<()> {
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
    Ok(())
}

fn cmd_note(db: &Database, args: dodo::cli::NoteArgs) -> Result<()> {
    let query = args.task.join(" ");

    if args.clear {
        let title = db.clear_notes(&query)?;
        println!("Cleared notes for: {}", title);
    } else if let Some(line_num) = args.delete_line {
        let task = db.resolve_task(&query)?;
        let notes = db
            .get_task_notes_by_id(&task.id)?
            .context("Task has no notes")?;
        let mut lines: Vec<&str> = notes.lines().collect();
        if line_num == 0 || line_num > lines.len() {
            anyhow::bail!("Line {} out of range (1-{})", line_num, lines.len());
        }
        lines.remove(line_num - 1);
        let joined = lines.join("\n");
        db.update_notes_by_id(&task.id, &joined)?;
        println!("Deleted line {} from: {}", line_num, task.title);
    } else if let Some(line_num) = args.edit_line {
        let task = db.resolve_task(&query)?;
        let notes = db
            .get_task_notes_by_id(&task.id)?
            .context("Task has no notes")?;
        let mut lines: Vec<String> = notes.lines().map(|l| l.to_string()).collect();
        if line_num == 0 || line_num > lines.len() {
            anyhow::bail!("Line {} out of range (1-{})", line_num, lines.len());
        }
        println!("Current: {}", lines[line_num - 1]);
        println!("Enter replacement (Ctrl+D to finish):");
        let stdin = io::stdin();
        let input: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
        if !input.is_empty() {
            lines[line_num - 1] = input.join("\n");
            let joined = lines.join("\n");
            db.update_notes_by_id(&task.id, &joined)?;
            println!("Updated line {} in: {}", line_num, task.title);
        }
    } else if args.show {
        let (title, notes) = db.get_task_notes(&query)?;
        println!("Notes for: {}", title);
        match notes {
            Some(text) => {
                for (i, line) in text.lines().enumerate() {
                    println!("  {}: {}", i + 1, line);
                }
            }
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
    Ok(())
}

fn cmd_recurring(db: &Database, args: dodo::cli::RecurringArgs) -> Result<()> {
    match args.action {
        None => {
            let templates = db.list_templates()?;
            if templates.is_empty() {
                println!(
                    "No recurring templates. Use 'dodo rec add <title> *daily' to create one."
                );
            } else {
                for t in &templates {
                    let status_icon = if t.status == TaskStatus::Paused {
                        "\u{23F8}"
                    } else {
                        "\u{21BB}"
                    };
                    let recurrence = t.recurrence.as_deref().unwrap_or("?");
                    let last_date = db.template_last_date(&t.id)?;
                    let last_str = last_date
                        .map(|d| d.format("%b %d").to_string())
                        .unwrap_or_else(|| "-".into());
                    let next_str = if t.status == TaskStatus::Paused {
                        "(paused)".to_string()
                    } else {
                        last_date
                            .and_then(|d| dodo::notation::next_occurrence(recurrence, d))
                            .map(|d| d.format("%b %d").to_string())
                            .unwrap_or_else(|| "-".into())
                    };
                    let num = t
                        .num_id
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "?".into());
                    let meta = t.display_metadata();
                    println!(
                        "[{}] {} {:<8} {}{} last:{} next:{}",
                        num, status_icon, recurrence, t.title, meta, last_str, next_str
                    );
                }
            }
        }
        Some(RecurringAction::Add(add_args)) => {
            let raw_input = add_args.title.join(" ");
            let prep = prepare_task(&raw_input);

            let recurrence = match prep.recurrence {
                Some(r) => r,
                None => anyhow::bail!("Recurrence pattern required (e.g., *daily, *3d, *weekly, *mon,wed,fri). Use * prefix."),
            };

            if prep.title == raw_input && prep.title.starts_with('*') {
                anyhow::bail!("Title is required");
            }

            let num_id = db.add_template(
                &prep.title,
                &recurrence,
                prep.project,
                prep.context,
                prep.estimate_minutes,
                prep.deadline,
                prep.scheduled,
                prep.tags,
                prep.priority,
            )?;
            println!(
                "Created recurring: {} [#{}] (*{})",
                prep.title, num_id, recurrence
            );
        }
        Some(RecurringAction::Edit(edit_args)) => {
            let raw_input = edit_args.args.join(" ");
            let parsed = parse_notation(&raw_input);

            let query = if parsed.title.is_empty() {
                anyhow::bail!("Edit requires a template identifier (numeric ID or text)");
            } else {
                parsed.title.clone()
            };

            if !parsed.has_updates() {
                anyhow::bail!("No changes specified. Use notation tokens (+project @context *pattern ~estimate etc.)");
            }

            let template = db.resolve_template(&query)?;
            db.update_template_fields(&template.id, &parsed)?;
            println!("Updated recurring: {}", template.title);
        }
        Some(RecurringAction::Delete(args)) => {
            let query = args.query.join(" ");
            let template = db.resolve_template(&query)?;
            db.delete_template(&template.id)?;
            println!("Deleted recurring: {}", template.title);
        }
        Some(RecurringAction::Pause(args)) => {
            let query = args.query.join(" ");
            let template = db.resolve_template(&query)?;
            db.pause_template(&template.id)?;
            println!("Paused: {}", template.title);
        }
        Some(RecurringAction::Resume(args)) => {
            let query = args.query.join(" ");
            let template = db.resolve_template(&query)?;
            db.resume_template(&template.id)?;
            println!("Resumed: {}", template.title);
        }
        Some(RecurringAction::Generate) => {
            let count = db.generate_instances()?;
            println!("Generated {} instance(s)", count);
        }
        Some(RecurringAction::History(args)) => {
            let query = args.query.join(" ");
            let template = db.resolve_template(&query)?;
            let history = db.template_history(&template.id)?;
            if history.is_empty() {
                println!("No completed instances for: {}", template.title);
            } else {
                println!("History for: {}", template.title);
                for task in &history {
                    let completed = task
                        .completed
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "-".into());
                    let elapsed = task.elapsed_seconds.unwrap_or(0);
                    let time_str = if elapsed > 0 {
                        format!(" ({})", format_duration(elapsed))
                    } else {
                        String::new()
                    };
                    println!("  {} {}{}", completed, task.title, time_str);
                }
            }
        }
    }
    Ok(())
}

fn cmd_config(args: dodo::cli::ConfigArgs) -> Result<()> {
    use dodo::cli::ConfigAction;
    match args.action {
        None | Some(ConfigAction::Show) => {
            let config = Config::load()?;
            let toml_str = toml::to_string_pretty(&config).context("Failed to serialize config")?;
            println!("{}", toml_str);
        }
        Some(ConfigAction::Path) => {
            let path = Config::config_path()?;
            println!("{}", path.display());
        }
    }
    Ok(())
}

fn cmd_report(db: &Database, args: dodo::cli::ReportArgs) -> Result<()> {
    let (from, to) = args.range.date_range();

    let tasks_done = db.report_tasks_done(&from, &to)?;
    let total_seconds = db.report_total_seconds(&from, &to)?;
    let active_days = db.report_active_days(&from, &to)?;
    let by_project = db.report_by_project(&from, &to)?;
    let by_weekday = db.report_by_weekday(&from, &to)?;
    let done_tasks = db.report_done_tasks(&from, &to, 10)?;

    // Header
    println!(
        "{}",
        format!("Report: {}", args.range.label()).cyan().bold()
    );
    println!();

    // Summary
    let total_h = total_seconds / 3600;
    let total_m = (total_seconds % 3600) / 60;
    println!("  Tasks completed:  {}", tasks_done);
    println!("  Total tracked:    {}h {}m", total_h, total_m);
    println!("  Active days:      {}", active_days);

    // By project
    if !by_project.is_empty() {
        println!();
        println!("{}", "  By project:".bold());
        let max_secs = by_project.iter().map(|(_, s)| *s).max().unwrap_or(1).max(1);
        for (project, secs) in &by_project {
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            let bar_len = (*secs as f64 / max_secs as f64 * 20.0) as usize;
            let bar = "\u{2588}".repeat(bar_len);
            println!("    {:<16} {:>2}h {:>2}m  {}", project, h, m, bar.green());
        }
    }

    // By weekday
    if !by_weekday.is_empty() {
        println!();
        println!("{}", "  By weekday:".bold());
        let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let max_secs = by_weekday.iter().map(|(_, s)| *s).max().unwrap_or(1).max(1);
        for (dow, secs) in &by_weekday {
            let name = day_names.get(*dow as usize).unwrap_or(&"???");
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            let bar_len = (*secs as f64 / max_secs as f64 * 20.0) as usize;
            let bar = "\u{2588}".repeat(bar_len);
            println!("    {}  {:>2}h {:>2}m  {}", name, h, m, bar.cyan());
        }
    }

    // Recent completions
    if !done_tasks.is_empty() {
        println!();
        println!("{}", "  Recent completions:".bold());
        for (title, secs) in &done_tasks {
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            println!("    {:<32} {:>2}h {:>2}m", title, h, m);
        }
    }

    Ok(())
}

fn cmd_sync(_db: &Database, args: dodo::cli::SyncArgs) -> Result<()> {
    let config = Config::load()?;
    match args.action {
        Some(SyncAction::Now) => {
            if !config.sync.is_ready() {
                anyhow::bail!("Sync not configured. Run: dodo sync enable");
            }
            // Safety: is_ready() guarantees turso_url and turso_token are Some
            let url = config.sync.turso_url.as_deref().unwrap();
            let token = config.sync.turso_token.as_deref().unwrap();
            println!("Syncing...");
            Database::do_remote_sync(url, token)?;
            println!("Synced.");
            return Ok(());
        }
        None | Some(SyncAction::Status) => {
            if config.sync.enabled {
                println!("Sync: {}", "enabled".green());
                if let Some(ref url) = config.sync.turso_url {
                    println!("  URL: {}", url);
                }
                println!(
                    "  Token: {}",
                    if config.sync.turso_token.is_some() {
                        "configured"
                    } else {
                        "not set"
                    }
                );
            } else {
                println!("Sync: {}", "disabled".dimmed());
                println!("Run 'dodo sync enable' to set up Turso sync.");
            }
        }
        Some(SyncAction::Enable) => {
            let mut config = config;
            println!("Enter Turso database URL (e.g., libsql://mydb-user.turso.io):");
            let mut url = String::new();
            io::stdin().read_line(&mut url)?;
            let url = url.trim().to_string();

            println!("Enter auth token (or set DODO_TURSO_TOKEN env var):");
            let mut token = String::new();
            io::stdin().read_line(&mut token)?;
            let token = token.trim().to_string();

            config.sync.enabled = true;
            config.sync.turso_url = Some(url);
            if !token.is_empty() {
                config.sync.turso_token = Some(token);
            }
            config.save()?;
            println!("{}", "Sync enabled.".green());
            println!("Run 'dodo sync now' to perform the first sync.");
        }
        Some(SyncAction::Disable) => {
            let mut config = config;
            config.sync.enabled = false;
            config.save()?;
            let _ = Database::clean_sync_metadata();
            let _ = Database::clean_sync_db();
            println!("Sync disabled.");
        }
    }
    Ok(())
}

fn cmd_backup(args: dodo::cli::BackupArgs) -> Result<()> {
    let config = Config::load()?;
    match args.action {
        None => {
            if !config.backup.is_ready() {
                anyhow::bail!(
                    "Backup is not configured. Add [backup] section to ~/.config/dodo/config.toml"
                );
            }
            println!("Creating backup...");
            let key = backup::create_backup(&config.backup)?;
            println!("{} {}", "Backup created:".green(), key);
        }
        Some(BackupAction::List) => {
            if !config.backup.is_ready() {
                anyhow::bail!("Backup is not configured.");
            }
            let entries = backup::list_backups(&config.backup)?;
            if entries.is_empty() {
                println!("No backups found.");
            } else {
                for (i, entry) in entries.iter().enumerate() {
                    println!(
                        "  {}. {}  ({}, {})",
                        i + 1,
                        entry.display_name,
                        backup::format_size(entry.size),
                        backup::format_age(&entry.timestamp),
                    );
                }
            }
        }
        Some(BackupAction::Restore(restore_args)) => {
            if !config.backup.is_ready() {
                anyhow::bail!("Backup is not configured.");
            }
            let entries = backup::list_backups(&config.backup)?;
            if entries.is_empty() {
                anyhow::bail!("No backups available to restore.");
            }

            let key = if restore_args.target == "latest" {
                entries[0].key.clone()
            } else {
                entries
                    .iter()
                    .find(|e| {
                        e.display_name.contains(&restore_args.target)
                            || e.key.contains(&restore_args.target)
                    })
                    .map(|e| e.key.clone())
                    .context("No backup matching that name")?
            };

            println!(
                "{}",
                "WARNING: This will replace your local database."
                    .red()
                    .bold()
            );
            println!("Current data will be saved as .pre-restore.");
            println!("Proceed? (y/n): ");
            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?;
            if confirm.trim().to_lowercase() != "y" {
                println!("Cancelled.");
                return Ok(());
            }

            println!("Restoring...");
            backup::restore_backup(&config.backup, &key)?;
            println!("{}", "Restored successfully.".green());
        }
        Some(BackupAction::Delete(delete_args)) => {
            if !config.backup.is_ready() {
                anyhow::bail!("Backup is not configured.");
            }
            let entries = backup::list_backups(&config.backup)?;
            let key = entries
                .iter()
                .find(|e| {
                    e.display_name.contains(&delete_args.name) || e.key.contains(&delete_args.name)
                })
                .map(|e| e.key.clone())
                .context("No backup matching that name")?;

            backup::delete_backup(&config.backup, &key)?;
            println!("Deleted backup: {}", key);
        }
    }
    Ok(())
}

fn apply_filters<'a>(
    tasks: &[&'a Task],
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
    let today = dodo::today();
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
    let is_overdue = task.status != TaskStatus::Done && task.deadline.is_some_and(|dl| dl < today);
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

fn cmd_email(db: &Database, args: dodo::cli::EmailArgs) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    match args.action {
        EmailAction::Digest => {
            if !config.email.is_ready() {
                anyhow::bail!(
                    "Email not configured. Run 'dodo tui' and edit Settings > Email section."
                );
            }
            dodo::email::send_digest(&config.email, db)?;
            println!("Digest email sent successfully.");
        }
        EmailAction::Cron => {
            dodo::email::print_cron_entry(&config.email);
        }
        EmailAction::Test => {
            if !config.email.is_ready() {
                anyhow::bail!(
                    "Email not configured. Run 'dodo tui' and edit Settings > Email section."
                );
            }
            dodo::email::send_test(&config.email)?;
            println!("Test email sent successfully.");
        }
    }
    Ok(())
}
