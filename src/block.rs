use std::time::Duration;
use input::I3barEvent;
use widget::I3BarWidget;
pub trait Block {
    fn update(&mut self) -> Option<Duration> {
        None
    }
    fn view(&self) -> Vec<&I3BarWidget>;

    #[allow(unused_variables)]
    fn click(&mut self, event: &I3barEvent) {}
    fn id(&self) -> &str;
}