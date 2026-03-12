mod common;

use common::{callbacks::*, harness::*};
use ymfm_oxide::Ym3526;

#[test]
fn reset_triggers_timer_callbacks() {
    let mut chip = Ym3526::new(RecordingCallbacksOpl::new());
    chip.reset();

    let events = chip.callbacks().take_events();
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
    let mut chip = Ym3526::new(RecordingCallbacksOpl::new());
    chip.reset();
    chip.callbacks().take_events();

    chip.write_address(0x20);
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
fn status_register_reflects_irq_flag() {
    let mut chip = Ym3526::new(RecordingCallbacksOpl::new());
    chip.reset();

    // OPL status bit 7 = IRQ flag (OR of unmasked timer flags)
    let status_before = chip.read_status();
    assert_eq!(
        status_before & 0x80,
        0,
        "IRQ flag should be clear initially"
    );

    // Trigger Timer A to set IRQ
    write_reg_opl(&mut chip, 0x02, 0xFF);
    write_reg_opl(&mut chip, 0x04, 0x01);
    chip.timer_expired(0);

    let status_after = chip.read_status();
    assert_eq!(
        status_after & 0x80,
        0x80,
        "IRQ flag (bit 7) should be set when timer flag is active"
    );
}

#[test]
fn multiple_register_writes_each_trigger_busy() {
    let mut chip = Ym3526::new(RecordingCallbacksOpl::new());
    chip.reset();
    chip.callbacks().take_events();

    for addr in [0x20, 0x40, 0x60, 0x80, 0xE0] {
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
    let mut chip = Ym3526::new(RecordingCallbacksOpl::new());
    chip.reset();
    chip.callbacks().take_events();

    chip.write_address(0x02);
    chip.write_data(0xFF);
    chip.write_address(0x04);
    chip.write_data(0x01);

    let events = chip.callbacks().take_events();

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
    let mut chip = Ym3526::new(RecordingCallbacksOpl::new());
    chip.reset();

    setup_opl_simple_tone(&mut chip, 0, 1, 0);
    key_on_opl(&mut chip, 0);

    let samples = generate_1_opl(&mut chip, 256);
    assert_eq!(samples.len(), 256);

    let has_nonzero = samples.iter().any(|s| s[0] != 0);
    assert!(has_nonzero, "FM output should be non-zero after key-on");
}
