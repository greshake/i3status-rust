#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
extern crate serde;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate chan;
extern crate toml;
extern crate clap;
extern crate uuid;
extern crate regex;
extern crate num;
extern crate inotify;
extern crate maildir;
extern crate chrono;
extern crate chrono_tz;
#[cfg(feature = "pulseaudio")]
extern crate libpulse_binding as pulse;

#[macro_use]
mod de;
#[macro_use]
mod util;
mod block;
pub mod blocks;
mod config;
mod errors;
mod input;
mod icons;
mod themes;
mod scheduler;
mod subprocess;
mod widget;
mod widgets;

#[cfg(feature = "profiling")]
extern crate cpuprofiler;
#[cfg(feature = "profiling")]
use cpuprofiler::PROFILER;
#[cfg(feature = "profiling")]
extern crate progress;

use std::collections::HashMap;
use std::time::Duration;
use std::ops::DerefMut;

use block::Block;

use blocks::create_block;
use config::Config;
use errors::*;
use input::{process_events, I3BarEvent};
use scheduler::{Task, UpdateScheduler};
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;

use util::deserialize_file;

use self::clap::{App, Arg, ArgMatches};
use self::chan::{Receiver, Sender};

fn main() {
    let mut builder = App::new("i3status-rs")
        .version("0.9")
        .author(
            "Kai Greshake <development@kai-greshake.de>, Contributors on GitHub: \\
             https://github.com/greshake/i3status-rust/graphs/contributors",
        )
        .about("Replacement for i3status for Linux, written in Rust")
        .arg(
            Arg::with_name("config")
                .value_name("CONFIG_FILE")
                .help("sets a toml config file")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("exit-on-error")
                .help(
                    "exit on error rather than printing the error to i3bar and keep running",
                )
                .long("exit-on-error")
                .takes_value(false),
        );

    if_debug!({
        builder = builder
            .arg(
                Arg::with_name("profile")
                    .long("profile")
                    .takes_value(true)
                    .help("A block to be profiled. Analyze block.profile with pprof"),
            )
            .arg(
                Arg::with_name("profile-runs")
                    .long("profile-runs")
                    .takes_value(true)
                    .default_value("10000")
                    .help("How many times to execute update when profiling."),
            );;
    });

    let matches = builder.get_matches();
    let exit_on_error = matches.is_present("exit-on-error");

    // Run and match for potential error
    if let Err(error) = run(&matches) {
        if exit_on_error {
            eprintln!("{:?}", error);
            ::std::process::exit(1);
        }

        let error_widget = TextWidget::new(Default::default())
            .with_state(State::Critical)
            .with_text(&format!("{:?}", error));
        let error_rendered = error_widget.get_rendered();
        println!(
            "{}",
            serde_json::to_string(&[error_rendered]).expect("failed to serialize error message")
        );

        eprintln!("\n\n{:?}", error);
        // Do nothing, so the error message keeps displayed
        loop {
            ::std::thread::sleep(Duration::from_secs(::std::u64::MAX));
        }
    }
}

