use std::time::Duration;
use input::I3barEvent;
use widget::UIElement;

pub trait Block {
    fn update(&self) -> Option<Duration> {
        None
    }
    fn get_ui(&self) -> Box<UIElement>;
    fn click(&self, &I3barEvent) {}
    fn id(&self) -> Option<&str> { None }
}