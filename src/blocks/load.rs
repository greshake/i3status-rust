use std::time::Duration;

use block::Block;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};
use input::I3barEvent;

use std::fs::OpenOptions;
use std::fs::File;
use std::io::Result;
use std::io::Read;

use serde_json::Value;
use uuid::Uuid;

pub struct Load {
    text: TextWidget,
    id: String,
    update_interval: Duration,
}

impl Load {
    pub fn new(config: Value, theme: Value) -> Load {
        let text = TextWidget::new(theme.clone()).with_icon("cogs").with_state(State::Info);
        Load {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(config, "interval", 3), 0),
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

        let values = map!("1min" => split[0],
                          "5min" => split[1],
                          "15min" => split[2]);

        self.text.set_state(
            match values["1min"].parse::<f32>().unwrap() {
                0. ... 1. => State::Idle,
                1. ... 2. => State::Info,
                2. ...3. => State::Warning,
                _ => State::Critical
        });

        self.text.set_text(String::from(values["1min"]));

        Some(self.update_interval.clone())
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }
    fn click(&mut self, _: &I3barEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}
