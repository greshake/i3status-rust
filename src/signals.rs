use futures::stream::StreamExt as _;
use signal_hook::consts::{SIGUSR1, SIGUSR2};
use signal_hook_tokio::Signals;

use crate::BoxedStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Usr1,
    Usr2,
    #[cfg(target_os = "linux")]
    Custom(i32),
}

/// Returns an infinite stream of `Signal`s
#[cfg(target_os = "linux")]
pub fn signals_stream() -> BoxedStream<Signal> {
    let (sigmin, sigmax) = (libc::SIGRTMIN(), libc::SIGRTMAX());
    let signals = Signals::new((sigmin..sigmax).chain([SIGUSR1, SIGUSR2])).unwrap();
    signals
        .map(move |signal| match signal {
            SIGUSR1 => Signal::Usr1,
            SIGUSR2 => Signal::Usr2,
            x => Signal::Custom(x - sigmin),
        })
        .boxed()
}

#[cfg(not(target_os = "linux"))]
pub fn signals_stream() -> BoxedStream<Signal> {
    {
        let signals = Signals::new([SIGUSR1, SIGUSR2]).unwrap();
        signals
            .filter_map(async move |signal| match signal {
                SIGUSR1 => Some(Signal::Usr1),
                SIGUSR2 => Some(Signal::Usr2),
                _ => None,
            })
            .boxed()
    }
}
