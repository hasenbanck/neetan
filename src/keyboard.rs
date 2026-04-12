use common::Machine;
use sdl3::keyboard::Scancode;

pub fn pc98_scancode_from_name(name: &str) -> Option<u8> {
    let name_lower = name.to_ascii_lowercase();
    Some(match name_lower.as_str() {
        "esc" => 0x00,
        "1" => 0x01,
        "2" => 0x02,
        "3" => 0x03,
        "4" => 0x04,
        "5" => 0x05,
        "6" => 0x06,
        "7" => 0x07,
        "8" => 0x08,
        "9" => 0x09,
        "0" => 0x0A,
        "minus" => 0x0B,
        "caret" => 0x0C,
        "yen" => 0x0D,
        "bs" => 0x0E,
        "tab" => 0x0F,
        "q" => 0x10,
        "w" => 0x11,
        "e" => 0x12,
        "r" => 0x13,
        "t" => 0x14,
        "y" => 0x15,
        "u" => 0x16,
        "i" => 0x17,
        "o" => 0x18,
        "p" => 0x19,
        "at" => 0x1A,
        "leftbracket" => 0x1B,
        "return" => 0x1C,
        "a" => 0x1D,
        "s" => 0x1E,
        "d" => 0x1F,
        "f" => 0x20,
        "g" => 0x21,
        "h" => 0x22,
        "j" => 0x23,
        "k" => 0x24,
        "l" => 0x25,
        "semicolon" => 0x26,
        "colon" => 0x27,
        "rightbracket" => 0x28,
        "z" => 0x29,
        "x" => 0x2A,
        "c" => 0x2B,
        "v" => 0x2C,
        "b" => 0x2D,
        "n" => 0x2E,
        "m" => 0x2F,
        "comma" => 0x30,
        "period" => 0x31,
        "slash" => 0x32,
        "underscore" => 0x33,
        "space" => 0x34,
        "xfer" => 0x35,
        "rollup" => 0x36,
        "rolldown" => 0x37,
        "ins" => 0x38,
        "del" => 0x39,
        "up" => 0x3A,
        "left" => 0x3B,
        "right" => 0x3C,
        "down" => 0x3D,
        "home" => 0x3E,
        "help" => 0x3F,
        "kpminus" => 0x40,
        "kpdivide" => 0x41,
        "kp7" => 0x42,
        "kp8" => 0x43,
        "kp9" => 0x44,
        "kpmultiply" => 0x45,
        "kp4" => 0x46,
        "kp5" => 0x47,
        "kp6" => 0x48,
        "kpplus" => 0x49,
        "kp1" => 0x4A,
        "kp2" => 0x4B,
        "kp3" => 0x4C,
        "kpequals" => 0x4D,
        "kp0" => 0x4E,
        "kpcomma" => 0x4F,
        "kpperiod" => 0x50,
        "nfer" => 0x51,
        "vf1" => 0x52,
        "vf2" => 0x53,
        "vf3" => 0x54,
        "vf4" => 0x55,
        "vf5" => 0x56,
        "stop" => 0x60,
        "copy" => 0x61,
        "f1" => 0x62,
        "f2" => 0x63,
        "f3" => 0x64,
        "f4" => 0x65,
        "f5" => 0x66,
        "f6" => 0x67,
        "f7" => 0x68,
        "f8" => 0x69,
        "f9" => 0x6A,
        "f10" => 0x6B,
        "shift" => 0x70,
        "caps" => 0x71,
        "kana" => 0x72,
        "grph" => 0x73,
        "ctrl" => 0x74,
        _ => return None,
    })
}

pub struct KeyMap {
    mappings: [u8; Scancode::COUNT],
}

impl KeyMap {
    pub const fn new() -> Self {
        Self {
            mappings: build_default_map(),
        }
    }

    pub fn set(&mut self, host: Scancode, pc98_code: u8) {
        self.mappings[host.index()] = pc98_code;
    }

    pub fn lookup(&self, host: Scancode) -> u8 {
        self.mappings[host.index()]
    }
}

pub(crate) struct KeyboardForwardingState {
    gui_modifier_active: bool,
    guest_pressed_pc98_scancodes: [Option<u8>; Scancode::COUNT],
    pending_pressed_pc98_scancode: Option<u8>,
    pending_released_pc98_scancodes: Vec<u8>,
}

impl KeyboardForwardingState {
    pub(crate) fn new() -> Self {
        Self {
            gui_modifier_active: false,
            guest_pressed_pc98_scancodes: [None; Scancode::COUNT],
            pending_pressed_pc98_scancode: None,
            pending_released_pc98_scancodes: Vec::with_capacity(Scancode::COUNT),
        }
    }

