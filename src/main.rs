// This is needed because apparently the large json! macro in the icons.rs file explodes at compile time...
#![recursion_limit="128"]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
extern crate serde;

#[macro_use]
extern crate serde_json;
extern crate toml;
extern crate clap;
extern crate uuid;
extern crate regex;

#[macro_use]
mod de;
#[macro_use]
pub mod util;
pub mod block;
pub mod blocks;
pub mod config;
mod errors;
pub mod input;
pub mod icons;
pub mod themes;
pub mod scheduler;
pub mod widget;
pub mod widgets;

#[cfg(debug_assertions)]
extern crate cpuprofiler;
#[cfg(debug_assertions)]
use cpuprofiler::PROFILER;
#[cfg(debug_assertions)]
extern crate progress;

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::thread;
use std::collections::HashMap;
use std::time::Duration;
use std::ops::DerefMut;

use block::Block;

use blocks::create_block;
use config::Config;
use errors::*;
use input::{process_events, I3BarEvent};
use scheduler::{UpdateScheduler, Task};
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;

use util::deserialize_file;

use self::clap::{Arg, ArgMatches, App};

fn run(matches: ArgMatches) -> Result<()> {
    let config: Config = deserialize_file(matches.value_of("config").unwrap())?;

    // Load all arguments
    let input_check_interval = Duration::new(0, matches.value_of("input-check-interval")
                                                .unwrap()
                                                .parse::<u32>()
                                                .internal_error("main", "Not a valid integer as interval")? * 1000000);

    let (tx, rx_update_requests): (Sender<Task>, Receiver<Task>) = mpsc::channel();

    #[cfg(debug_assertions)]
    if_debug!({
        if matches.value_of("profile").is_some() {
            for &(ref block_name, ref block_config) in &config.blocks {
                if block_name == matches.value_of("profile").unwrap() {
                    let mut block = create_block(&block_name, block_config.clone(), config.clone(), tx.clone())?;
                    profile(matches.value_of("profile-runs").unwrap().parse::<i32>().unwrap(), &block_name, block.deref_mut());
                    return Ok(());
                }
            }
        }
    });

    let mut blocks: Vec<Box<Block>> = Vec::new();

    for &(ref block_name, ref block_config) in &config.blocks {
        blocks.push(create_block(&block_name, block_config.clone(), config.clone(), tx.clone())?)
    }

    let order = blocks.iter().map(|x| String::from(x.id())).collect();

    let mut scheduler = UpdateScheduler::new(&blocks);

    let mut block_map: HashMap<String, &mut Block> = HashMap::new();

    for block in blocks.iter_mut() {
        block_map.insert(String::from(block.id()), (*block).deref_mut());
    }

    // Now we can start to run the i3bar protocol
    print!("{{\"version\": 1, \"click_events\": true}}\n[");

    // We wait for click events in a seperate thread, to avoid blocking to wait for stdin
    let (tx, rx_clicks): (Sender<I3BarEvent>, Receiver<I3BarEvent>) = mpsc::channel();
    process_events(tx);

    loop {
        // See if the user has clicked.
        while let Ok(event) = rx_clicks.try_recv() {
            for (_, block) in &mut block_map {
                block.click(&event)?;
            }
            util::print_blocks(&order, &block_map, &config)?;
        }

        // Enqueue pending update requests
        while let Ok(request) = rx_update_requests.try_recv() {
            scheduler.schedule(request)
        }

        // This interval allows us to react to click events faster,
        // while still sleeping most of the time and not requiring all
        // Blocks to be Send.
        if let Some(ttnu) = scheduler.time_to_next_update() {
            if ttnu < input_check_interval {
                scheduler.do_scheduled_updates(&mut block_map)?;

                // redraw the blocks, state changed
                util::print_blocks(&order, &block_map, &config)?;
            } else {
                thread::sleep(input_check_interval)
            }
        }
    }
}

fn main() {
    let mut builder = App::new("i3status-rs")
        .version("0.1")
        .author("Kai Greshake <development@kai-greshake.de>, Contributors on GitHub: \\
                 https://github.com/greshake/i3status-rust/graphs/contributors")
        .about("Replacement for i3status for Linux, written in Rust")
        .arg(Arg::with_name("config")
            .value_name("CONFIG_FILE")
            .help("sets a json config file")
            .required(true)
            .index(1))
        .arg(Arg::with_name("theme")
            .help("which theme to use, can be a builtin theme or file.\nBuiltin themes: solarized-dark, plain")
            .default_value("plain")
            .short("t")
            .long("theme"))
        .arg(Arg::with_name("icons")
            .help("which icons to use, can be a builtin set or file.\nBuiltin sets: awesome, none (textual)")
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
        .arg(Arg::with_name("exit-on-error")
             .help("exit on error rather than printing the error to i3bar and keep running")
             .long("exit-on-error")
             .takes_value(false));

    if_debug!({
        builder = builder
        .arg(Arg::with_name("profile")
            .long("profile")
            .takes_value(true)
            .help("A block to be profiled. Analyze block.profile with pprof"))
        .arg(Arg::with_name("profile-runs")
            .long("profile-runs")
            .takes_value(true)
            .default_value("10000")
            .help("How many times to execute update when profiling."));;
    });

    let matches = builder.get_matches();
    let exit_on_error = matches.is_present("exit-on-error");

    // Run and match for potential error
    if let Err(error) = run(matches) {
        if exit_on_error {
            eprintln!("{:?}", error);
            ::std::process::exit(1);
        }

        let error_widget = TextWidget::new(Default::default())
            .with_state(State::Critical)
            .with_text(&format!("{}", error));
        let error_rendered = error_widget.get_rendered();
        println!("{}", serde_json::to_string(&[error_rendered]).expect("failed to serialize error message"));

        // Do nothing, so the error message keeps displayed
        loop {
            ::std::thread::sleep(Duration::from_secs(10));
        }
    }
}

#[cfg(debug_assertions)]
fn profile(iterations: i32, name: &str, block: &mut Block) {
    let mut bar = progress::Bar::new();
    println!("Now profiling the {0} block by executing {1} updates.\n \
              Use pprof to analyze {0}.profile later.", name, iterations);

    PROFILER.lock().unwrap().start(format!("./{}.profile", name)).unwrap();

    bar.set_job_title("Profiling...");

    for i in 0..iterations {
        block.update().expect("block update failed");
        bar.reach_percent(((i as f64 / iterations as f64) * 100.).round() as i32);
    }

    PROFILER.lock().unwrap().stop().unwrap();
}
