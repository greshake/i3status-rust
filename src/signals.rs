use crossbeam_channel::Sender;
use libc::{SIGRTMAX, SIGRTMIN};
use signal_hook::consts::{SIGUSR1, SIGUSR2};
use std::thread;

pub enum Signal {
    SigUsr1,
    SigUsr2,
    Other(i32),
}

/// Starts a thread that listens for provided signals and sends these on the provided channel
pub fn process_signals(sender: Sender<Signal>) {
    thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            let (sigmin, sigmax) = (SIGRTMIN(), SIGRTMAX());
            loop {
                let mut signals =
                    signal_hook::iterator::Signals::new((sigmin..sigmax).chain([SIGUSR1, SIGUSR2]))
                        .unwrap();
                for sig in signals.forever() {
                    let _ = sender.send(match sig {
                        SIGUSR1 => Signal::SigUsr1,
                        SIGUSR2 => Signal::SigUsr2,
                        other => Signal::Other(other - sigmin),
                    });
                }
            }
        })
        .unwrap();
}
