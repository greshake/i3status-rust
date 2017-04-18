use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct RotatingText {
    rotation_pos: usize,
    width: usize,
    rotation_interval: Duration,
    rotation_speed: Duration,
    next_rotation: Option<Instant>,
    content: String,
    pub rotating: bool
}


impl RotatingText {
    pub fn new(interval: Duration, speed: Duration, width: usize) -> RotatingText {
        RotatingText {
            rotation_pos: 0,
            width: width,
            rotation_interval: interval,
            rotation_speed: speed,
            next_rotation: None,
            content: String::new(),
            rotating: false
        }
    }

    pub fn set_content(&mut self, content: String) {
        if self.content != content{
            self.content = content;
            self.rotation_pos = 0;
            if self.content.len() > self.width {
                self.next_rotation = Some(Instant::now() + self.rotation_interval);
            } else {
                self.next_rotation = None;
            }
        }
    }

    pub fn to_string(&self) -> String {
        if self.content.len() > self.width {
            let missing = (self.rotation_pos + self.width).saturating_sub(self.content.len());
            if missing == 0 {
                self.content.chars().skip(self.rotation_pos).take(self.width).collect()
            } else {
                let mut avail: String = self.content.chars().skip(self.rotation_pos).take(self.width).collect();
                avail.push_str("|");
                avail.push_str(&self.content.chars().take(missing - 1).collect::<String>());
                avail
            }
            
        } else {
            self.content.clone()
        }
    }

    pub fn next(&mut self) -> (bool, Option<Duration>) {
        if let Some(next_rotation) = self.next_rotation {
            if next_rotation > Instant::now() {
                (false, Some(next_rotation - Instant::now()))
            } else {
                if self.rotating {
                    if self.rotation_pos < self.content.len() {
                        self.rotation_pos += 1;
                        self.next_rotation = Some(Instant::now() + self.rotation_speed);
                        (true, Some(self.rotation_speed))
                    } else {
                        self.rotation_pos = 0;
                        self.rotating = false;
                        self.next_rotation = Some(Instant::now() + self.rotation_interval);
                        (true, Some(self.rotation_interval))
                    }
                } else {
                    self.rotating = true;
                    (true, Some(self.rotation_speed))
                }
            }
        } else {
            (false, None)
        }
    }
}
