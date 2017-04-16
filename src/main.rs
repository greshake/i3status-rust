#![warn(warnings)]

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate serde_json;

pub mod block;
pub mod blocks;
pub mod input;
pub mod scheduler;

#[macro_use]
pub mod util;

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use block::{Block, MouseButton};

use blocks::disk_info::{DiskInfo, DiskInfoType, Unit};
use blocks::time::Time;
use blocks::template::Template;
use blocks::toggle::Toggle;

use input::{process_events, I3barEvent};
use scheduler::UpdateScheduler;

use self::serde_json::Value;

fn main() {
    let input_check_interval = Duration::new(0, 50000000); // 500ms

    let root_usage = DiskInfo::new("/", "/", DiskInfoType::Free, Unit::GB);
    let time = Time::new("t1");
    let toggle = Toggle::new("test_toggle");
    let template = Template::new("template1");

    let blocks = vec![&toggle as &Block,
                      &template as &Block,
                      &time as &Block,
                      &root_usage as &Block];

    let theme = json!({
        "idle_bg": "#002b36",
        "idle_fg": "#93a1a1",
        "info_bg": "#268bd2",
        "info_fg": "#002b36",
        "good_bg": "#859900",
        "good_fg": "#002b36",
        "warning_bg": "#b58900",
        "warning_fg": "#002b36",
        "critical_bg": "#dc322f",
        "critical_fg": "#002b36"
    });

    print!("{{\"version\": 1, \"click_events\": true}}[");

    let mut scheduler = UpdateScheduler::new(&blocks);

    let (tx, rx): (Sender<I3barEvent>, Receiver<I3barEvent>) = mpsc::channel();
    process_events(tx);

    loop {
        // See if the user has clicked.
        if let Ok(event) = rx.try_recv() {
            if let Some(ref name) = event.name {
                for block in &blocks {
                    if let Some(ref id) = block.id() {
                        if id == name {
                            match event.button {
                                1 => block.click(MouseButton::Left),
                                2 => block.click(MouseButton::Middle),
                                3 => block.click(MouseButton::Right),
                                _ => {}
                            }
                            // redraw the blocks
                            util::print_blocks(&blocks, &theme);
                        }
                    }
                }
            }
        }

        if scheduler.time_to_next_update() < input_check_interval {
            scheduler.do_scheduled_updates();

            // redraw the blocks
            util::print_blocks(&blocks, &theme);
        } else {
            thread::sleep(input_check_interval)
        }
    }
}
