use chan::Sender;
use scheduler::Task;
use std::process::Command;
use std::time::Duration;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use input::{I3BarEvent, MouseButton};
use widget::{I3BarWidget, State};
use widgets::button::ButtonWidget;

use uuid::Uuid;

pub struct Pacman2 {
    output: ButtonWidget,
    id: String,
    update_interval: Duration,
}

struct UpdateCount {
    official: usize,
    aur: usize,
}

impl UpdateCount {
    fn new(official: usize, aur: usize) -> UpdateCount {
        UpdateCount { official, aur }
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct Pacman2Config {
    /// Update interval in seconds
    #[serde(default = "Pacman2Config::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl Pacman2Config {
    fn default_interval() -> Duration {
        Duration::from_secs(60 * 10)
    }
}

impl ConfigBlock for Pacman2 {
    type Config = Pacman2Config;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Pacman2 {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            output: ButtonWidget::new(config, "pacman2").with_icon("update"),
        })
    }
}

fn get_update_count() -> Result<UpdateCount> {
    let checkupdates_output = Command::new("checkupdates")
        .env("LANG", "en_US")
        .env("LC_ALL", "en_US")
        .output()
        .block_error("pacman2", "failed to run checkupdates")?
        .stdout;
    let cower = Command::new("cower")
        .arg("-u")
        .env("LANG", "en_US")
        .env("LC_ALL", "en_US")
        .output()
        .block_error("pacman2", "failed to run cower -u")
        .unwrap();
    // if we are offline, cower will print a bunch of warnings to stderr but exit with 0
    // if we are online, and no updates are found, cower will print nothing and exit with 0
    // if we are online and cower found updates, we will have updates in stdout and exit status 1
    // checkupdates will just silently ignore network failure, no need to handle this
    let cower_exit_code: i32 = cower.status.code().unwrap();
    let cower_update_count = match cower_exit_code {
        0 => 0,
        _ => {
            let cower_output = cower.stdout;
            String::from_utf8_lossy(&cower_output).lines().count()
        }
    };

    let checkupdates_count = String::from_utf8_lossy(&checkupdates_output).lines().count();

    let update_count = UpdateCount::new(checkupdates_count, cower_update_count);

    Ok(update_count)
}

impl Block for Pacman2 {
    fn update(&mut self) -> Result<Option<Duration>> {
        let count = get_update_count()?;
        self.output.set_text(format!("{} {}", count.official, count.aur));
        self.output.set_state(match count.official + count.aur {
            0 => State::Idle,
            _ => State::Info,
        });
        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if event.name.as_ref().map(|s| s == "pacman2").unwrap_or(false) && event.button == MouseButton::Left {
            self.update()?;
        }

        Ok(())
    }
}
