use std::time::Duration;

use block::Block;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};
use input::I3barEvent;
use util::FormatTemplate;

use std::io::BufReader;
use std::io::prelude::*;
use std::fs::{File, OpenOptions};

use serde_json::Value;
use uuid::Uuid;

pub struct Load {
    text: TextWidget,
    logical_cores: u32,
    format: FormatTemplate,
    id: String,
    update_interval: Duration,
}

impl Load {
    pub fn new(config: Value, theme: Value) -> Load {
        let text = TextWidget::new(theme.clone()).with_icon("cogs").with_state(State::Info);

        let f = File::open("/proc/cpuinfo").expect("Your system doesn't support /proc/cpuinfo");
        let f = BufReader::new(f);

        let mut logical_cores = 0;

        for line in f.lines().scan((), |_, x| x.ok()) {
            // TODO: Does this value always represent the correct number of logical cores?
            if line.starts_with("siblings") {
                let split: Vec<&str> = (&line).split(" ").collect();
                logical_cores = split[1].parse::<u32>().expect("Invalid Cpu info format!");
                break;
            }
        }

        Load {
            id: Uuid::new_v4().simple().to_string(),
            logical_cores: logical_cores,
            update_interval: Duration::new(get_u64_default!(config, "interval", 3), 0),
            format: FormatTemplate::from_string(get_str_default!(config, "format", "{1m}")).expect("Invalid format specified for load"),
            text: text
        }
    }
}

impl Block for Load
{
    fn update(&mut self) -> Option<Duration> {
        let mut f = OpenOptions::new()
            .read(true)
            .open("/proc/loadavg")
            .expect("Your system does not support reading the load average from /proc/loadavg");
        let mut loadavg = String::new();
        f.read_to_string(&mut loadavg).expect("Failed to read the load average of your system!");

        let split: Vec<&str> = (&loadavg).split(" ").collect();

        let values = map!("{1m}" => split[0],
                          "{5m}" => split[1],
                          "{15m}" => split[2]);

        let used_perc = values["{1m}"].parse::<f32>().unwrap() / self.logical_cores as f32;
        self.text.set_state(
            match  used_perc {
                0. ... 0.3 => State::Idle,
                0.3 ... 0.6 => State::Info,
                0.6 ... 0.9 => State::Warning,
                _ => State::Critical
        });

        self.text.set_text(self.format.render_static_str(&values));

        Some(self.update_interval.clone())
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }
    fn click_left(&mut self, _: &I3barEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}
