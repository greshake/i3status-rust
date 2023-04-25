//! Time and information of the current timewarrior task
//!
//! Clicking left mouse stops or resumes the task
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `interval` | Update interval in seconds | `30`
//! `format` | A string to customise the output of the block. See placeholders. | `" $icon {$elapsed\|}"`
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
//! # Example
//! ```toml
//! [[block]]
//! block  = "timewarrior"
//! format = " $icon {$tags.rot-str(8,1) $elapsed|}"
//! ```
//!
//! # Icons Used
//! - `tasks`

use super::prelude::*;
use tokio::process::Command;
use chrono::DateTime;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    #[default(30.into())]
    interval : Seconds,
    format: FormatConfig,

    info: Option<u64>,
    good: Option<u64>,
    warning: Option<u64>,
    critical: Option<u64>,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {

    api.set_default_actions(&[
        (MouseButton::Left, None, "stop_continue"),
    ])
    .await?;

    let widget = Widget::new().with_format(
        config
            .format
            .with_default(" $icon {$elapsed|}")?,
    );


    loop {
        let mut values = map! {
            "icon" => Value::icon(api.get_icon("tasks")?),
        };
        let mut state = State::Idle;
        let mut widget = widget.clone();

        let data = process_timewarrior_data(&call_timewarrior().await?);
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

                let elapsedstr = format!("{}:{:0>2}",
                                         elapsed.num_hours(),
                                         elapsed.num_minutes()%60);
                values.insert("elapsed".into(), Value::text(elapsedstr));

                if let Some(annotation) = tw.annotation {
                    values.insert("annotation".into(), Value::text(annotation));
                }
            }
        }

        widget.state = state;
        widget.set_values(values);
        api.set_widget(widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            event = api.event() => match event {
                UpdateRequest => (),
                Action(a) => {
                    if a == "stop_continue" {
                        stop_continue().await?;
                    }
                },
            }
        }
    }
}

/// Raw output from timew
#[derive(Deserialize, Debug)]
struct TimewarriorRAW {
    pub id : u32,
    pub start : String,
    pub tags : Vec<String>,
    pub annotation : Option<String>,
    pub end : Option<String>,
}

/// TimeWarrior entry
#[derive(Debug, PartialEq)]
struct TimewarriorData {
    pub id : u32,
    pub start : DateTime<chrono::offset::Utc>,
    pub tags : Vec<String>,
    pub annotation : Option<String>,
    pub end : Option<DateTime<chrono::offset::Utc>>,
}

impl From<TimewarriorRAW> for TimewarriorData {
    fn from(item:TimewarriorRAW) -> Self {
        Self {
            id: item.id,
            tags: item.tags,
            annotation: item.annotation,
            start : DateTime::from_utc(
                        chrono::NaiveDateTime::parse_from_str(&item.start, "%Y%m%dT%H%M%SZ")
                            .unwrap(),
                        chrono::Utc),
            end : item.end.map(|v| DateTime::from_utc(
                                    chrono::NaiveDateTime::parse_from_str(&v, "%Y%m%dT%H%M%SZ")
                                        .unwrap(),
                                    chrono::Utc)),
        }
    }
}

/// Format a DateTime given a format string
#[allow(dead_code)]
fn format_datetime(date:&DateTime<chrono::Utc>, format:&str) -> String {
    date.format(format).to_string()
}

/// Execute "timew export now" and return the result
async fn call_timewarrior() -> Result<String> {
    let out = Command::new("timew")
        .args(["export", "now"])
        .output()
        .await
        .error("failed to run timewarrior")?
        .stdout;
    let output = std::str::from_utf8(&out)
        .error("failed to read output from timewarrior (invalid UTF-8)")?
        .trim();
    Ok(output.to_string())
}

/// Stop or continue a task
async fn stop_continue() -> Result<()> {
    let mut execute_continue:bool = true;
    if let Some(tw) = process_timewarrior_data(&call_timewarrior().await?) {
        // we only execute continue if the current task is stopped
        // i.e. has an end defined
        execute_continue = tw.end.is_some();
    }

    // is there a more rust way of doing this?
    let args = match execute_continue {
        true => "continue",
        false => "stop",
    };

    Command::new("timew")
        .args(&[args])
        .stdout(std::process::Stdio::null())
        .spawn()
        .error("Error spawing timew")?
        .wait()
        .await
        .error("Error executing stop/continue")
        .map(|_| ())
}


/// Process the output from "timew export" and return the first entry
fn process_timewarrior_data(input:&str) -> Option<TimewarriorData> {
    let t : Vec<TimewarriorRAW> = serde_json::from_str(input).unwrap_or_default();
    match t.into_iter().next() {
        Some(t) => Some(TimewarriorData::from(t)),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_timewarrior_data() {
        assert_eq!(
            process_timewarrior_data(""),
            None,
        );

        assert_eq!(
            process_timewarrior_data("[]"),
            None,
        );

        let a = process_timewarrior_data("[{\"id\":1,\"start\":\"20230131T175754Z\",\"tags\":[\"i3status\"],\"annotation\":\"timewarrior plugin\"}]");
        assert_eq!(a.is_some(), true);
        if let Some(b) = a {
            assert_eq!(b.id, 1);
            assert_eq!(b.end, None);
        }
    }
}