    pub(crate) fn handle_key_down(
        &mut self,
        scancode: Option<Scancode>,
        gui_modifier_active: bool,
        repeat: bool,
        key_map: &KeyMap,
    ) {
        self.clear_pending_actions();

        if repeat {
            return;
        }

        if gui_modifier_active && !self.gui_modifier_active {
            self.release_all_guest_keys();
        }
        self.gui_modifier_active = gui_modifier_active;

        if self.gui_modifier_active {
            return;
        }

        let Some(scancode) = scancode else {
            return;
        };

        let scancode_index = scancode.index();
        if self.guest_pressed_pc98_scancodes[scancode_index].is_some() {
            return;
        }

        let pc98_scancode = key_map.lookup(scancode);
        self.guest_pressed_pc98_scancodes[scancode_index] = Some(pc98_scancode);
        self.pending_pressed_pc98_scancode = Some(pc98_scancode);
    }

    pub(crate) fn handle_key_up(
        &mut self,
        scancode: Option<Scancode>,
        repeat: bool,
        key_map: &KeyMap,
    ) -> Option<u8> {
        if repeat {
            return None;
        }

        let scancode = scancode?;
        let scancode_index = scancode.index();
        let pc98_scancode = self.guest_pressed_pc98_scancodes[scancode_index]?;
        self.guest_pressed_pc98_scancodes[scancode_index] = None;

        let expected_pc98_scancode = key_map.lookup(scancode);
        debug_assert_eq!(pc98_scancode, expected_pc98_scancode);

        Some(pc98_scancode | 0x80)
    }

    fn release_all_guest_keys(&mut self) {
        for guest_pressed_pc98_scancode in &mut self.guest_pressed_pc98_scancodes {
            if let Some(pc98_scancode) = guest_pressed_pc98_scancode.take() {
                self.pending_released_pc98_scancodes
                    .push(pc98_scancode | 0x80);
            }
        }
    }

    pub(crate) fn apply_pending_actions(&mut self, machine: &mut dyn Machine) {
        for &released_pc98_scancode in &self.pending_released_pc98_scancodes {
            machine.push_keyboard_scancode(released_pc98_scancode);
        }

        if let Some(pressed_pc98_scancode) = self.pending_pressed_pc98_scancode {
            machine.push_keyboard_scancode(pressed_pc98_scancode);
        }

        self.clear_pending_actions();
    }

    #[cfg(test)]
    fn pending_pressed_pc98_scancode(&self) -> Option<u8> {
        self.pending_pressed_pc98_scancode
    }

    #[cfg(test)]
    fn pending_released_pc98_scancodes(&self) -> &[u8] {
        &self.pending_released_pc98_scancodes
    }

    fn clear_pending_actions(&mut self) {
        self.pending_pressed_pc98_scancode = None;
        self.pending_released_pc98_scancodes.clear();
    }
}

#[allow(clippy::just_underscores_and_digits)]
const fn build_default_map() -> [u8; Scancode::COUNT] {
    use Scancode::*;

    const ALL_SCANCODES: &[(Scancode, u8)] = &[
        (Escape, 0x00),
        (_1, 0x01),
        (_2, 0x02),
        (_3, 0x03),
        (_4, 0x04),
        (_5, 0x05),
        (_6, 0x06),
        (_7, 0x07),
        (_8, 0x08),
        (_9, 0x09),
        (_0, 0x0A),
        (Minus, 0x0B),
        (Equals, 0x0C),
        (Backslash, 0x0D),
        (Backspace, 0x0E),
        (Tab, 0x0F),
        (Q, 0x10),
        (W, 0x11),
        (E, 0x12),
        (R, 0x13),
        (T, 0x14),
        (Y, 0x15),
        (U, 0x16),
        (I, 0x17),
        (O, 0x18),
        (P, 0x19),
        (Grave, 0x1A),
        (LeftBracket, 0x1B),
        (Return, 0x1C),
        (A, 0x1D),
        (S, 0x1E),
        (D, 0x1F),
        (F, 0x20),
        (G, 0x21),
        (H, 0x22),
        (J, 0x23),
        (K, 0x24),
        (L, 0x25),
        (Semicolon, 0x26),
        (Apostrophe, 0x27),
        (RightBracket, 0x28),
        (Z, 0x29),
        (X, 0x2A),
        (C, 0x2B),
        (V, 0x2C),
        (B, 0x2D),
        (N, 0x2E),
        (M, 0x2F),
        (Comma, 0x30),
        (Period, 0x31),
        (Slash, 0x32),
        (NonUsBackslash, 0x33),
        (Space, 0x34),
        (RAlt, 0x35),
        (PageUp, 0x36),
        (PageDown, 0x37),
        (Insert, 0x38),
        (Delete, 0x39),
        (Up, 0x3A),
        (Left, 0x3B),
        (Right, 0x3C),
        (Down, 0x3D),
        (Home, 0x3E),
        (End, 0x3F),
        (KpMinus, 0x40),
        (KpDivide, 0x41),
        (Kp7, 0x42),
        (Kp8, 0x43),
        (Kp9, 0x44),
        (KpMultiply, 0x45),
        (Kp4, 0x46),
        (Kp5, 0x47),
        (Kp6, 0x48),
        (KpPlus, 0x49),
        (Kp1, 0x4A),
        (Kp2, 0x4B),
        (Kp3, 0x4C),
        (KpEnter, 0x4D),
        (Kp0, 0x4E),
        (KpComma, 0x4F),
        (KpPeriod, 0x50),
        (Application, 0x51),
        (F11, 0x52),
        (F12, 0x53),
        (F13, 0x54),
        (F14, 0x55),
        (F15, 0x56),
        (Pause, 0x60),
        (PrintScreen, 0x61),
        (F1, 0x62),
        (F2, 0x63),
        (F3, 0x64),
        (F4, 0x65),
        (F5, 0x66),
        (F6, 0x67),
        (F7, 0x68),
        (F8, 0x69),
        (F9, 0x6A),
        (F10, 0x6B),
        (LShift, 0x70),
        (RShift, 0x70),
        (CapsLock, 0x71),
        (NumLock, 0x72),
        (LAlt, 0x73),
        (LCtrl, 0x74),
        (RCtrl, 0x74),
    ];

    let mut map = [0u8; Scancode::COUNT];
    let mut i = 0;
    while i < ALL_SCANCODES.len() {
        let (scancode, pc98) = ALL_SCANCODES[i];
        map[scancode.index()] = pc98;
        i += 1;
    }
    map
}

