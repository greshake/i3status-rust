use std::time::Duration;
use std::sync::mpsc::Sender;
use std::process::Command;

use block::Block;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3barEvent;
use scheduler::Task;

use serde_json::Value;
use uuid::Uuid;


pub struct Pacman {
    text: TextWidget,
    id: String,
    update_interval: Duration,

    //useful, but optional
    #[allow(dead_code)]
    theme: Value,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

impl Pacman {
    pub fn new(config: Value, tx: Sender<Task>, theme: Value) -> Pacman {
        {
            Pacman {
                id: Uuid::new_v4().simple().to_string(),
                update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
                text: TextWidget::new(theme.clone()).with_text(""),
                tx_update_request: tx,
                theme: theme,
            }
        }
        
    }
}

impl Block for Pacman
{
    fn update(&mut self) -> Option<Duration> {
        let output = String::from_utf8(Command::new("sh").arg("-c").arg("pacman -Sup 2>/dev/null").output().expect("You need to have pacman set up").stdout).expect("crazy stuff");
        let count = output.lines().count() - 1;
        self.text.set_text(format!("{} updates", count));
        self.text.set_icon("update");
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
