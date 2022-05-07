use crate::errors::*;
use crossbeam_channel::Sender;
use libc::{SIGRTMAX, SIGRTMIN};
use std::thread;

/// Starts a thread that listens for provided signals and sends these on the provided channel
pub fn process_signals(sender: Sender<i32>) {
    thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            let (sigmin, sigmax) = (SIGRTMIN(), SIGRTMAX());
            loop {
                let mut signals = (sigmin..sigmax).collect::<Vec<_>>();
                signals.push(signal_hook::consts::SIGUSR1);
                signals.push(signal_hook::consts::SIGUSR2);
                let mut signals = signal_hook::iterator::Signals::new(&signals).unwrap();
                for sig in signals.forever() {
                    sender.send(sig).unwrap();
                }
            }
        })
        .unwrap();
}

pub fn convert_to_valid_signal(signal: i32) -> Result<i32> {
    let (sigmin, sigmax) = (SIGRTMIN(), SIGRTMAX());
    if signal < 0 || signal > sigmax - sigmin {
        //NOTE If some important information is encoded in the third field of this error this might
        //need to be added
        Err(Error::new(format!(
            "A provided signal was out of bounds. An allowed signal needs to be between 0 and {}",
            sigmax - sigmin
        )))
    } else {
        Ok(signal + sigmin)
    }
}
