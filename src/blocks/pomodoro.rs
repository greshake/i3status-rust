//! A [pomodoro timer](https://en.wikipedia.org/wiki/Pomodoro_Technique)
//!
//! # Technique
//!
//! There are six steps in the original technique:
//! 1) Decide on the task to be done.
//! 2) Set the pomodoro timer (traditionally to 25 minutes).
//! 3) Work on the task.
//! 4) End work when the timer rings and put a checkmark on a piece of paper.
//! 5) If you have fewer than four checkmarks, take a short break (3–5 minutes) and then return to step 2.
//! 6) After four pomodoros, take a longer break (15–30 minutes), reset your checkmark count to zero, then go to step 1.
//!
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | The format used when in idle, prompt, or notify states | <code>\" $icon{ $message\|} \"</code>
//! `pomodoro_format` | The format used when the pomodoro is running or paused | <code>\" $icon $status_icon{ $pomodoro_tally\|}{ pomodoro $pomodoro\|} $time_remaining min \"</code>
//! `break_format` |The format used when the pomodoro is during the break | <code>\" $icon $status_icon Break: $time_remaining min \"</code>
//! `message` | Message when timer expires | `"Pomodoro over! Take a break!"`
//! `break_message` | Message when break is over | `"Break over! Time to work!"`
//! `notify_cmd` | A shell command to run as a notifier. `{msg}` will be substituted with either `message` or `break_message`. | `None`
//! `blocking_cmd` | Is `notify_cmd` blocking? If it is, then pomodoro block will wait until the command finishes before proceeding. Otherwise, you will have to click on the block in order to proceed. | `false`
//!
//! Placeholder      | Value                                         | Type   | Supported by
//! -----------------|-----------------------------------------------|--------|--------------
//! `icon`           | A static icon                                 | Icon   | All formats
//! `status_icon`    | An icon that reflects the pomodoro state      | Icon   | `pomodoro_format`, `break_format`
//! `message`        | Current message                               | Text   | `format`
//! `time_remaining` | How much time is left (minutes)               | Number | `pomodoro_format`, `break_format`
//! `pomodoro`       | The pomodoro count                            | Number | `pomodoro_format`
//! `pomodoro_tally` | The pomodoro count represented as tally marks | Text   | `pomodoro_format`
//!
//! # Example
//!
//! Use `swaynag` as a notifier:
//!
//! ```toml
//! [[block]]
//! block = "pomodoro"
//! notify_cmd = "swaynag -m '{msg}'"
//! blocking_cmd = true
//! ```
//!
//! Use `notify-send` as a notifier:
//!
//! ```toml
//! [[block]]
//! block = "pomodoro"
//! notify_cmd = "notify-send '{msg}'"
//! blocking_cmd = false
//! ```
//!
//! # Icons Used
//! - `pomodoro`
//! - `pomodoro_started`
//! - `pomodoro_stopped`
//! - `pomodoro_paused`
//! - `pomodoro_break`

use num_traits::{Num, NumAssignOps, SaturatingSub};
use tokio::sync::mpsc;

use super::prelude::*;
use crate::{
    formatting::Format,
    subprocess::{spawn_shell, spawn_shell_sync},
};
use std::time::Instant;

make_log_macro!(debug, "pomodoro");

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    pub pomodoro_format: FormatConfig,
    pub break_format: FormatConfig,
    #[default("Pomodoro over! Take a break!".into())]
    pub message: String,
    #[default("Break over! Time to work!".into())]
    pub break_message: String,
    pub notify_cmd: Option<String>,
    pub blocking_cmd: bool,
}

enum PomodoroState {
    Idle,
    Prompt,
    Notify,
    Break,
    PomodoroRunning,
    PomodoroPaused,
}

impl PomodoroState {
    fn get_block_state(&self) -> State {
        use PomodoroState::*;
        match self {
            Idle | PomodoroPaused => State::Idle,
            Prompt => State::Warning,
            Notify => State::Good,
            Break | PomodoroRunning => State::Info,
        }
    }

    fn get_status_icon(&self) -> Option<Cow<'static, str>> {
        use PomodoroState::*;
        match self {
            Idle => Some("pomodoro_stopped".into()),
            Break => Some("pomodoro_break".into()),
            PomodoroRunning => Some("pomodoro_started".into()),
            PomodoroPaused => Some("pomodoro_paused".into()),
            _ => None,
        }
    }
}

struct Block<'a> {
    widget: Widget,
    actions: mpsc::UnboundedReceiver<BlockAction>,
    api: &'a CommonApi,
    config: &'a Config,
    state: PomodoroState,
    format: Format,
    pomodoro_format: Format,
    break_format: Format,
}

