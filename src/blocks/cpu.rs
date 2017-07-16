use std::time::Duration;
use chan::Sender;
use scheduler::Task;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};

use std::io::BufReader;
use std::io::prelude::*;
use std::fs::File;

use uuid::Uuid;

pub struct Cpu {
    utilization: TextWidget,
    prev_idle: u64,
    prev_non_idle: u64,
    id: String,
    update_interval: Duration,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CpuConfig {
    /// Update interval in seconds
    #[serde(default = "CpuConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,
}

impl CpuConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1)
    }
}

impl ConfigBlock for Cpu {
    type Config = CpuConfig;

    fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
        Ok(Cpu {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            utilization: TextWidget::new(config).with_icon("cpu"),
            prev_idle: 0,
            prev_non_idle: 0,
        })
    }
}

impl Block for Cpu {
    fn update(&mut self) -> Result<Option<Duration>> {
        let f = File::open("/proc/stat")
            .block_error("cpu", "Your system doesn't support /proc/stat")?;
        let f = BufReader::new(f);

        let mut utilization = 0;

        for line in f.lines().scan((), |_, x| x.ok()) {
            if line.starts_with("cpu ") {
                let data: Vec<u64> = (&line)
                    .split(' ')
                    .collect::<Vec<&str>>()
                    .iter()
                    .skip(2)
                    .filter_map(|x| x.parse::<u64>().ok())
                    .collect::<Vec<_>>();

                // idle = idle + iowait
                let idle = data[3] + data[4];
                let non_idle = data[0] + // user
                                data[1] + // nice
                                data[2] + // system
                                data[5] + // irq
                                data[6] + // softirq
                                data[7]; // steal

                let prev_total = self.prev_idle + self.prev_non_idle;
                let total = idle + non_idle;

                // This check is needed because the new values may be reset, for
                // example after hibernation.

                let (total_delta, idle_delta) = if prev_total < total && self.prev_idle <= idle {
                    (total - prev_total, idle - self.prev_idle)
                } else {
                    (1, 1)
                };

                utilization = (((total_delta - idle_delta) as f64 / total_delta as f64) * 100.) as u64;

                self.prev_idle = idle;
                self.prev_non_idle = non_idle;
            }
        }

        self.utilization.set_state(match utilization {
            0...30 => State::Idle,
            31...60 => State::Info,
            61...90 => State::Warning,
            _ => State::Critical,
        });

        self.utilization.set_text(format!("{:02}%", utilization));

        Ok(Some(self.update_interval))
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.utilization]
    }

    fn id(&self) -> &str {
        &self.id
    }
}
