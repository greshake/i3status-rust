use crate::errors::*;
use crossbeam_channel::Sender;
use std::thread;

/// Starts a thread that listens for provided signals and sends these on the provided channel
pub fn process_signals(sender: Sender<i32>) {
    thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            let sigmin;
            let sigmax;
            unsafe {
                sigmin = __libc_current_sigrtmin();
                sigmax = __libc_current_sigrtmax();
            }
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
    let sigmin;
    let sigmax;
    unsafe {
        sigmin = __libc_current_sigrtmin();
        sigmax = __libc_current_sigrtmax();
    }
    if signal < 0 || signal > sigmax - sigmin {
        //NOTE If some important information is encoded in the third field of this error this might
        //need to be added
        Err(Error::ConfigurationError(
            format!(
            "A provided signal was out of bounds. An allowed signal needs to be between {} and {}",
            0,
            sigmax - sigmin
        ),
            format!(
                "Provided signal is {} which is not between {} and {}",
                signal,
                0,
                sigmax - sigmin
            ),
        ))
    } else {
        Ok(signal + sigmin)
    }
}

//TODO when libc exposes this through their library and even better when the nix crate does we
//should be using that binding rather than a C-binding.
///C bindings to SIGMIN and SIGMAX values
extern "C" {
    fn __libc_current_sigrtmin() -> i32;
    fn __libc_current_sigrtmax() -> i32;
}