impl Block<'_> {
    async fn set_text(&mut self, additional_values: Values) -> Result<()> {
        let mut values = map! {
            "icon" => Value::icon("pomodoro"),
        };
        values.extend(additional_values);

        if let Some(icon) = self.state.get_status_icon() {
            values.insert("status_icon".into(), Value::icon(icon));
        }
        self.widget.set_format(match self.state {
            PomodoroState::Idle | PomodoroState::Prompt | PomodoroState::Notify => {
                self.format.clone()
            }
            PomodoroState::Break => self.break_format.clone(),
            PomodoroState::PomodoroRunning | PomodoroState::PomodoroPaused => {
                self.pomodoro_format.clone()
            }
        });
        self.widget.state = self.state.get_block_state();
        debug!("{:?}", values);
        self.widget.set_values(values);
        self.api.set_widget(self.widget.clone())
    }

    async fn wait_for_click(&mut self, button: &str) -> Result<()> {
        while self.actions.recv().await.error("channel closed")? != button {}
        Ok(())
    }

    async fn read_params(&mut self) -> Result<(Duration, Duration, usize)> {
        self.state = PomodoroState::Prompt;
        let task_len = self.read_number(25, "Task length:").await?;
        let break_len = self.read_number(5, "Break length:").await?;
        let pomodoros = self.read_number(4, "Pomodoros:").await?;
        Ok((
            Duration::from_secs(task_len * 60),
            Duration::from_secs(break_len * 60),
            pomodoros,
        ))
    }

    async fn read_number<T: Num + NumAssignOps + SaturatingSub + std::fmt::Display>(
        &mut self,
        mut number: T,
        msg: &str,
    ) -> Result<T> {
        loop {
            self.set_text(map! {"message" => Value::text(format!("{msg} {number}"))})
                .await?;
            match &*self.actions.recv().await.error("channel closed")? {
                "_left" => break,
                "_up" => number += T::one(),
                "_down" => number = number.saturating_sub(&T::one()),
                _ => (),
            }
        }
        Ok(number)
    }

    async fn set_notification(&mut self, message: &str) -> Result<()> {
        self.state = PomodoroState::Notify;
        self.set_text(map! {"message" => Value::text(message.to_string())})
            .await?;
        if let Some(cmd) = &self.config.notify_cmd {
            let cmd = cmd.replace("{msg}", message);
            if self.config.blocking_cmd {
                spawn_shell_sync(&cmd)
                    .await
                    .error("failed to run notify_cmd")?;
            } else {
                spawn_shell(&cmd).error("failed to run notify_cmd")?;
                self.wait_for_click("_left").await?;
            }
        } else {
            self.wait_for_click("_left").await?;
        }
        Ok(())
    }

    async fn run_pomodoro(
        &mut self,
        task_len: Duration,
        break_len: Duration,
        pomodoros: usize,
    ) -> Result<()> {
        for pomodoro in 0..pomodoros {
            let mut total_elapsed = Duration::ZERO;
            'pomodoro_run: loop {
                // Task timer
                self.state = PomodoroState::PomodoroRunning;
                let timer = Instant::now();
                loop {
                    total_elapsed += timer.elapsed();
                    if total_elapsed >= task_len {
                        break 'pomodoro_run;
                    }
                    let left = task_len - total_elapsed;
                    let values = map! {
                        [if pomodoro != 0] "pomodoro_tally" => Value::text("|".repeat(pomodoro)),
                        [if pomodoro != 0] "pomodoro" => Value::number(pomodoro),
                        "time_remaining" => Value::number((left.as_secs() + 59) / 60),
                    };
                    self.set_text(values.clone()).await?;
                    select! {
                        _ = sleep(Duration::from_secs(10)) => (),
                        Some(action) = self.actions.recv() => match action.as_ref() {
                            "_middle" => return Ok(()),
                            "_right" => {
                                debug!("right");
                                self.state = PomodoroState::PomodoroPaused;
                                self.set_text(values).await?;
                                loop {
                                    match self.actions.recv().await.as_deref() {
                                        Some("_middle") => return Ok(()),
                                        Some("_right") =>  {
                                            continue 'pomodoro_run;
                                        },
                                        _ => ()

                                    }
                                }
                            },
                            _ => ()
                        }
                    }
                }
            }

            // Show break message
            self.set_notification(&self.config.message).await?;

            // No break after the last pomodoro
            if pomodoro == pomodoros - 1 {
                break;
            }

            // Break timer
            self.state = PomodoroState::Break;
            let timer = Instant::now();
            loop {
                let elapsed = timer.elapsed();
                if elapsed >= break_len {
                    break;
                }
                let left = break_len - elapsed;
                self.set_text(map! {
                    "time_remaining" => Value::number((left.as_secs() + 59) / 60),
                })
                .await?;
                select! {
                    _ = sleep(Duration::from_secs(10)) => (),
                    _ = self.wait_for_click("_middle") => return Ok(()),
                }
            }

            // Show task message
            self.set_notification(&self.config.break_message).await?;
        }

        Ok(())
    }
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    api.set_default_actions(&[
        (MouseButton::Left, None, "_left"),
        (MouseButton::Middle, None, "_middle"),
        (MouseButton::Right, None, "_right"),
        (MouseButton::WheelUp, None, "_up"),
        (MouseButton::WheelDown, None, "_down"),
    ])?;

    let format = config.format.clone().with_default(" $icon{ $message|} ")?;

    let pomodoro_format = config.pomodoro_format.clone().with_default(
        " $icon $status_icon{ $pomodoro_tally|}{ pomodoro $pomodoro|} $time_remaining min ",
    )?;

    let break_format = config
        .break_format
        .clone()
        .with_default(" $icon $status_icon Break: $time_remaining min ")?;

    let widget = Widget::new();

    let mut block = Block {
        widget,
        actions: api.get_actions()?,
        api,
        config,
        state: PomodoroState::Idle,
        format,
        pomodoro_format,
        break_format,
    };

    loop {
        // Send collaped block
        block.state = PomodoroState::Idle;
        block.set_text(Values::default()).await?;

        block.wait_for_click("_left").await?;

        let (task_len, break_len, pomodoros) = block.read_params().await?;
        block.run_pomodoro(task_len, break_len, pomodoros).await?;
    }
}
