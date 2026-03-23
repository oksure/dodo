pub mod backup;
pub mod cli;
pub mod config;
pub mod db;
pub mod email;
pub mod fuzzy;
pub mod notation;
pub mod session;
pub mod task;

use chrono::{Local, NaiveDate, Utc};
use std::sync::RwLock;

static TIMEZONE: RwLock<Option<chrono_tz::Tz>> = RwLock::new(None);

/// Set the timezone from config. Can be called again if the user changes the setting.
pub fn init_timezone(tz_name: Option<&str>) {
    let tz = tz_name.and_then(|name| name.parse::<chrono_tz::Tz>().ok());
    if let Ok(mut guard) = TIMEZONE.write() {
        *guard = tz;
    }
}

/// Get today's date in the configured timezone, falling back to system local.
pub fn today() -> NaiveDate {
    now_naive().date()
}

/// Get the current NaiveDateTime in the configured timezone, falling back to system local.
/// Use this for formatting timestamps (e.g., note entries, header display).
pub fn now_naive() -> chrono::NaiveDateTime {
    if let Ok(guard) = TIMEZONE.read() {
        if let Some(tz) = *guard {
            return Utc::now().with_timezone(&tz).naive_local();
        }
    }
    Local::now().naive_local()
}
