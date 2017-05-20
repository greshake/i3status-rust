#![warn(missing_docs)]

use std::time::Duration;
use input::I3barEvent;
use widget::I3BarWidget;

pub trait Block {
    /// Updates the internal state of a Block
    fn update(&mut self) -> Option<Duration> {
        None
    }
    /// Returns the view of the block, comprised of widgets
    fn view(&self) -> Vec<&I3BarWidget>;

    #[allow(unused_variables)]
    /// This function is called on every block for every click.
    /// Filter events by using the event.name property (matches the ButtonWidget name)
    fn click(&mut self, event: &I3barEvent) {}

    /// This function returns a unique id.
    fn id(&self) -> &str;
}