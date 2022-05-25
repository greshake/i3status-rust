use std::os::unix::io::FromRawFd;
use std::time::Duration;

use serde::Deserialize;

use futures::StreamExt;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::click::MouseButton;
use crate::BoxedStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct I3BarEvent {
    pub id: usize,
    pub instance: Option<usize>,
    pub button: MouseButton,
}

fn unprocessed_events_stream(invert_scrolling: bool) -> BoxedStream<I3BarEvent> {
    // Avoid spawning a blocking therad (why doesn't tokio do this too?)
    // This should be safe given that this function is called only once
    let stdin = unsafe { File::from_raw_fd(0) };
    let lines = BufReader::new(stdin).lines();

    futures::stream::unfold(lines, move |mut lines| async move {
        loop {
            // Take only the valid JSON object betweem curly braces (cut off leading bracket, commas and whitespace)
            let line = lines.next_line().await.ok().flatten()?;
            let line = line.trim_start_matches(|c| c != '{');
            let line = line.trim_end_matches(|c| c != '}');

            if line.is_empty() {
                continue;
            }

            #[derive(Deserialize)]
            struct I3BarEventRaw {
                name: Option<String>,
                instance: Option<String>,
                button: MouseButton,
            }

            let event: I3BarEventRaw = serde_json::from_str(line).unwrap();
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

            let event = I3BarEvent {
                id,
                instance,
                button,
            };

            break Some((event, lines));
        }
    })
    .boxed_local()
}

pub fn events_stream(
    invert_scrolling: bool,
    double_click_delay: Duration,
) -> BoxedStream<I3BarEvent> {
    let events = unprocessed_events_stream(invert_scrolling);
    futures::stream::unfold((events, None), move |(mut events, pending)| async move {
        if let Some(pending) = pending {
            return Some((pending, (events, None)));
        }

        let mut event = events.next().await?;

        // Handle double clicks (for now only left)
        if event.button == MouseButton::Left && !double_click_delay.is_zero() {
            if let Ok(new_event) = tokio::time::timeout(double_click_delay, events.next()).await {
                let new_event = new_event?;
                if event == new_event {
                    event.button = MouseButton::DoubleLeft;
                } else {
                    return Some((event, (events, Some(new_event))));
                }
            }
        }

        Some((event, (events, None)))
    })
    .boxed_local()
}
