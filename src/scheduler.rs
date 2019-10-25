use crate::blocks::Block;
use crate::errors::*;
use std::collections::{BinaryHeap, HashMap};
use std::fmt;
use std::thread;
use std::cmp;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub update_time: Instant,
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl cmp::PartialEq for Task {
    fn eq(&self, other: &Task) -> bool {
        self.update_time.eq(&other.update_time)
    }
}

impl cmp::Eq for Task {}

impl cmp::PartialOrd for Task {
    fn partial_cmp(&self, other: &Task) -> Option<cmp::Ordering> {
        other.update_time.partial_cmp(&self.update_time)
    }
}

impl cmp::Ord for Task {
    fn cmp(&self, other: &Task) -> cmp::Ordering {
        other.update_time.cmp(&self.update_time)
    }
}

pub struct UpdateScheduler {
    schedule: BinaryHeap<Task>,
}

impl UpdateScheduler {
    pub fn new(blocks: &[Box<dyn Block>]) -> UpdateScheduler {
        let mut schedule = BinaryHeap::new();

        let now = Instant::now();
        for block in blocks.iter() {
            schedule.push(Task {
                id: String::from(block.id()),
                update_time: now,
            });
        }

        UpdateScheduler { schedule }
    }

    pub fn time_to_next_update(&self) -> Option<Duration> {
        if let Some(peeked) = self.schedule.peek() {
            let next_update = peeked.update_time;
            let now = Instant::now();

            if next_update > now {
                Some(next_update - now)
            } else {
                Some(Duration::new(0, 0))
            }
        } else {
            None
        }
    }

    pub fn do_scheduled_updates(&mut self, block_map: &mut HashMap<String, &mut dyn Block>) -> Result<()> {
        let t = self.schedule
            .pop()
            .internal_error("scheduler", "schedule is empty")?;
        let mut tasks_next = vec![t.clone()];

        while !self.schedule.is_empty() &&
            t.update_time ==
                self.schedule
                    .peek()
                    .internal_error("scheduler", "schedule is empty")?
                    .update_time
        {
            tasks_next.push(self.schedule
                .pop()
                .internal_error("scheduler", "schedule is empty")?)
        }

        let now = Instant::now();
        if t.update_time > now {
            thread::sleep(t.update_time - now);
        }

        let now = Instant::now();

        for task in tasks_next {
            if let Some(dur) = block_map
                .get_mut(&task.id)
                .internal_error("scheduler", "could not get required block")?
                .update()?
            {
                self.schedule.push(Task {
                    id: task.id,
                    update_time: now + dur,
                })
            }
        }

        Ok(())
    }
}
