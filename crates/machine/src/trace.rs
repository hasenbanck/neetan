//! Zero-cost tracing infrastructure for observing bus activity.

use common::ScheduledEvent;

/// Records bus activity (I/O, memory, IRQ, scheduler events).
///
/// All methods have empty default bodies so that [`NoTracing`] compiles
/// every call to nothing.
pub trait Tracing: Default {
    /// Update the current CPU cycle counter for timestamping trace output.
    fn set_cycle(&mut self, _cycle: u64) {}
    /// A scheduled event fired.
    fn trace_event(&mut self, _event: &ScheduledEvent) {}
    /// An I/O port was read.
    fn trace_io_read(&mut self, _port: u16, _value: u8) {}
    /// An I/O port was written.
    fn trace_io_write(&mut self, _port: u16, _value: u8) {}
    /// An unhandled I/O port read occurred.
    fn trace_io_unhandled_read(&mut self, _port: u16) {}
    /// An unhandled I/O port write occurred.
    fn trace_io_unhandled_write(&mut self, _port: u16, _value: u8) {}
    /// A byte was read from memory.
    fn trace_mem_read(&mut self, _address: u32, _value: u8) {}
    /// A byte was written to memory.
    fn trace_mem_write(&mut self, _address: u32, _value: u8) {}
    /// A word was read from memory.
    fn trace_mem_read_word(&mut self, _address: u32, _value: u16) {}
    /// A word was written to memory.
    fn trace_mem_write_word(&mut self, _address: u32, _value: u16) {}
    /// An IRQ line was raised.
    fn trace_irq_raise(&mut self, _irq: u8) {}
    /// An IRQ line was cleared.
    fn trace_irq_clear(&mut self, _irq: u8) {}
    /// An IRQ was acknowledged by the CPU.
    fn trace_irq_acknowledge(&mut self, _irq: u8, _vector: u8) {}
    /// The BIOS started to execute.
    fn trace_bios_start(&mut self) {}
    /// A BIOS HLE call was dispatched.
    fn trace_bios_hle(&mut self, _vector: u8, _ah: u8, _al: u8) {}
    /// An FDC seek command was executed.
    fn trace_fdc_seek(&mut self, _drive: usize, _cylinder: u8, _hd_us: u8) {}
    /// INT 1Bh FDD parameter trace.
    #[allow(clippy::too_many_arguments)]
    fn trace_int1bh_fdd_params(&mut self, _ah: u8, _al: u8, _cl: u8, _dh: u8, _dl: u8, _ch: u8) {}
    /// An FDC read data command was executed.
    fn trace_fdc_read(
        &mut self,
        _drive: usize,
        _track_index: usize,
        _c: u8,
        _h: u8,
        _r: u8,
        _n: u8,
    ) {
    }
    /// An INT 1Bh FDD read was dispatched via HLE.
    #[allow(clippy::too_many_arguments)]
    fn trace_int1bh_fdd_read(
        &mut self,
        _drive: usize,
        _c: u8,
        _h: u8,
        _r: u8,
        _n: u8,
        _sector_count: usize,
        _buf_addr: u32,
        _result: u8,
    ) {
    }
    /// An INT 1Bh FDD write was dispatched via HLE.
    #[allow(clippy::too_many_arguments)]
    fn trace_int1bh_fdd_write(
        &mut self,
        _drive: usize,
        _c: u8,
        _h: u8,
        _r: u8,
        _n: u8,
        _sector_count: usize,
        _buf_addr: u32,
        _result: u8,
    ) {
    }
    /// A SASI HLE operation was executed.
    #[allow(clippy::too_many_arguments)]
    fn trace_sasi_hle(
        &mut self,
        _function: u8,
        _drive: u8,
        _result: u8,
        _bx: u16,
        _cx: u16,
        _dx: u16,
        _es: u16,
        _bp: u16,
    ) {
    }
}

/// No-op tracer — all calls compile away.
#[derive(Default)]
pub struct NoTracing;

impl Tracing for NoTracing {}
