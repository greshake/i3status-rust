// TODO: scheduling is now handled by tokio's runtime, maybe this module
//       should be renamed `task`.

use std::cmp;
use std::fmt;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize,
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
