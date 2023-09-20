use clap::Parser;

use i3status_rs::blocks::BlockError;
use i3status_rs::config::Config;
use i3status_rs::errors::*;
use i3status_rs::escape::Escaped;
use i3status_rs::widget::{State, Widget};
use i3status_rs::{protocol, util, BarState};

#[derive(Debug, thiserror::Error)]
enum ErrorMaybeInBlock {
    #[error(transparent)]
    InBlock(#[from] BlockError),
    #[error(transparent)]
    NotInBlock(#[from] Error),
}

fn main() {
    env_logger::init();

    let args = i3status_rs::CliArgs::parse();
    let blocking_threads = args.blocking_threads;

    if !args.no_init {
        protocol::init(args.never_pause);
    }

    let result: Result<(), ErrorMaybeInBlock> = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(blocking_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let config_path = util::find_file(&args.config, None, Some("toml"))
                .or_error(|| format!("Configuration file '{}' not found", args.config))?;
            let mut config: Config = util::deserialize_toml_file(&config_path)?;
            let blocks = std::mem::take(&mut config.blocks);
            let mut bar = BarState::new(config);
            for block_config in blocks {
                bar.spawn_block(block_config).await?;
            }
            bar.run_event_loop(restart).await?;
            Ok(())
        });
    if let Err(error) = result {
        let error_widget = Widget::new()
            .with_text(error.to_string().pango_escaped())
            .with_state(State::Critical);

        println!(
            "{},",
            serde_json::to_string(&error_widget.get_data(&Default::default(), 0).unwrap()).unwrap()
        );
        eprintln!("\n\n{error}\n\n");
        dbg!(error);

        // Wait for USR2 signal to restart
        signal_hook::iterator::Signals::new([signal_hook::consts::SIGUSR2])
            .unwrap()
            .forever()
            .next()
            .unwrap();
        restart();
    }
}

/// Restart in-place
fn restart() -> ! {
    use std::env;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStringExt;

    // On linux this line should be OK
    let exe = CString::new(env::current_exe().unwrap().into_os_string().into_vec()).unwrap();

    // Get current arguments
    let mut arg: Vec<CString> = env::args_os()
        .map(|a| CString::new(a.into_vec()).unwrap())
        .collect();

    // Add "--no-init" argument if not already added
    let no_init_arg = CString::new("--no-init").unwrap();
    if !arg.iter().any(|a| *a == no_init_arg) {
        arg.push(no_init_arg);
    }

    // Restart
    nix::unistd::execvp(&exe, &arg).unwrap();
    unreachable!();
}
