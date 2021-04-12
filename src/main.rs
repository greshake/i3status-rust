#[macro_use]
mod de;
#[macro_use]
mod util;
pub mod blocks;
mod config;
mod errors;
mod formatting;
mod http;
mod icons;
mod protocol;
mod scheduler;
mod signals;
mod subprocess;
mod themes;
mod widgets;

#[cfg(feature = "profiling")]
use cpuprofiler::PROFILER;
#[cfg(feature = "profiling")]
use std::ops::DerefMut;

#[cfg(feature = "pulseaudio")]
use libpulse_binding as pulse;

use std::time::Duration;

use clap::{crate_authors, crate_description, App, Arg, ArgMatches};
use crossbeam_channel::{select, Receiver, Sender};
use futures::StreamExt;

use crate::blocks::{block_into_stream, create_block, Block};
use crate::config::Config;
use crate::config::SharedConfig;
use crate::errors::*;
use crate::protocol::i3bar_event::{process_events, I3BarEvent};
use crate::scheduler::{Task, UpdateScheduler};
use crate::signals::process_signals;
use crate::util::deserialize_file;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

#[tokio::main(flavor = "current_thread")]
async fn main() {
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

    let mut builder = App::new("i3status-rs");
    builder = builder
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
        )
        .arg(
            Arg::with_name("no-init")
                .help("Do not send an init sequence")
                .long("no-init")
                .takes_value(false)
                .hidden(true),
        );

    #[cfg(feature = "profiling")]
    {
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
    }

    let matches = builder.get_matches();
    let exit_on_error = matches.is_present("exit-on-error");

    // Run and match for potential error
    if let Err(error) = run(&matches).await {
        if exit_on_error {
            eprintln!("{:?}", error);
            ::std::process::exit(1);
        }

        // Create widget with error message
        let error_widget = TextWidget::new(0, 0, Default::default())
            .with_state(State::Critical)
            .with_text(&format!("{:?}", error));

        // Print errors
        println!("[{}],", error_widget.get_data().render());
        eprintln!("\n\n{:?}", error);

        // Wait for USR2 signal to restart
        signal_hook::iterator::Signals::new(&[signal_hook::consts::SIGUSR2])
            .unwrap()
            .forever()
            .next()
            .unwrap();
        restart();
    }
}