#[allow(unused_mut)] // TODO: Remove when fixed in chan_select
fn run(matches: &ArgMatches) -> Result<()> {
    // Now we can start to run the i3bar protocol
    print!("{{\"version\": 1, \"click_events\": true}}\n[");

    // Read & parse the config file
    let config: Config = deserialize_file(matches.value_of("config").unwrap())?;

    // Update request channel
    let (tx_update_requests, rx_update_requests): (Sender<Task>, Receiver<Task>) = chan::async();

    // In dev build, we might diverge into profiling blocks here
    if let Some(name) = matches.value_of("profile") {
        profile_config(name, matches.value_of("profile-runs").unwrap(), &config, &tx_update_requests)?;
        return Ok(());
    }

    let mut config_alternating_tint = config.clone();
    {
        let tint_bg = &config.theme.alternating_tint_bg;
        config_alternating_tint.theme.idle_bg = util::add_colors(&config_alternating_tint.theme.idle_bg, tint_bg)
            .configuration_error("can't parse alternative_tint color code")?;
        config_alternating_tint.theme.info_bg = util::add_colors(&config_alternating_tint.theme.info_bg, tint_bg)
            .configuration_error("can't parse alternative_tint color code")?;
        config_alternating_tint.theme.good_bg = util::add_colors(&config_alternating_tint.theme.good_bg, tint_bg)
            .configuration_error("can't parse alternative_tint color code")?;
        config_alternating_tint.theme.warning_bg = util::add_colors(&config_alternating_tint.theme.warning_bg, tint_bg)
            .configuration_error("can't parse alternative_tint color code")?;
        config_alternating_tint.theme.critical_bg = util::add_colors(&config_alternating_tint.theme.critical_bg, tint_bg)
            .configuration_error("can't parse alternative_tint color code")?;

        let tint_fg = &config.theme.alternating_tint_fg;
        config_alternating_tint.theme.idle_fg = util::add_colors(&config_alternating_tint.theme.idle_fg, tint_fg)
            .configuration_error("can't parse alternative_tint color code")?;
        config_alternating_tint.theme.info_fg = util::add_colors(&config_alternating_tint.theme.info_fg, tint_fg)
            .configuration_error("can't parse alternative_tint color code")?;
        config_alternating_tint.theme.good_fg = util::add_colors(&config_alternating_tint.theme.good_fg, tint_fg)
            .configuration_error("can't parse alternative_tint color code")?;
        config_alternating_tint.theme.warning_fg = util::add_colors(&config_alternating_tint.theme.warning_fg, tint_fg)
            .configuration_error("can't parse alternative_tint color code")?;
        config_alternating_tint.theme.critical_fg = util::add_colors(&config_alternating_tint.theme.critical_fg, tint_fg)
            .configuration_error("can't parse alternative_tint color code")?;
    }

    let mut blocks: Vec<Box<Block>> = Vec::new();

    let mut alternator = false;
    // Initialize the blocks
    for &(ref block_name, ref block_config) in &config.blocks {
        blocks.push(create_block(
            block_name,
            block_config.clone(),
            if alternator {
                config_alternating_tint.clone()
            } else {
                config.clone()
            },
            tx_update_requests.clone(),
        )?);
        alternator = !alternator;
    }

    // We save the order of the blocks here,
    // because they will be passed to an unordered HashMap
    let order = blocks.iter().map(|x| String::from(x.id())).collect::<Vec<_>>();

    let mut scheduler = UpdateScheduler::new(&blocks);

    let mut block_map: HashMap<String, &mut Block> = HashMap::new();

    for block in &mut blocks {
        block_map.insert(String::from(block.id()), (*block).deref_mut());
    }

    // We wait for click events in a separate thread, to avoid blocking to wait for stdin
    let (tx_clicks, rx_clicks): (Sender<I3BarEvent>, Receiver<I3BarEvent>) = chan::async();
    process_events(tx_clicks);

    // Time to next update channel.
    // Fires immediately for first updates
    let mut ttnu = chan::after_ms(0);

    loop {
        // We use the message passing concept of channel selection
        // to avoid busy wait

        chan_select! {
            // Receive click events
            rx_clicks.recv() -> res => if let Some(event) = res {
                    for block in block_map.values_mut() {
                        block.click(&event)?;
                    }
                    util::print_blocks(&order, &block_map, &config)?;
            },
            // Receive async update requests
            rx_update_requests.recv() -> res => if let Some(request) = res {
                // Process immediately and forget
                block_map
                    .get_mut(&request.id)
                    .internal_error("scheduler", "could not get required block")?
                    .update()?;
                util::print_blocks(&order, &block_map, &config)?;
            },
            // Receive update timer events
            ttnu.recv() => {
                scheduler.do_scheduled_updates(&mut block_map)?;

                // redraw the blocks, state changed
                util::print_blocks(&order, &block_map, &config)?;
            }
        }

        // Set the time-to-next-update timer
        match scheduler.time_to_next_update() {
            Some(time) => ttnu = chan::after(time),
            None => ttnu = chan::after(Duration::from_secs(std::u64::MAX)),
        }
    }
}

#[cfg(feature = "profiling")]
fn profile(iterations: i32, name: &str, block: &mut Block) {
    let mut bar = progress::Bar::new();
    println!(
        "Now profiling the {0} block by executing {1} updates.\n \
         Use pprof to analyze {0}.profile later.",
        name,
        iterations
    );

    PROFILER
        .lock()
        .unwrap()
        .start(format!("./{}.profile", name))
        .unwrap();

    bar.set_job_title("Profiling...");

    for i in 0..iterations {
        block.update().expect("block update failed");
        bar.reach_percent(((i as f64 / iterations as f64) * 100.).round() as i32);
    }

    PROFILER.lock().unwrap().stop().unwrap();
}

#[cfg(feature = "profiling")]
fn profile_config(name: &str, runs: &str, config: &Config, update: Sender<Task>) -> Result<()> {
    let profile_runs = runs.parse::<i32>()
        .configuration_error("failed to parse --profile-runs as an integer")?;
    for &(ref block_name, ref block_config) in &config.blocks {
        if block_name == name {
            let mut block = create_block(
                &block_name,
                block_config.clone(),
                config.clone(),
                update.clone(),
            )?;
            profile(profile_runs, &block_name, block.deref_mut());
            break;
        }
    }
    Ok(())
}

#[cfg(not(feature = "profiling"))]
fn profile_config(_name: &str, _runs: &str, _config: &Config, _update: &Sender<Task>) -> Result<()> {
    // TODO: Maybe we should just panic! here.
    Err(InternalError(
        "profile".to_string(),
        "The 'profiling' feature was not enabled at compile time.".to_string(),
        None,
    ))
}
