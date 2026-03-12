use std::cell::{Cell, RefCell};

use ymfm_oxide::{OplCallbacks, Y8950Callbacks, Ym2203Callbacks, Ym2608Callbacks, YmfmAccessClass};

#[derive(Debug, Clone, PartialEq)]
pub enum CallbackEvent {
    SetTimer {
        timer_id: u32,
        duration_in_clocks: i32,
    },
    SetBusyEnd {
        clocks: u32,
    },
    IsBusy,
    UpdateIrq {
        asserted: bool,
    },
}

pub struct RecordingCallbacks2203 {
    pub events: RefCell<Vec<CallbackEvent>>,
    pub busy: Cell<bool>,
}

impl RecordingCallbacks2203 {
    pub fn new() -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            busy: Cell::new(false),
        }
    }

    pub fn take_events(&self) -> Vec<CallbackEvent> {
        self.events.borrow_mut().drain(..).collect()
    }
}

impl Ym2203Callbacks for RecordingCallbacks2203 {
    fn set_timer(&self, timer_id: u32, duration_in_clocks: i32) {
        self.events.borrow_mut().push(CallbackEvent::SetTimer {
            timer_id,
            duration_in_clocks,
        });
    }

    fn set_busy_end(&self, clocks: u32) {
        self.events
            .borrow_mut()
            .push(CallbackEvent::SetBusyEnd { clocks });
    }

    fn is_busy(&self) -> bool {
        self.events.borrow_mut().push(CallbackEvent::IsBusy);
        self.busy.get()
    }

    fn update_irq(&self, asserted: bool) {
        self.events
            .borrow_mut()
            .push(CallbackEvent::UpdateIrq { asserted });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallbackEventExt {
    SetTimer {
        timer_id: u32,
        duration_in_clocks: i32,
    },
    SetBusyEnd {
        clocks: u32,
    },
    IsBusy,
    UpdateIrq {
        asserted: bool,
    },
    ExternalRead {
        access_class: YmfmAccessClass,
        address: u32,
    },
    ExternalWrite {
        access_class: YmfmAccessClass,
        address: u32,
        data: u8,
    },
}

pub struct RecordingCallbacks2608 {
    pub events: RefCell<Vec<CallbackEventExt>>,
    pub busy: Cell<bool>,
    pub adpcm_memory: RefCell<Vec<u8>>,
}

impl RecordingCallbacks2608 {
    pub fn new() -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            busy: Cell::new(false),
            adpcm_memory: RefCell::new(vec![0; 256 * 1024]),
        }
    }

    pub fn with_adpcm_data(data: Vec<u8>) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            busy: Cell::new(false),
            adpcm_memory: RefCell::new(data),
        }
    }

    pub fn take_events(&self) -> Vec<CallbackEventExt> {
        self.events.borrow_mut().drain(..).collect()
    }
}

pub struct RecordingCallbacksOpl {
    pub events: RefCell<Vec<CallbackEvent>>,
    pub busy: Cell<bool>,
}

impl RecordingCallbacksOpl {
    pub fn new() -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            busy: Cell::new(false),
        }
    }

    pub fn take_events(&self) -> Vec<CallbackEvent> {
        self.events.borrow_mut().drain(..).collect()
    }
}

impl OplCallbacks for RecordingCallbacksOpl {
    fn set_timer(&self, timer_id: u32, duration_in_clocks: i32) {
        self.events.borrow_mut().push(CallbackEvent::SetTimer {
            timer_id,
            duration_in_clocks,
        });
    }

    fn set_busy_end(&self, clocks: u32) {
        self.events
            .borrow_mut()
            .push(CallbackEvent::SetBusyEnd { clocks });
    }

    fn is_busy(&self) -> bool {
        self.events.borrow_mut().push(CallbackEvent::IsBusy);
        self.busy.get()
    }

    fn update_irq(&self, asserted: bool) {
        self.events
            .borrow_mut()
            .push(CallbackEvent::UpdateIrq { asserted });
    }
}

pub struct RecordingCallbacksY8950 {
    pub events: RefCell<Vec<CallbackEventExt>>,
    pub busy: Cell<bool>,
    pub adpcm_memory: RefCell<Vec<u8>>,
}