async fn run(matches: &ArgMatches<'_>) -> Result<()> {
    if !matches.is_present("no-init") {
        // Now we can start to run the i3bar protocol
        protocol::init(matches.is_present("never-pause"));
    }

    // Read & parse the config file
    let config_path = match matches.value_of("config") {
        Some(config_path) => std::path::PathBuf::from(config_path),
        None => util::xdg_config_home().join("i3status-rust/config.toml"),
    };
    let config: Config = deserialize_file(&config_path)?;

    // Update request channel
    let (tx_update_requests, rx_update_requests): (Sender<Task>, Receiver<Task>) =
        crossbeam_channel::unbounded();

    // In dev build, we might diverge into profiling blocks here
    #[cfg(feature = "profiling")]
    {
        if let Some(name) = matches.value_of("profile") {
            return profile_config(
                name,
                matches.value_of("profile-runs").unwrap(),
                &config,
                tx_update_requests,
            );
        }
    }

    let shared_config = SharedConfig::new(&config);

    // Initialize the blocks
    let (block_updates, block_event_handles): (Vec<_>, Vec<_>) = config
        .blocks
        .iter()
        .enumerate()
        .map(|(id, &(ref block_name, ref block_config))| {
            create_block(
                id,
                block_name,
                block_config.clone(),
                shared_config.clone(),
                tx_update_requests.clone(),
            )
            .expect("failed to create block")
        })
        .map(block_into_stream)
        .unzip();

    // List of latest rendered widgets for each blocks
    let mut blocks_rendered: Vec<Vec<Box<dyn I3BarWidget>>> =
        (0..block_updates.len()).map(|_| Vec::new()).collect();

    let mut block_updates = futures::stream::select_all(
        block_updates
            .into_iter()
            .enumerate()
            .map(|(id, stream)| stream.map(move |val| (id, val))),
    );

    while let Some((id, rendered)) = block_updates.next().await {
        blocks_rendered[id] = rendered;
        protocol::print_blocks(&blocks_rendered, &shared_config)?;
    }

    // TODO: We wait for click events in a separate thread, to avoid blocking to wait for stdin
    //
    // let (tx_clicks, rx_clicks): (Sender<I3BarEvent>, Receiver<I3BarEvent>) =
    //     crossbeam_channel::unbounded();
    //
    // process_events(tx_clicks);

    // TODO: We wait for signals in a separate thread
    // let (tx_signals, rx_signals): (Sender<i32>, Receiver<i32>) = crossbeam_channel::unbounded();
    // process_signals(tx_signals);

    // // Time to next update channel.
    // // Fires immediately for first updates
    // let mut ttnu = crossbeam_channel::after(Duration::from_millis(0));
    //
    // let one_shot = matches.is_present("one-shot");
    // loop {
    //     // We use the message passing concept of channel selection
    //     // to avoid busy wait
    //     select! {
    //         // Receive click events
    //         recv(rx_clicks) -> res => if let Ok(event) = res {
    //             if let Some(id) = event.id {
    //                     blocks.get_mut(id)
    //                 .internal_error("click handler", "could not get required block")?
    //                         .click(event)?;
    //                 protocol::print_blocks(&blocks_rendered, &shared_config)?;
    //             }
    //         },
    //         // Receive async update requests
    //         recv(rx_update_requests) -> request => if let Ok(req) = request {
    //             // Process immediately and forget
    //             blocks_rendered[req.id] = blocks[req.id].render()?;
    //             protocol::print_blocks(&blocks_rendered, &shared_config)?;
    //         },
    //         // Receive update timer events
    //         recv(ttnu) -> _ => {
    //             scheduler.do_scheduled_updates(&mut blocks, &mut blocks_rendered)?;
    //             protocol::print_blocks(&blocks_rendered, &shared_config)?;
    //         },
    //         // Receive signal events
    //         recv(rx_signals) -> res => if let Ok(sig) = res {
    //             match sig {
    //                 signal_hook::consts::SIGUSR1 => {
    //                     // USR1 signal that updates every block in the bar
    //                     // TODO: quésako?
    //                     // for block in blocks.iter_mut() {
    //                     //     block.update()?;
    //                     // }
    //                 },
    //                 signal_hook::consts::SIGUSR2 => {
    //                     //USR2 signal that should reload the config
    //                     restart();
    //                 },
    //                 _ => {
    //                     //Real time signal that updates only the blocks listening
    //                     //for that signal
    //                     for block in blocks.iter_mut() {
    //                         block.signal(sig)?;
    //                     }
    //                 },
    //             };
    //             protocol::print_blocks(&blocks_rendered, &shared_config)?;
    //         }
    //     }
    //
    //     // Set the time-to-next-update timer
    //     if let Some(time) = scheduler.time_to_next_update() {
    //         ttnu = crossbeam_channel::after(time)
    //     }
    //     if one_shot {
    //         break Ok(());
    //     }
    // }

    todo!()
}

/// Restart `i3status-rs` in-place
fn restart() -> ! {
    use std::env;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStringExt;

    // On linux this line should be OK
    let exe = CString::new(env::current_exe().unwrap().into_os_string().into_vec()).unwrap();

    // Get current arguments
    let mut arg = env::args()
        .map(|a| CString::new(a).unwrap())
        .collect::<Vec<CString>>();

    // Add "--no-init" argument if not already added
    let no_init_arg = CString::new("--no-init").unwrap();
    if !arg.iter().any(|a| *a == no_init_arg) {
        arg.push(no_init_arg);
    }

    // Restart
    nix::unistd::execvp(&exe, &arg).unwrap();
    unreachable!();
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
    let shared_config = SharedConfig::new(&config);
    for &(ref block_name, ref block_config) in &config.blocks {
        if block_name == name {
            let mut block =
                create_block(0, &block_name, block_config.clone(), shared_config, update)?;
            profile(profile_runs, &block_name, block.deref_mut());
            break;
        }
    }
    Ok(())
}
