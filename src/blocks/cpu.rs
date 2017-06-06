use std::time::Duration;

use block::Block;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};
use input::I3barEvent;

use std::io::BufReader;
use std::io::prelude::*;
use std::fs::{File};

use serde_json::Value;
use uuid::Uuid;


pub struct Cpu {
    utilization: TextWidget,
    prev_idle: u64,
    prev_non_idle: u64,
    id: String,
    update_interval: Duration,
}

impl Cpu {
    pub fn new(config: Value, theme: Value) -> Cpu {
        {
            let text = TextWidget::new(theme.clone()).with_icon("cpu");
            return Cpu {
                id: Uuid::new_v4().simple().to_string(),
                update_interval: Duration::new(get_u64_default!(config, "interval", 1), 0),
                utilization: text,
                prev_idle: 0,
                prev_non_idle: 0,
            }
        }
        
    }
}


impl Block for Cpu
{
    fn update(&mut self) -> Option<Duration> {
        let f = File::open("/proc/stat").expect("Your system doesn't support /proc/stat");
        let f = BufReader::new(f);

        let mut utilization = 0;

        for line in f.lines().scan((), |_, x| x.ok()) {
            if line.starts_with("cpu ") {
                let data: Vec<u64> = (&line)
                                        .split(" ")
                                        .collect::<Vec<&str>>()
                                        .iter().skip(2)
                                        .filter_map(|x| x.parse::<u64>().ok())
                                        .collect::<Vec<_>>();

                // idle = idle + iowait
                let idle =  data[3] + data[4];
                let non_idle =  data[0] + // user
                                data[1] + // nice
                                data[2] + // system
                                data[5] + // irq
                                data[6] + // softirq
                                data[7];  // steal

                let prev_total = self.prev_idle + self.prev_non_idle;
                let total = idle + non_idle;

                let mut total_delta = 1;
                let mut idle_delta = 1;

                // This check is needed because the new values may be reset, for
                // example after hibernation.
                if prev_total < total && self.prev_idle <= idle {
                    total_delta = total - prev_total;
                    idle_delta = idle - self.prev_idle;
                }


                utilization = (((total_delta - idle_delta) as f64 / total_delta as f64) * 100.) as u64;

                self.prev_idle = idle;
                self.prev_non_idle = non_idle;
            }
        }

        self.utilization.set_state(match utilization {
            0 ... 30 => State::Idle,
            30 ... 60 => State::Info,
            60 ... 90 => State::Warning,
            _ => State::Critical
        });

        self.utilization.set_text(format!("{:02}%", utilization));

        Some(self.update_interval.clone())
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.utilization]
    }
    fn click_left(&mut self, _: &I3barEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}