impl RecordingCallbacksY8950 {
    pub fn new() -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            busy: Cell::new(false),
            adpcm_memory: RefCell::new(vec![0; 256 * 1024]),
        }
    }

    pub fn with_adpcm_data(data: Vec<u8>) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            busy: Cell::new(false),
            adpcm_memory: RefCell::new(data),
        }
    }

    pub fn take_events(&self) -> Vec<CallbackEventExt> {
        self.events.borrow_mut().drain(..).collect()
    }
}

impl Y8950Callbacks for RecordingCallbacksY8950 {
    fn set_timer(&self, timer_id: u32, duration_in_clocks: i32) {
        self.events.borrow_mut().push(CallbackEventExt::SetTimer {
            timer_id,
            duration_in_clocks,
        });
    }

    fn set_busy_end(&self, clocks: u32) {
        self.events
            .borrow_mut()
            .push(CallbackEventExt::SetBusyEnd { clocks });
    }

    fn is_busy(&self) -> bool {
        self.events.borrow_mut().push(CallbackEventExt::IsBusy);
        self.busy.get()
    }

    fn update_irq(&self, asserted: bool) {
        self.events
            .borrow_mut()
            .push(CallbackEventExt::UpdateIrq { asserted });
    }

    fn external_read(&self, access_class: YmfmAccessClass, address: u32) -> u8 {
        self.events
            .borrow_mut()
            .push(CallbackEventExt::ExternalRead {
                access_class,
                address,
            });
        let mem = self.adpcm_memory.borrow();
        mem.get(address as usize).copied().unwrap_or(0)
    }

    fn external_write(&self, access_class: YmfmAccessClass, address: u32, data: u8) {
        self.events
            .borrow_mut()
            .push(CallbackEventExt::ExternalWrite {
                access_class,
                address,
                data,
            });
        let mut mem = self.adpcm_memory.borrow_mut();
        if (address as usize) < mem.len() {
            mem[address as usize] = data;
        }
    }
}

pub struct AdpcmTestCallbacks {
    pub memory: RefCell<[u8; 0x40000]>,
}

impl AdpcmTestCallbacks {
    pub fn new() -> Self {
        Self {
            memory: RefCell::new([0x80; 0x40000]),
        }
    }
}

impl Ym2608Callbacks for AdpcmTestCallbacks {
    fn set_timer(&self, _timer_id: u32, _duration_in_clocks: i32) {}
    fn set_busy_end(&self, _clocks: u32) {}
    fn is_busy(&self) -> bool {
        false
    }
    fn update_irq(&self, _asserted: bool) {}

    fn external_read(&self, _access_class: YmfmAccessClass, address: u32) -> u8 {
        let mem = self.memory.borrow();
        mem[address as usize % mem.len()]
    }

    fn external_write(&self, _access_class: YmfmAccessClass, address: u32, data: u8) {
        let mut mem = self.memory.borrow_mut();
        let len = mem.len();
        mem[address as usize % len] = data;
    }
}

impl Ym2608Callbacks for RecordingCallbacks2608 {
    fn set_timer(&self, timer_id: u32, duration_in_clocks: i32) {
        self.events.borrow_mut().push(CallbackEventExt::SetTimer {
            timer_id,
            duration_in_clocks,
        });
    }

    fn set_busy_end(&self, clocks: u32) {
        self.events
            .borrow_mut()
            .push(CallbackEventExt::SetBusyEnd { clocks });
    }

    fn is_busy(&self) -> bool {
        self.events.borrow_mut().push(CallbackEventExt::IsBusy);
        self.busy.get()
    }

    fn update_irq(&self, asserted: bool) {
        self.events
            .borrow_mut()
            .push(CallbackEventExt::UpdateIrq { asserted });
    }

    fn external_read(&self, access_class: YmfmAccessClass, address: u32) -> u8 {
        self.events
            .borrow_mut()
            .push(CallbackEventExt::ExternalRead {
                access_class,
                address,
            });
        let mem = self.adpcm_memory.borrow();
        mem.get(address as usize).copied().unwrap_or(0)
    }

    fn external_write(&self, access_class: YmfmAccessClass, address: u32, data: u8) {
        self.events
            .borrow_mut()
            .push(CallbackEventExt::ExternalWrite {
                access_class,
                address,
                data,
            });
        let mut mem = self.adpcm_memory.borrow_mut();
        if (address as usize) < mem.len() {
            mem[address as usize] = data;
        }
    }
}
