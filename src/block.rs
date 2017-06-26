use config::Config;
use errors::*;
use scheduler::Task;
use std::sync::mpsc::Sender;
use std::time::Duration;
use input::I3BarEvent;
use widget::I3BarWidget;

pub trait Block {
    /// Updates the internal state of a Block
    fn update(&mut self) -> Result<Option<Duration>> {
        Ok(None)
    }
    /// Returns the view of the block, comprised of widgets
    fn view(&self) -> Vec<&I3BarWidget>;

    #[allow(unused_variables)]
    /// This function is called on every block for every click.
    /// Filter events by using the event.name property (matches the ButtonWidget name)
    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    /// This function returns a unique id.
    fn id(&self) -> &str;

    /// Is the block allowed to raise errors without
    /// crashing the whole bar?
    fn optional(&self) -> bool {
        false
    }
}

pub trait ConfigBlock: Block {
    type Config;

    fn new(block_config: Self::Config, config: Config, tx_update_request: Sender<Task>) -> Result<Self>
    where
        Self: Sized;
}
