use crate::errors::*;
use crossbeam_channel::Sender;
use std::thread;
/// These are the SIGRTMIN and SIGRTMAX values used to determine the allowed range of signal values
//FIXME currently these are hardcoded and tested on my system, I am not sure how to bring these in
//dynamically from the host OS, as such they may vary with OS
const SIGMIN: i32 = 34;
const SIGMAX: i32 = 64;

/// Starts a thread that listens for provided signals and sends these on the provided channel
pub fn process_signals(sender: Sender<i32>) {
    thread::Builder::new()
        .name("signals".into())
        .spawn(move || loop {
            let signals =
                signal_hook::iterator::Signals::new(&(SIGMIN..SIGMAX).collect::<Vec<_>>()).unwrap();
            for sig in signals.forever() {
                sender.send(sig).unwrap();
            }
        })
        .unwrap();
}

pub fn convert_to_valid_signal(signal: i32) -> Result<i32> {
    if signal < 0 || signal > SIGMAX - SIGMIN {
        //NOTE If some important information is encoded in the third field of this error this might
        //need to be added
        return Err(Error::ConfigurationError(
            format!(
            "A provided signal was out of bounds. An allowed signal needs to be between {} and {}",
            0,
            SIGMAX - SIGMIN
        ),
            (
                format!(
                    "Provided signal is {} which is not between {} and {}",
                    signal,
                    0,
                    SIGMAX - SIGMIN
                ),
                String::new(),
            ),
        ));
    } else {
        return Ok(signal + SIGMIN);
    }
}
