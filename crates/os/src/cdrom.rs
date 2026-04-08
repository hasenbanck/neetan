//! MSCDEX state and CD-ROM device driver request handling.

pub(crate) struct MscdexState {
    pub active: bool,
    pub device_name: Vec<u8>,
}

impl MscdexState {
    pub(crate) fn new() -> Self {
        Self {
            active: false,
            device_name: Vec::new(),
        }
    }
}
