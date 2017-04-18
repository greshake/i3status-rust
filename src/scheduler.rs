use block::Block;
use std::collections::{BinaryHeap, HashMap};
use std::thread;
use std::cmp;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Task<'a> {
    block: &'a Block,
    update_time: Instant,
}

pub struct UpdateRequest {
    pub id: String,
    pub update_time: Instant
}

impl<'a> cmp::PartialEq for Task<'a> {
    fn eq(&self, other: &Task) -> bool {
        self.update_time.eq(&other.update_time)
    }
}

impl<'a> cmp::Eq for Task<'a> {}

impl<'a> cmp::PartialOrd for Task<'a> {
    fn partial_cmp(&self, other: &Task) -> Option<cmp::Ordering> {
        other.update_time.partial_cmp(&self.update_time)
    }
}

impl<'a> cmp::Ord for Task<'a> {
    fn cmp(&self, other: &Task) -> cmp::Ordering {
        other.update_time.cmp(&self.update_time)
    }
}


pub struct UpdateScheduler<'a> {
    schedule: BinaryHeap<Task<'a>>,
    block_map: HashMap<String, &'a Block>
}

impl<'a> UpdateScheduler<'a> {
    pub fn new(blocks: &Vec<&'a Block>) -> UpdateScheduler<'a> {
        let mut schedule = BinaryHeap::new();
        let mut block_map = HashMap::new();

        let now = Instant::now();
        for block in blocks.iter() {
            schedule.push(Task {
                block: *block,
                update_time: now.clone(),
            });

            if let Some(id) = block.id() {
                block_map.insert(String::from(id.clone()), *block);
            }
        }

        UpdateScheduler {
            schedule: schedule,
            block_map: block_map
        }
    }

    pub fn schedule(&mut self, id: String, time: Instant) {
        self.schedule
            .push(Task {
                block: *self.block_map.get(&id).expect(&format!("Update Request contains invalid block id: {}", id)),
                update_time: time,
            })
    }

    pub fn time_to_next_update(&self) -> Duration {
        let next_update = self.schedule.peek().unwrap().update_time;
        let now = Instant::now();

        if next_update > now {
            next_update - now
        } else {
            Duration::new(0, 0)
        }
    }

    pub fn do_scheduled_updates(&mut self) {
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
            if let Some(dur) = task.block.update() {
                self.schedule
                    .push(Task {
                        block: task.block,
                        update_time: now + dur,
                    })
            }
        }
    }
}
