use device::i8251_keyboard::I8251Keyboard;

#[test]
fn keyboard_fifo_preserves_order_and_rxrdy() {
    let mut kb = I8251Keyboard::new();

    kb.push_scancode(0x1C);
    kb.push_scancode(0x9C);

    assert_eq!(kb.read_status() & 0x02, 0x02);

    let (first, clear_irq, retrigger_irq) = kb.read_data();
    assert_eq!(first, 0x1C);
    assert!(
        !clear_irq,
        "IRQ should stay pending while FIFO still has data"
    );
    assert!(retrigger_irq, "Second byte should retrigger IRQ after EOI");
    assert_eq!(kb.read_status() & 0x02, 0x02);

    let (second, clear_irq, retrigger_irq) = kb.read_data();
    assert_eq!(second, 0x9C);
    assert!(clear_irq, "IRQ should clear after final buffered byte");
    assert!(!retrigger_irq);
    assert_eq!(kb.read_status() & 0x02, 0x00);
}

#[test]
fn keyboard_fifo_drops_new_data_when_full() {
    let mut kb = I8251Keyboard::new();

    for code in 0u8..20 {
        kb.push_scancode(code);
    }

    let mut drained = Vec::new();
    while kb.has_rx_ready() {
        let (code, _, _) = kb.read_data();
        drained.push(code);
    }

    assert_eq!(drained.len(), 16);
    assert_eq!(drained[0], 0x00);
    assert_eq!(drained[15], 0x0F);
}
