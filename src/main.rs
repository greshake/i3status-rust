#[macro_use]
extern crate serde_json;
#[cfg(feature = "pulseaudio")]
use libpulse_binding as pulse;

#[macro_use]
mod de;
#[macro_use]
mod util;
pub mod blocks;
mod config;
mod errors;
mod http;
mod icons;
mod input;
mod scheduler;
mod signals;
mod subprocess;
mod themes;
mod widget;
mod widgets;

#[cfg(feature = "profiling")]
use cpuprofiler::PROFILER;
#[cfg(feature = "profiling")]
use std::ops::DerefMut;

use std::time::Duration;

use clap::{crate_authors, crate_description, App, Arg, ArgMatches};
use crossbeam_channel::{select, Receiver, Sender};

use crate::blocks::create_block;
use crate::blocks::Block;
use crate::config::Config;
use crate::errors::*;
use crate::input::{process_events, I3BarEvent};
use crate::scheduler::{Task, UpdateScheduler};
use crate::signals::process_signals;
use crate::util::deserialize_file;
use crate::widget::{I3BarWidget, State};
use crate::widgets::text::TextWidget;

fn main() {
    let ver = if env!("GIT_COMMIT_HASH").is_empty() || env!("GIT_COMMIT_DATE").is_empty() {
        env!("CARGO_PKG_VERSION").to_string()
    } else {
        format!(
            "{} (commit {} {})",
            env!("CARGO_PKG_VERSION"),
            env!("GIT_COMMIT_HASH"),
            env!("GIT_COMMIT_DATE")
        )
    };
    let mut builder = App::new("i3status-rs")
        .version(&*ver)
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("config")
                .value_name("CONFIG_FILE")
                .help("Sets a toml config file")
                .required(false)
                .index(1),
        )
        .arg(
            Arg::with_name("exit-on-error")
                .help("Exit rather than printing errors to i3bar and continuing")
                .long("exit-on-error")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("never-pause")
                .help("Ignore any attempts by i3 to pause the bar when hidden/fullscreen")
                .long("never-pause")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("one-shot")
                .help("Print blocks once and exit")
                .long("one-shot")
                .takes_value(false)
                .hidden(true),
        );

    if_debug!({
        builder = builder
            .arg(
                Arg::with_name("profile")
                    .long("profile")
                    .takes_value(true)
                    .help("A block to be profiled. Creates a `block.profile` file that can be analyzed with `pprof`"),
            )
            .arg(
                Arg::with_name("profile-runs")
                    .long("profile-runs")
                    .takes_value(true)
                    .default_value("10000")
                    .help("Number of times to execute update when profiling"),
            );
    });

    let matches = builder.get_matches();
    let exit_on_error = matches.is_present("exit-on-error");

    // Run and match for potential error
    if let Err(error) = run(&matches) {
        if exit_on_error {
            eprintln!("{:?}", error);
            ::std::process::exit(1);
        }

        let error_widget = TextWidget::new(Default::default(), 9999999999)
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

fn run(matches: &ArgMatches) -> Result<()> {
    // Now we can start to run the i3bar protocol
    let initialise = if matches.is_present("never-pause") {
        "\"version\": 1, \"click_events\": true, \"stop_signal\": 0"
    } else {
        "\"version\": 1, \"click_events\": true"
    };
    print!("{{{}}}\n[", initialise);

    // Read & parse the config file
    let config_path = match matches.value_of("config") {
        Some(config_path) => std::path::PathBuf::from(config_path),
        None => util::xdg_config_home().join("i3status-rust/config.toml"),
    };
    let config = deserialize_file(&config_path)?;

    // Update request channel
    let (tx_update_requests, rx_update_requests): (Sender<Task>, Receiver<Task>) =
        crossbeam_channel::unbounded();

    // In dev build, we might diverge into profiling blocks here
    if let Some(name) = matches.value_of("profile") {
        return profile_config(
            name,
            matches.value_of("profile-runs").unwrap(),
            &config,
            tx_update_requests,
        );
    }

    // Initialize the blocks
    let mut blocks: Vec<Box<dyn Block>> = Vec::new();
    for &(ref block_name, ref block_config) in &config.blocks {
        blocks.push(create_block(
            blocks.len(),
            block_name,
            block_config.clone(),
            config.clone(),
            tx_update_requests.clone(),
        )?);
    }

    let mut scheduler = UpdateScheduler::new(&blocks);

    // We wait for click events in a separate thread, to avoid blocking to wait for stdin
    let (tx_clicks, rx_clicks): (Sender<I3BarEvent>, Receiver<I3BarEvent>) =
        crossbeam_channel::unbounded();
    process_events(tx_clicks);

    // We wait for signals in a separate thread
    let (tx_signals, rx_signals): (Sender<i32>, Receiver<i32>) = crossbeam_channel::unbounded();
    process_signals(tx_signals);

    // Time to next update channel.
    // Fires immediately for first updates
    let mut ttnu = crossbeam_channel::after(Duration::from_millis(0));

    let one_shot = matches.is_present("one-shot");
    loop {
        // We use the message passing concept of channel selection
        // to avoid busy wait
        select! {
            // Receive click events
            recv(rx_clicks) -> res => if let Ok(event) = res {
                    for block in blocks.iter_mut() {
                        block.click(&event)?;
                    }
                    util::print_blocks(&blocks, &config)?;
            },
            // Receive async update requests
            recv(rx_update_requests) -> request => if let Ok(req) = request {
                // Process immediately and forget
                blocks.get_mut(req.id)
                    .internal_error("scheduler", "could not get required block")?
                    .update()?;
                util::print_blocks(&blocks, &config)?;
            },
            // Receive update timer events
            recv(ttnu) -> _ => {
                scheduler.do_scheduled_updates(&mut blocks)?;
                // redraw the blocks, state changed
                util::print_blocks(&blocks, &config)?;
            },
            // Receive signal events
            recv(rx_signals) -> res => if let Ok(sig) = res {
                match sig {
                    signal_hook::consts::SIGUSR1 => {
                        //USR1 signal that updates every block in the bar
                        for block in blocks.iter_mut() {
                            block.update()?;
                        }
                        util::print_blocks(&blocks, &config)?;
                    },
                    signal_hook::consts::SIGUSR2 => {
                        //USR2 signal that should reload the config
                        //TODO not implemented
                        //unimplemented!("SIGUSR2 is meant to be used to reload the config toml, but this feature is yet not implemented");
                    },
                    _ => {
                        //Real time signal that updates only the blocks listening
                        //for that signal
                        for block in blocks.iter_mut() {
                            block.signal(sig)?;
                        }
                    },
                };
            }
        }

        // Set the time-to-next-update timer
        if let Some(time) = scheduler.time_to_next_update() {
            ttnu = crossbeam_channel::after(time)
        }
        if one_shot {
            break Ok(());
        }
    }
}

#[cfg(feature = "profiling")]
fn profile(iterations: i32, name: &str, block: &mut dyn Block) {
    let mut bar = progress::Bar::new();
    println!(
        "Now profiling the {0} block by executing {1} updates.\n \
         Use pprof to analyze {0}.profile later.",
        name, iterations
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
    let profile_runs = runs
        .parse::<i32>()
        .configuration_error("failed to parse --profile-runs as an integer")?;
    for &(ref block_name, ref block_config) in &config.blocks {
        if block_name == name {
            let mut block =
                create_block(0, &block_name, block_config.clone(), config.clone(), update)?;
            profile(profile_runs, &block_name, block.deref_mut());
            break;
        }
    }
    Ok(())
}

#[cfg(not(feature = "profiling"))]
fn profile_config(_name: &str, _runs: &str, _config: &Config, _update: Sender<Task>) -> Result<()> {
    // TODO: Maybe we should just panic! here.
    Err(InternalError(
        "profile".to_string(),
        "The 'profiling' feature was not enabled at compile time.".to_string(),
        None,
    ))
}
