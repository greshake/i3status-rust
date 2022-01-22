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
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `message` | Message when timer expires | No | `Pomodoro over! Take a break!`
//! `break_message` | Message when break is over | No | `Break over! Time to work!`
//! `notify_cmd` | A shell command to run as a notifier. `{msg}` will be substituted with either `message` or `break_message`. | No | `swaynag -m '{msg}'`
//! `blocking_cmd` | Is `notify_cmd` blocking? If it is, then pomodoro block will wait until the command finishes before proceeding. Otherwise, you will have to click on the block in order to proceed. | No | `true`
//!
//! # Example
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
//!
//! # Icons Used
//! - `pomodoro`
//! - `pomodoro_started`
//! - `pomodoro_stopped`
//! - `pomodoro_paused`
//! - `pomodoro_break`
//!
//! # TODO
//! - Use different icons.
//! - Use format strings.

use super::prelude::*;
use crate::subprocess::{spawn_shell, spawn_shell_sync};
use std::time::Instant;
use tokio::sync::mpsc;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
struct PomodoroConfig {
    message: String,
    break_message: String,
    notify_cmd: Option<String>,
    blocking_cmd: bool,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            message: "Pomodoro over! Take a break!".into(),
            break_message: "Break over! Time to work!".into(),
            notify_cmd: Some("swaynag -m '{msg}'".into()),
            blocking_cmd: true,
        }
    }
}

struct Block {
    api: CommonApi,
    block_config: PomodoroConfig,
    events_receiver: mpsc::Receiver<BlockEvent>,
}

impl Block {
    async fn set_text(&mut self, text: String) -> Result<()> {
        self.api.set_text(text);
        self.api.flush().await
    }

    async fn wait_for_click(&mut self, button: MouseButton) {
        loop {
            if let Some(BlockEvent::Click(click)) = self.events_receiver.recv().await {
                if click.button == button {
                    break;
                }
            }
        }
    }

    async fn read_params(&mut self) -> Result<(Duration, Duration, u64)> {
        let task_len = self.read_u64(25, "Task length:").await?;
        let break_len = self.read_u64(5, "Break length:").await?;
        let pomodoros = self.read_u64(4, "Pomodoros:").await?;
        Ok((
            Duration::from_secs(task_len * 60),
            Duration::from_secs(break_len * 60),
            pomodoros,
        ))
    }

    async fn read_u64(&mut self, mut number: u64, msg: &str) -> Result<u64> {
        loop {
            self.set_text(format!("{} {}", msg, number).into()).await?;
            if let Some(BlockEvent::Click(click)) = self.events_receiver.recv().await {
                match click.button {
                    MouseButton::Left => break,
                    MouseButton::WheelUp => number += 1,
                    MouseButton::WheelDown => number = number.saturating_sub(1),
                    _ => (),
                }
            }
        }
        Ok(number)
    }

    async fn run_pomodoro(
        &mut self,
        task_len: Duration,
        break_len: Duration,
        pomodoros: u64,
    ) -> Result<()> {
        for pomodoro in 0..pomodoros {
            // Task timer
            self.api.set_state(State::Idle);
            let timer = Instant::now();
            loop {
                let elapsed = timer.elapsed();
                if elapsed >= task_len {
                    break;
                }
                let left = task_len - elapsed;
                let text = if pomodoro == 0 {
                    format!("{} min", (left.as_secs() + 59) / 60,)
                } else {
                    format!(
                        "{} {} min",
                        "|".repeat(pomodoro as usize),
                        (left.as_secs() + 59) / 60,
                    )
                };
                self.set_text(text.into()).await?;
                tokio::select! {
                    _ = sleep(Duration::from_secs(10)) => (),
                    Some(BlockEvent::Click(click)) = self.events_receiver.recv() => {
                        if click.button == MouseButton::Middle {
                            return Ok(());
                        }
                    }
                }
            }

            // Show break message
            self.api.set_state(State::Good);
            self.set_text(self.block_config.message.clone()).await?;
            if let Some(cmd) = &self.block_config.notify_cmd {
                let cmd = cmd.replace("{msg}", &self.block_config.message);
                if self.block_config.blocking_cmd {
                    spawn_shell_sync(&cmd)
                        .await
                        .error("failed to run notify_cmd")?;
                } else {
                    spawn_shell(&cmd).error("failed to run notify_cmd")?;
                    self.wait_for_click(MouseButton::Left).await;
                }
            } else {
                self.wait_for_click(MouseButton::Left).await;
            }

            // No break after the last pomodoro
            if pomodoro == pomodoros - 1 {
                break;
            }

            // Break timer
            let timer = Instant::now();
            loop {
                let elapsed = timer.elapsed();
                if elapsed >= break_len {
                    break;
                }
                let left = break_len - elapsed;
                self.set_text(format!("Break: {} min", (left.as_secs() + 59) / 60,).into())
                    .await?;
                tokio::select! {
                    _ = sleep(Duration::from_secs(10)) => (),
                    Some(BlockEvent::Click(click)) = self.events_receiver.recv() => {
                        if click.button == MouseButton::Middle {
                            return Ok(());
                        }
                    }
                }
            }

            // Show task message
            self.api.set_state(State::Good);
            self.set_text(self.block_config.break_message.clone())
                .await?;
            if let Some(cmd) = &self.block_config.notify_cmd {
                let cmd = cmd.replace("{msg}", &self.block_config.break_message);
                if self.block_config.blocking_cmd {
                    spawn_shell_sync(&cmd)
                        .await
                        .error("failed to run notify_cmd")?;
                } else {
                    spawn_shell(&cmd).error("failed to run notify_cmd")?;
                    self.wait_for_click(MouseButton::Left).await;
                }
            } else {
                self.wait_for_click(MouseButton::Left).await;
            }
        }

        Ok(())
    }
}

pub async fn run(block_config: toml::Value, mut api: CommonApi) -> Result<()> {
    let events = api.get_events().await?;
    let block_config = PomodoroConfig::deserialize(block_config).config_error()?;
    api.set_icon("pomodoro")?;
    let mut block = Block {
        api,
        block_config,
        events_receiver: events,
    };

    loop {
        // Send collaped block
        block.api.set_state(State::Idle);
        block.set_text(String::new()).await?;

        // Wait for left click
        block.wait_for_click(MouseButton::Left).await;

        // Read params
        let (task_len, break_len, pomodoros) = block.read_params().await?;

        // Run!
        block.run_pomodoro(task_len, break_len, pomodoros).await?;
    }
}
