use anyhow::{Context, Result};

use crate::config::EmailConfig;
use crate::db::Database;
use crate::task::TaskStatus;

/// Send a daily digest email via the Resend API.
pub fn send_digest(config: &EmailConfig, db: &Database) -> Result<()> {
    let from = config
        .from
        .as_deref()
        .context("Email 'from' not configured")?;
    let to = config.to.as_deref().context("Email 'to' not configured")?;
    let api_key = config
        .api_key
        .as_deref()
        .context("Resend API key not configured")?;

    let today = crate::today();
    let today_str = today.format("%A, %B %e, %Y").to_string();

    // Gather data
    let all_tasks = db.list_tasks(None)?;
    let mut today_tasks = Vec::new();
    let mut overdue_tasks = Vec::new();
    let mut running_title: Option<String> = None;

    for task in &all_tasks {
        if task.status == TaskStatus::Done {
            continue;
        }
        if task.status == TaskStatus::Running {
            running_title = Some(task.title.clone());
        }
        let scheduled = task.scheduled.unwrap_or(today);
        if scheduled <= today {
            if scheduled < today {
                overdue_tasks.push(&task.title);
            }
            today_tasks.push(task);
        }
    }

    // Build HTML
    let mut html = format!(
        r#"<div style="font-family: -apple-system, system-ui, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
<h2 style="color: #333; border-bottom: 2px solid #89b4fa; padding-bottom: 8px;">Dodo Daily Digest</h2>
<p style="color: #666;">{}</p>"#,
        today_str
    );

    // Running task
    if let Some(ref title) = running_title {
        html.push_str(&format!(
            r#"<div style="background: #f0fdf4; border-left: 4px solid #a6e3a1; padding: 12px; margin: 16px 0; border-radius: 4px;">
<strong style="color: #166534;">Currently Running:</strong> {}
</div>"#,
            html_escape(title)
        ));
    }

    // Overdue
    if !overdue_tasks.is_empty() {
        html.push_str(r#"<h3 style="color: #f38ba8;">Overdue</h3><ul>"#);
        for title in &overdue_tasks {
            html.push_str(&format!("<li>{}</li>", html_escape(title)));
        }
        html.push_str("</ul>");
    }

    // Today's tasks
    html.push_str(r#"<h3 style="color: #89b4fa;">Today's Tasks</h3>"#);
    if today_tasks.is_empty() {
        html.push_str("<p style=\"color: #999;\">No tasks scheduled for today.</p>");
    } else {
        html.push_str("<ul>");
        for task in &today_tasks {
            let status_icon = match task.status {
                TaskStatus::Running => "▶",
                TaskStatus::Paused => "⏸",
                _ => "○",
            };
            let project = task
                .project
                .as_deref()
                .map(|p| {
                    format!(
                        " <span style=\"color: #cba6f7;\">+{}</span>",
                        html_escape(p)
                    )
                })
                .unwrap_or_default();
            let estimate = task
                .estimate_minutes
                .map(|m| format!(" <span style=\"color: #999;\">~{}m</span>", m))
                .unwrap_or_default();
            html.push_str(&format!(
                "<li>{} {}{}{}</li>",
                status_icon,
                html_escape(&task.title),
                project,
                estimate,
            ));
        }
        html.push_str("</ul>");
    }

    // Summary
    let total = all_tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Done)
        .count();
    let done_today = all_tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done && t.scheduled == Some(today))
        .count();
    html.push_str(&format!(
        r#"<div style="background: #f8f9fa; padding: 12px; margin: 16px 0; border-radius: 4px; color: #666;">
<strong>{}</strong> active tasks &middot; <strong>{}</strong> completed today &middot; <strong>{}</strong> overdue
</div>"#,
        total, done_today, overdue_tasks.len()
    ));

    html.push_str(r#"<p style="color: #999; font-size: 12px;">Sent by Dodo CLI</p></div>"#);

    // Send via Resend API
    let body = serde_json::json!({
        "from": from,
        "to": [to],
        "subject": format!("Dodo Digest: {}", today_str),
        "html": html,
    });

    let resp = ureq::post("https://api.resend.com/emails")
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .send(body.to_string().as_bytes())
        .context("Failed to send email via Resend")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_str = resp.into_body().read_to_string().unwrap_or_default();
        anyhow::bail!("Resend API error ({}): {}", status, body_str);
    }

    Ok(())
}

/// Send a test email to verify the configuration.
pub fn send_test(config: &EmailConfig) -> Result<()> {
    let from = config
        .from
        .as_deref()
        .context("Email 'from' not configured")?;
    let to = config.to.as_deref().context("Email 'to' not configured")?;
    let api_key = config
        .api_key
        .as_deref()
        .context("Resend API key not configured")?;

    let body = serde_json::json!({
        "from": from,
        "to": [to],
        "subject": "Dodo Test Email",
        "html": "<p>This is a test email from <strong>Dodo CLI</strong>. Your email configuration is working correctly.</p>",
    });

    let resp = ureq::post("https://api.resend.com/emails")
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .send(body.to_string().as_bytes())
        .context("Failed to send test email via Resend")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_str = resp.into_body().read_to_string().unwrap_or_default();
        anyhow::bail!("Resend API error ({}): {}", status, body_str);
    }

    Ok(())
}

/// Print a crontab entry for the daily digest.
pub fn print_cron_entry(config: &EmailConfig) {
    let time = &config.digest_time;
    let parts: Vec<&str> = time.split(':').collect();
    let (hour, minute) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        ("7", "0")
    };

    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "dodo".to_string());

    println!("Add this line to your crontab (crontab -e):");
    println!();
    println!("{} {} * * * {} email digest", minute, hour, exe);
    println!();
    println!("This will send a daily digest at {}.", time);
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
