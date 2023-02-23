use std::cmp::{max, min};
use std::process::Stdio;
use tokio::process::{ChildStdout, Command};

use super::super::prelude::*;
use super::SoundDevice;

pub(super) struct Device {
    name: String,
    device: String,
    natural_mapping: bool,
    volume: u32,
    muted: bool,
    monitor: ChildStdout,
}

impl Device {
    pub(super) fn new(name: String, device: String, natural_mapping: bool) -> Result<Self> {
        Ok(Device {
            name,
            device,
            natural_mapping,
            volume: 0,
            muted: false,
            monitor: Command::new("alsactl")
                .arg("monitor")
                .stdout(Stdio::piped())
                .spawn()
                .error("Failed to start alsactl monitor")?
                .stdout
                .error("Failed to pipe alsactl monitor output")?,
        })
    }
}

#[async_trait::async_trait]
impl SoundDevice for Device {
    fn volume(&self) -> u32 {
        self.volume
    }

    fn muted(&self) -> bool {
        self.muted
    }

    fn output_name(&self) -> String {
        self.name.clone()
    }

    fn output_description(&self) -> Option<String> {
        // TODO Does Alsa has something similar like descriptions in Pulse?
        None
    }

    fn active_port(&self) -> Option<&str> {
        None
    }

    fn form_factor(&self) -> Option<&str> {
        None
    }

    async fn get_info(&mut self) -> Result<()> {
        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M");
        };
        args.extend(["-D", &self.device, "get", &self.name]);

        let output: String = Command::new("amixer")
            .args(&args)
            .output()
            .await
            .map(|o| std::str::from_utf8(&o.stdout).unwrap().trim().into())
            .error("could not run amixer to get sound info")?;

        let last_line = &output.lines().last().error("could not get sound info")?;

        const FILTER: &[char] = &['[', ']', '%'];
        let mut last = last_line
            .split_whitespace()
            .filter(|x| x.starts_with('[') && !x.contains("dB"))
            .map(|s| s.trim_matches(FILTER));

        self.volume = last
            .next()
            .error("could not get volume")?
            .parse::<u32>()
            .error("could not parse volume to u32")?;

        self.muted = last.next().map(|muted| muted == "off").unwrap_or(false);

        Ok(())
    }

    async fn set_volume(&mut self, step: i32, max_vol: Option<u32>) -> Result<()> {
        let new_vol = max(0, self.volume as i32 + step) as u32;
        let capped_volume = if let Some(vol_cap) = max_vol {
            min(new_vol, vol_cap)
        } else {
            new_vol
        };
        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M");
        };
        let vol_str = format!("{capped_volume}%");
        args.extend(["-D", &self.device, "set", &self.name, &vol_str]);

        Command::new("amixer")
            .args(&args)
            .output()
            .await
            .error("failed to set volume")?;

        self.volume = capped_volume;

        Ok(())
    }

    async fn toggle(&mut self) -> Result<()> {
        let mut args = Vec::new();
        if self.natural_mapping {
            args.push("-M");
        };
        args.extend(["-D", &self.device, "set", &self.name, "toggle"]);

        Command::new("amixer")
            .args(&args)
            .output()
            .await
            .error("failed to toggle mute")?;

        self.muted = !self.muted;

        Ok(())
    }

    async fn wait_for_update(&mut self) -> Result<()> {
        let mut buf = [0u8; 1024];
        self.monitor
            .read(&mut buf)
            .await
            .error("Failed to read stdbuf output")?;
        Ok(())
    }
}
