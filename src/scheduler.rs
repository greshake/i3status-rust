use block::Block;
use std::collections::{BinaryHeap, HashMap};
use std::thread;
use std::cmp;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::sync::mpsc::Sender;

#[derive(Clone)]
pub struct Task {
    pub id: String,
    pub update_time: Instant,
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


pub struct UpdateScheduler  {
    schedule: BinaryHeap<Task>
}

impl UpdateScheduler {
    pub fn new(blocks: &Vec<Box<Block>>) -> UpdateScheduler {
        let mut schedule = BinaryHeap::new();

        let now = Instant::now();
        for block in blocks.iter() {
            schedule.push(Task {
                id: String::from(block.id()),
                update_time: now.clone(),
            });
        }

        UpdateScheduler {
            schedule: schedule
        }
    }

    pub fn schedule(&mut self, task: Task) {
        self.schedule.push(task);
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

    pub fn do_scheduled_updates(&mut self, block_map: &mut HashMap<String, &mut Block>) {
        let t = self.schedule.pop().unwrap();
        let mut tasks_next = vec![t.clone()];

        while !self.schedule.is_empty() &&
            t.update_time == self.schedule.peek().unwrap().update_time {
            tasks_next.push(self.schedule.pop().unwrap())
        }

        if t.update_time > Instant::now() {
            thread::sleep(t.update_time - Instant::now());
        }

        let now = Instant::now();

        for task in tasks_next {
            if let Some(dur) = block_map.get_mut(&task.id).unwrap().update() {
                self.schedule
                    .push(Task {
                        id: task.id,
                        update_time: now + dur,
                    })
            }
        }
    }
}
