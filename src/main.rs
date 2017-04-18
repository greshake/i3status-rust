#![warn(warnings)]

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate serde_json;
extern crate clap;
extern crate uuid;

#[macro_use]
pub mod util;
pub mod block;
pub mod blocks;
pub mod input;
pub mod icons;
pub mod themes;
pub mod scheduler;

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::fs::File;
use std::io::Read;

use block::Block;

use blocks::create_block;
use input::{process_events, I3barEvent};
use scheduler::{UpdateScheduler, UpdateRequest};
use themes::get_theme;
use icons::get_icons;

use self::clap::{Arg, App};
use self::serde_json::Value;

fn main() {
    let matches = App::new("i3status-rs")
        .version("0.1")
        .author("Kai Greshake <developement@kai-greshake.de>, Contributors on GitHub: \\
                 https://github.com/XYunknown/i3status-rust/graphs/contributors")
        .about("Replacement for i3status for Linux, written in Rust")
        .arg(Arg::with_name("config")
            .value_name("CONFIG_FILE")
            .help("sets a json config file")
            .required(true)
            .index(1))
        .arg(Arg::with_name("theme")
            .help("which theme to use")
            .default_value("solarized-dark")
            .short("t")
            .long("theme"))
        .arg(Arg::with_name("icons")
            .help("which icons to use")
            .default_value("none")
            .short("i")
            .long("icons"))
        .arg(Arg::with_name("debug")
            .short("d")
            .long("debug")
            .takes_value(false)
            .help("Prints debug information"))
        .arg(Arg::with_name("input-check-interval")
            .help("max. delay to react to clicking, in ms")
            .default_value("50"))
        .get_matches();

    // Load all arguments
    let input_check_interval = Duration::new(0, matches.value_of("input-check-interval")
                                                .unwrap()
                                                .parse::<u32>()
                                                .expect("Not a valid integer as interval") * 1000000);

    // Merge the selected icons and color theme
    let icons = get_icons(matches.value_of("icons").unwrap());
    let mut theme = get_theme(matches.value_of("theme").unwrap()).expect("Not a valid theme!");
    theme["icons"] = icons;

    // Load the config file
    let mut config_str = String::new();
    let mut config_file = File::open(matches.value_of("config").unwrap())
        .expect("Unable to open config file");
    config_file.read_to_string(&mut config_str).expect("Unable to read config file");

    // Create the blocks specified
    let config = serde_json::from_str(&config_str).expect("Config file is not valid JSON!");
    let mut blocks_owned: Vec<Box<Block>> = Vec::new();

    let (tx, rx_update_requests): (Sender<UpdateRequest>, Receiver<UpdateRequest>) = mpsc::channel();

    if let Value::Array(b) = config {
        for block in b {
            let name = block["block"].clone();
            blocks_owned.push(create_block(name.as_str().expect("block name must be a string"),
                                           block, tx.clone()))
        }
    } else {
        println!("The configs outer layer must be an array! For example: []")
    }

    let blocks = blocks_owned.iter().map(|x| x.as_ref()).collect();

    // Now we can start to run the i3bar protocol
    print!("{{\"version\": 1, \"click_events\": true}}[");

    let mut scheduler = UpdateScheduler::new(&blocks);

    // We wait for click events in a seperate thread, to avoid blocking to wait for stdin
    let (tx, rx_clicks): (Sender<I3barEvent>, Receiver<I3barEvent>) = mpsc::channel();
    process_events(tx);

    loop {
        // See if the user has clicked.
        while let Ok(event) = rx_clicks.try_recv() {
            if let Some(ref name) = event.name {
                for block in &blocks {
                    if let Some(ref id) = block.id() {
                        if id == name {
                            block.click(event.clone());
                            // redraw the blocks, state may have changed
                            util::print_blocks(&blocks, &theme);
                            break;
                        }
                    }
                }
            }
        }

        // Enqueue pending update requests
        while let Ok(request) = rx_update_requests.try_recv() {
            scheduler.schedule(request.id, request.update_time)
        }

        // This interval allows us to react to click events faster,
        // while still sleeping most of the time and not requiring all
        // Blocks to be Send.
        if scheduler.time_to_next_update() < input_check_interval {
            scheduler.do_scheduled_updates();

            // redraw the blocks, state changed
            util::print_blocks(&blocks, &theme);
        } else {
            thread::sleep(input_check_interval)
        }
    }
}
