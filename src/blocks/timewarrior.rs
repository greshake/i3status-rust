//! Time and information of the current timewarrior task
//!
//! Clicking left mouse stops or resumes the task
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Update interval in seconds | `30`
//! `format` | A string to customise the output of the block. See placeholders. | <code>" $icon {$elapsed &vert;}"</code>
//! `info` | The threshold of minutes the task turns into a info state | -
//! `good` | The threshold of minutes the task turns into a good state | -
//! `warning` | The threshold of minutes the task turns into a warning state | -
//! `critical` | The threshold of minutes the task turns into a critical state | -
//!
//! Placeholder | Value | Type | Unit
//! ------------|-------|------|------
//! `icon`   | A static icon | Icon | -
//! `elapsed`| Elapsed time in format H:MM (Only present if task is active) | Text | -
//! `tags`   | Tags of the active task separated by space (Only present if task is active) | Text | -
//! `annotation` | Annotation of the active task (Only present if task is active) | Text | -
//!
//! Action          | Default button
//! ----------------|----------------
//! `stop_continue` | Left
//!
//! # Example
//! ```toml
//! [[block]]
//! block  = "timewarrior"
//! format = " $icon {$tags.str(w:8,rot_interval:4) $elapsed|}"
//! ```
//!
//! # Icons Used
//! - `tasks`

use super::prelude::*;
use chrono::DateTime;
use tokio::process::Command;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    #[default(30.into())]
    interval: Seconds,
    format: FormatConfig,

    info: Option<u64>,
    good: Option<u64>,
    warning: Option<u64>,
    critical: Option<u64>,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "stop_continue")])?;

    let widget = Widget::new().with_format(config.format.with_default(" $icon {$elapsed|}")?);

    loop {
        let mut values = map! {
            "icon" => Value::icon("tasks"),
        };
        let mut state = State::Idle;
        let mut widget = widget.clone();

        let data = get_current_timewarrior_task().await?;
        if let Some(tw) = data {
            if tw.end.is_none() {
                // only show active tasks
                let elapsed = chrono::Utc::now() - tw.start;

                // calculate state
                for (level, st) in [
                    (&config.critical, State::Critical),
                    (&config.warning, State::Warning),
                    (&config.good, State::Good),
                    (&config.info, State::Info),
                ] {
                    if let Some(value) = level {
                        if (elapsed.num_minutes() as u64) >= *value {
                            state = st;
                            break;
                        }
                    }
                }

                values.insert("tags".into(), Value::text(tw.tags.join(" ")));

                let elapsedstr =
                    format!("{}:{:0>2}", elapsed.num_hours(), elapsed.num_minutes() % 60);
                values.insert("elapsed".into(), Value::text(elapsedstr));

                if let Some(annotation) = tw.annotation {
                    values.insert("annotation".into(), Value::text(annotation));
                }
            }
        }

        widget.state = state;
        widget.set_values(values);
        api.set_widget(widget)?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
            Some(action) = actions.recv() => match action.as_ref() {
                "stop_continue" => { stop_continue().await?; }
                _ => (),
            }
        }
    }
}

/// Raw output from timew
#[derive(Deserialize, Debug)]
struct TimewarriorRAW {
    pub id: u32,
    pub start: String,
    pub tags: Vec<String>,
    pub annotation: Option<String>,
    pub end: Option<String>,
}

/// TimeWarrior entry
#[derive(Debug, PartialEq, Deserialize)]
#[serde(from = "TimewarriorRAW")]
struct TimewarriorData {
    pub id: u32,
    pub start: DateTime<chrono::offset::Utc>,
    pub tags: Vec<String>,
    pub annotation: Option<String>,
    pub end: Option<DateTime<chrono::offset::Utc>>,
}

impl From<TimewarriorRAW> for TimewarriorData {
    fn from(item: TimewarriorRAW) -> Self {
        Self {
            id: item.id,
            tags: item.tags,
            annotation: item.annotation,
            start: chrono::TimeZone::from_utc_datetime(
                &chrono::Utc,
                &chrono::NaiveDateTime::parse_from_str(&item.start, "%Y%m%dT%H%M%SZ").unwrap()
            ),
            end: item.end.map(|v| {
                chrono::TimeZone::from_utc_datetime(
                    &chrono::Utc,
                    &chrono::NaiveDateTime::parse_from_str(&v, "%Y%m%dT%H%M%SZ").unwrap()
                )
            }),
        }
    }
}

/// Format a DateTime given a format string
#[allow(dead_code)]
fn format_datetime(date: &DateTime<chrono::Utc>, format: &str) -> String {
    date.format(format).to_string()
}

/// Execute "timew export now" and return the current task (if any)
async fn get_current_timewarrior_task() -> Result<Option<TimewarriorData>> {
    let out = Command::new("timew")
        .args(["export", "now"])
        .output()
        .await
        .error("failed to run timewarrior")?
        .stdout;
    Ok(serde_json::from_slice::<Vec<TimewarriorData>>(&out)
        .unwrap_or_default()
        .into_iter()
        .next())
}

/// Stop or continue a task
async fn stop_continue() -> Result<()> {
    let is_stopped = get_current_timewarrior_task()
        .await?
        .map_or(true, |tw| tw.end.is_some());
    let args = if is_stopped { "continue" } else { "stop" };
    Command::new("timew")
        .args([args])
        .stdout(std::process::Stdio::null())
        .spawn()
        .error("Error spawning timew")?
        .wait()
        .await
        .error("Error executing stop/continue")?
        .success()
        .then_some(())
        .error("timew exited with non-zero value when attempting to stop/continue")
}
