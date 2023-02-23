use crate::BoxedStream;
use futures::stream::StreamExt;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub fn manage_widgets_updates() -> (UnboundedSender<(usize, Vec<u64>)>, BoxedStream<Vec<usize>>) {
    let (intervals_tx, intervals_rx) = unbounded_channel::<(usize, Vec<u64>)>();
    struct State {
        time_anchor: Instant,
        last_update: u64,
        intervals_rx: UnboundedReceiver<(usize, Vec<u64>)>,
        intervals: Vec<(usize, Vec<u64>)>,
    }
    let stream = futures::stream::unfold(
        State {
            time_anchor: Instant::now(),
            last_update: 0,
            intervals_rx,
            intervals: Vec::new(),
        },
        |mut state| async move {
            loop {
                if state.intervals.is_empty() {
                    let (id, new_intervals) = state.intervals_rx.recv().await?;
                    state.intervals.retain(|(i, _)| *i != id);
                    if !new_intervals.is_empty() {
                        state.intervals.push((id, new_intervals));
                    }
                    continue;
                }

                let time = state.time_anchor.elapsed().as_millis() as u64;

                let mut blocks = Vec::new();
                let mut delay = 100000;
                for (id, intervals) in &state.intervals {
                    let block_delay = single_block_next_update(intervals, time, state.last_update);
                    if block_delay < delay {
                        delay = block_delay;
                        blocks.clear();
                    }
                    if block_delay == delay {
                        blocks.push(*id);
                    }
                }

                if delay == 0 {
                    state.last_update = time;
                    return Some((blocks, state));
                }

                if let Ok(Some((id, new_intervals))) =
                    tokio::time::timeout(Duration::from_millis(delay), state.intervals_rx.recv())
                        .await
                {
                    state.intervals.retain(|(i, _)| *i != id);
                    if !new_intervals.is_empty() {
                        state.intervals.push((id, new_intervals));
                    }
                }
            }
        },
    )
    .boxed();
    (intervals_tx, stream)
}

fn single_block_next_update(intervals: &[u64], time: u64, last_update: u64) -> u64 {
    fn next_update(time: u64, interval: u64) -> u64 {
        time + interval - time % interval
    }
    let mut time_to_next = u64::MAX;
    for &interval in intervals {
        if next_update(last_update, interval) <= time {
            return 0;
        }
        time_to_next = time_to_next.min(next_update(time, interval) - time);
    }
    time_to_next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_block() {
        //     0   100  200  300  400  500  600  700  800  900  1000
        //     |    |    |    |    |    |    |    |    |    |    |
        // 200 x         x         x         x         x         x
        // 300 x              x              x              x
        // 500 x                        x                        x
        let intervals = &[200, 300, 500];
        assert_eq!(single_block_next_update(intervals, 0, 0), 200);
        assert_eq!(single_block_next_update(intervals, 50, 0), 150);
        assert_eq!(single_block_next_update(intervals, 210, 50), 0);
        assert_eq!(single_block_next_update(intervals, 290, 210), 10);
        assert_eq!(single_block_next_update(intervals, 300, 290), 0);
        assert_eq!(single_block_next_update(intervals, 300, 300), 100);
        assert_eq!(single_block_next_update(intervals, 800, 300), 0);
    }
}
