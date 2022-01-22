use std::os::unix::io::FromRawFd;
use std::time::Duration;

use serde_derive::Deserialize;

use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::{channel, Receiver};

use crate::click::MouseButton;

#[derive(Deserialize, Debug, Clone)]
struct I3BarEventInternal {
    pub name: Option<String>,
    pub instance: Option<String>,
    pub button: MouseButton,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct I3BarEvent {
    pub id: usize,
    pub instance: Option<usize>,
    pub button: MouseButton,
}

fn unprocessed_events_stream(invert_scrolling: bool) -> Receiver<I3BarEvent> {
    // Avoid spawning a blocking therad (why doesn't tokio do this too?)
    // This should be safe given that this function is called only once
    let stdin = unsafe { File::from_raw_fd(0) };
    let mut stdin = BufReader::new(stdin);

    let mut buf = String::new();
    let (tx, rx) = channel(32);

    tokio::spawn(async move {
        loop {
            buf.clear();
            stdin.read_line(&mut buf).await.unwrap();

            // Take only the valid JSON object betweem curly braces (cut off leading bracket,
            // commas and whitespace)
            let slice = buf.trim_start_matches(|c| c != '{');
            let slice = slice.trim_end_matches(|c| c != '}');

            if !slice.is_empty() {
                let event: I3BarEventInternal = serde_json::from_str(slice).unwrap();
                let id = match event.name {
                    Some(name) => name.parse().unwrap(),
                    None => continue,
                };
                let instance = event.instance.map(|x| x.parse::<usize>().unwrap());

                use MouseButton::*;
                let button = match (event.button, invert_scrolling) {
                    (WheelUp, false) | (WheelDown, true) => WheelUp,
                    (WheelUp, true) | (WheelDown, false) => WheelDown,
                    (other, _) => other,
                };

                if tx
                    .send(I3BarEvent {
                        id,
                        instance,
                        button,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    });

    rx
}

pub fn events_stream(invert_scrolling: bool, double_click_delay: Duration) -> Receiver<I3BarEvent> {
    let mut events = unprocessed_events_stream(invert_scrolling);
    let (tx, rx) = channel(32);

    tokio::spawn(async move {
        loop {
            let mut event = events.recv().await.unwrap();

            // Handle double clicks (for now only left)
            if event.button == MouseButton::Left {
                tokio::select! {
                    _ = tokio::time::sleep(double_click_delay) => (),
                    Some(new_event) = events.recv() => {
                        if event == new_event {
                            event.button = MouseButton::DoubleLeft;
                        } else {
                            if tx.send(event).await.is_err() {
                                break;
                            }
                            event = new_event;
                        }
                    }
                }
            }

            if tx.send(event).await.is_err() {
                break;
            }
        }
    });

    rx
}
