//! 100-entry ring buffer with up/down navigation.

pub(crate) struct History {
    entries: Vec<Vec<u8>>,
    position: usize,
}

impl History {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            position: 0,
        }
    }
}
