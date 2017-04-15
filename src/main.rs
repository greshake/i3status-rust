#[macro_use]
extern crate serde_derive;
extern crate serde_json;

pub mod block;
#[macro_use]
pub mod util;
pub mod blocks;
pub mod scheduler;
pub mod input;

use blocks::time::Time;
use blocks::separator::Separator;
use blocks::template::Template;
use blocks::toggle::Toggle;
use block::{Block, Theme, Color, MouseButton};
use std::boxed::Box;
use input::{process_events, I3barEvent};
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use scheduler::UpdateScheduler;
use std::collections::HashMap;
use self::serde_json::Value;
use std::thread;
use std::time::Duration;

fn main() {
    let time = Time::new("Time Module 1");
    let separator = Separator {};
    //let home_usage = DiskUsage::new("home", "/home");
    let template = Template::new("test");
    let input_check_interval = Duration::new(0, 50000000); // 50ms
    let time = Time::new("t1");
    let sep = Separator {};
    let toggle = Toggle::new("test_toggle");

    let blocks = vec![&sep as &Block,
                      &toggle as &Block,
                      &time as &Block,
                      &sep as &Block,
                      &template as &Block,
                      &separator as &Block];

    let theme = Theme {
        bg: Color(0, 0, 0),
        fg: Color::from_string("#FFFFFF"),
        info: Color::from_string("#FFFFFF"),
        warn: Color::from_string("#FFFFFF"),
        crit: Color::from_string("#FFFFFF"),
        seperator: Color::from_string("#666666"),
    };

    let template = map! {
        "background" => theme.bg.to_string()
    };

    print!("{{\"version\": 1, \"click_events\": true}}[");

    let mut scheduler = UpdateScheduler::new(&blocks);

    let (tx, rx): (Sender<I3barEvent>, Receiver<I3barEvent>) = mpsc::channel();
    process_events(tx);

    loop {
        // See if the user has clicked.
        if let Ok(event) = rx.try_recv() {
            for block in &blocks {
                if let Some(ref id) = block.id() {
                    if let Some(ref name) = event.name {
                        if id == name {
                            match event.button {
                                1 => block.click(MouseButton::Left),
                                2 => block.click(MouseButton::Middle),
                                3 => block.click(MouseButton::Right),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        if scheduler.time_to_next_update() < input_check_interval {
            scheduler.do_scheduled_updates();

            // redraw the blocks
            util::print_blocks(&blocks, &template, &theme);
        } else {
            thread::sleep(input_check_interval)
        }
    }
}
