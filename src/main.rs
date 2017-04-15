pub mod block;
#[macro_use]
pub mod util;
pub mod blocks;
pub mod scheduler;

use blocks::time::Time;
use blocks::separator::Separator;
use block::{Block, Theme, Color};
use std::boxed::Box;
use scheduler::UpdateScheduler;
use std::collections::HashMap;



fn main() {
    let time = Time::new("Time Module 1");
    let separator = Separator {};

    let blocks = vec![&separator as &Block,
                      &time as &Block,
                      &separator as &Block];

    let theme = Theme {
        bg: Color(255, 255, 255),
        fg: Color::from_string("#FFFFFF"),
        info: Color::from_string("#FFFFFF"),
        warn: Color::from_string("#FFFFFF"),
        crit: Color::from_string("#FFFFFF"),
    };

    let template = map!{
        "background" => theme.bg.to_string()
    };

    print!("{{\"version\": 1, \"click_events\": true}}[");

    let mut scheduler = UpdateScheduler::new(&blocks);

    loop {
        // Process click events, call the right blocks.
        // TODO: implement

        // wait for the scheduler to execute updates
        scheduler.next();

        // redraw the blocks
        util::print_blocks(&blocks, &template, &theme);
    }
}