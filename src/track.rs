use std::time::Duration;
pub struct Track {
    pub name: String,
    pub duration: Duration,
}

impl Clone for Track {
    fn clone(&self) -> Self {
        Track {
            name: self.name.clone(),
            duration: self.duration,
        }
    }
}

impl PartialEq for Track {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.duration == other.duration
    }
}

impl PartialOrd for Track {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl Eq for Track {}

impl Ord for Track {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}