pub fn parse_key_binding(host_name: &str, pc98_name: &str) -> Option<(Scancode, u8)> {
    let host = Scancode::from_name(host_name)?;
    let pc98 = pc98_scancode_from_name(pc98_name)?;
    Some((host, pc98))
}

#[cfg(test)]
mod tests {
    use sdl3::keyboard::Scancode;

    use super::{KeyMap, KeyboardForwardingState};

    #[test]
    fn normal_left_alt_is_forwarded_to_the_guest() {
        let mut keyboard_forwarding_state = KeyboardForwardingState::new();
        let key_map = KeyMap::new();

        keyboard_forwarding_state.handle_key_down(Some(Scancode::LAlt), false, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            Some(0x73)
        );
        assert!(
            keyboard_forwarding_state
                .pending_released_pc98_scancodes()
                .is_empty()
        );

        let key_up_scancode =
            keyboard_forwarding_state.handle_key_up(Some(Scancode::LAlt), false, &key_map);
        assert_eq!(key_up_scancode, Some(0xF3));
    }

    #[test]
    fn gui_combo_does_not_forward_left_alt_or_function_keys() {
        let mut keyboard_forwarding_state = KeyboardForwardingState::new();
        let key_map = KeyMap::new();

        keyboard_forwarding_state.handle_key_down(None, true, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            None
        );
        assert!(
            keyboard_forwarding_state
                .pending_released_pc98_scancodes()
                .is_empty()
        );

        keyboard_forwarding_state.handle_key_down(Some(Scancode::LAlt), true, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            None
        );
        assert!(
            keyboard_forwarding_state
                .pending_released_pc98_scancodes()
                .is_empty()
        );

        keyboard_forwarding_state.handle_key_down(Some(Scancode::F9), true, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            None
        );
        assert!(
            keyboard_forwarding_state
                .pending_released_pc98_scancodes()
                .is_empty()
        );

        let function_key_up_scancode =
            keyboard_forwarding_state.handle_key_up(Some(Scancode::F9), false, &key_map);
        assert_eq!(function_key_up_scancode, None);

        let left_alt_key_up_scancode =
            keyboard_forwarding_state.handle_key_up(Some(Scancode::LAlt), false, &key_map);
        assert_eq!(left_alt_key_up_scancode, None);
    }

    #[test]
    fn gui_activation_releases_guest_keys_that_were_already_held() {
        let mut keyboard_forwarding_state = KeyboardForwardingState::new();
        let key_map = KeyMap::new();

        keyboard_forwarding_state.handle_key_down(Some(Scancode::LAlt), false, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            Some(0x73)
        );

        keyboard_forwarding_state.handle_key_down(None, true, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            None
        );
        assert_eq!(
            keyboard_forwarding_state.pending_released_pc98_scancodes(),
            [0xF3]
        );

        let left_alt_key_up_scancode =
            keyboard_forwarding_state.handle_key_up(Some(Scancode::LAlt), false, &key_map);
        assert_eq!(left_alt_key_up_scancode, None);
    }

    #[test]
    fn forwarding_recovers_after_gui_is_released() {
        let mut keyboard_forwarding_state = KeyboardForwardingState::new();
        let key_map = KeyMap::new();

        keyboard_forwarding_state.handle_key_down(None, true, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            None
        );
        assert!(
            keyboard_forwarding_state
                .pending_released_pc98_scancodes()
                .is_empty()
        );

        keyboard_forwarding_state.handle_key_down(Some(Scancode::LAlt), true, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            None
        );
        assert!(
            keyboard_forwarding_state
                .pending_released_pc98_scancodes()
                .is_empty()
        );

        keyboard_forwarding_state.handle_key_down(Some(Scancode::A), false, false, &key_map);
        assert_eq!(
            keyboard_forwarding_state.pending_pressed_pc98_scancode(),
            Some(0x1D)
        );
        assert!(
            keyboard_forwarding_state
                .pending_released_pc98_scancodes()
                .is_empty()
        );
    }
}
