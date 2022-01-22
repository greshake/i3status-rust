use futures::stream::StreamExt;
use libc::{SIGRTMAX, SIGRTMIN};
use signal_hook::consts::{SIGUSR1, SIGUSR2};
use signal_hook_tokio::Signals;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Usr1,
    Usr2,
    Custom(i32),
}

/// Spawn a task that listens for signals and sends these on the returned channel
pub fn signals_stream() -> mpsc::Receiver<Signal> {
    let (tx, rx) = mpsc::channel(32);

    let (sigmin, sigmax) = (SIGRTMIN(), SIGRTMAX());
    let mut signals = Signals::new((sigmin..sigmax).chain(Some(SIGUSR1)).chain(Some(SIGUSR2)))
        .unwrap()
        .fuse();

    tokio::spawn(async move {
        loop {
            if tx
                .send(match signals.next().await {
                    Some(SIGUSR1) => Signal::Usr1,
                    Some(SIGUSR2) => Signal::Usr2,
                    Some(x) => Signal::Custom(x - sigmin),
                    None => {
                        eprintln!("signals.next() returned None: no more signals will be received");
                        break;
                    }
                })
                .await
                .is_err()
            {
                // Receiver is dropped - no need to loop anymore
                break;
            }
        }
    });

    rx
}
