use std::time::Duration;
use input::I3barEvent;
use widget::I3BarWidget;

pub trait Block {
    fn update(&mut self) -> Option<Duration> {
        None
    }
    fn view(&self) -> Vec<&I3BarWidget>;
    fn click(&mut self, &I3barEvent) {}
    fn id(&self) -> &str;
}