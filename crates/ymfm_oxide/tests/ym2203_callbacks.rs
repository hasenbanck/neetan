mod common;

use common::{callbacks::*, harness::*};
use ymfm_oxide::Ym2203;

#[test]
fn reset_triggers_timer_callbacks() {
    let mut chip = Ym2203::new(RecordingCallbacks2203::new());
    chip.reset();

    let events = chip.callbacks().take_events();
    // Reset should produce set_timer calls (cancelling both timers)
    let timer_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, CallbackEvent::SetTimer { .. }))
        .collect();
    assert!(
        !timer_events.is_empty(),
        "reset should trigger set_timer callbacks"
    );
}

#[test]
fn set_busy_end_on_register_write() {
    let mut chip = Ym2203::new(RecordingCallbacks2203::new());
    chip.reset();
    chip.callbacks().take_events(); // Clear reset events

    // Write a register
    chip.write_address(0x28);
    chip.write_data(0x00);

    let events = chip.callbacks().take_events();
    let busy_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, CallbackEvent::SetBusyEnd { .. }))
        .collect();
    assert!(
        !busy_events.is_empty(),
        "register write should trigger set_busy_end"
    );
}

#[test]
fn is_busy_propagates_to_status() {
    let mut chip = Ym2203::new(RecordingCallbacks2203::new());
    chip.reset();
    chip.callbacks().take_events();

    // When not busy, bit 7 should be clear
    chip.callbacks().busy.set(false);
    let status = chip.read_status();
    assert_eq!(status & 0x80, 0, "busy bit should be clear when not busy");

    // When busy, bit 7 should be set
    chip.callbacks().busy.set(true);
    let status = chip.read_status();
    assert_eq!(status & 0x80, 0x80, "busy bit should be set when busy");
}

#[test]
fn multiple_register_writes_each_trigger_busy() {
    let mut chip = Ym2203::new(RecordingCallbacks2203::new());
    chip.reset();
    chip.callbacks().take_events();

    // Write 5 different registers
    for addr in [0x30, 0x40, 0x50, 0x60, 0x70] {
        chip.write_address(addr);
        chip.write_data(0x00);
    }

    let events = chip.callbacks().take_events();
    let busy_count = events
        .iter()
        .filter(|e| matches!(e, CallbackEvent::SetBusyEnd { .. }))
        .count();
    assert!(
        busy_count >= 5,
        "each register write should trigger set_busy_end, got {busy_count}"
    );
}

#[test]
fn callback_event_ordering() {
    let mut chip = Ym2203::new(RecordingCallbacks2203::new());
    chip.reset();
    chip.callbacks().take_events();

    // Enable Timer A
    chip.write_address(0x24);
    chip.write_data(0xFF); // Timer A high
    chip.write_address(0x25);
    chip.write_data(0x03); // Timer A low
    chip.write_address(0x27);
    chip.write_data(0x05); // Enable + load Timer A

    let events = chip.callbacks().take_events();

    // Should have busy events (from writes) and eventually a set_timer event
    let has_busy = events
        .iter()
        .any(|e| matches!(e, CallbackEvent::SetBusyEnd { .. }));
    let has_timer = events
        .iter()
        .any(|e| matches!(e, CallbackEvent::SetTimer { .. }));

    assert!(has_busy, "should have busy events from register writes");
    assert!(
        has_timer,
        "should have set_timer event from Timer A configuration"
    );
}

#[test]
fn generate_does_not_crash_with_recording_callbacks() {
    let mut chip = Ym2203::new(RecordingCallbacks2203::new());
    chip.reset();

    setup_ym2203_simple_tone(&mut chip, 0, 7, 0);
    key_on_2203(&mut chip, 0);

    let samples = generate_4(&mut chip, 256);
    assert_eq!(samples.len(), 256);

    // Should have non-zero FM output
    let has_nonzero_fm = samples.iter().any(|s| s[0] != 0);
    assert!(has_nonzero_fm, "FM output should be non-zero after key-on");
}
