use sdl3_sys::scancode::SDL_Scancode;

/// A physical keyboard key.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Scancode {
    /// Escape key.
    Escape,
    /// Number row 1.
    _1,
    /// Number row 2.
    _2,
    /// Number row 3.
    _3,
    /// Number row 4.
    _4,
    /// Number row 5.
    _5,
    /// Number row 6.
    _6,
    /// Number row 7.
    _7,
    /// Number row 8.
    _8,
    /// Number row 9.
    _9,
    /// Number row 0.
    _0,
    /// Minus / hyphen key.
    Minus,
    /// Equals key.
    Equals,
    /// Backslash key.
    Backslash,
    /// Backspace key.
    Backspace,
    /// Tab key.
    Tab,
    /// Q key.
    Q,
    /// W key.
    W,
    /// E key.
    E,
    /// R key.
    R,
    /// T key.
    T,
    /// Y key.
    Y,
    /// U key.
    U,
    /// I key.
    I,
    /// O key.
    O,
    /// P key.
    P,
    /// Return / Enter key.
    Return,
    /// Left bracket key.
    LeftBracket,
    /// Right bracket key.
    RightBracket,
    /// Grave accent / tilde key.
    Grave,
    /// Semicolon key.
    Semicolon,
    /// Apostrophe key.
    Apostrophe,
    /// Non-US backslash key.
    NonUsBackslash,
    /// A key.
    A,
    /// S key.
    S,
    /// D key.
    D,
    /// F key.
    F,
    /// G key.
    G,
    /// H key.
    H,
    /// J key.
    J,
    /// K key.
    K,
    /// L key.
    L,
    /// Z key.
    Z,
    /// X key.
    X,
    /// C key.
    C,
    /// V key.
    V,
    /// B key.
    B,
    /// N key.
    N,
    /// M key.
    M,
    /// Comma key.
    Comma,
    /// Period key.
    Period,
    /// Slash key.
    Slash,
    /// Space bar.
    Space,
    /// Page Down key.
    PageDown,
    /// Page Up key.
    PageUp,
    /// Insert key.
    Insert,
    /// Delete key.
    Delete,
    /// Up arrow key.
    Up,
    /// Left arrow key.
    Left,
    /// Right arrow key.
    Right,
    /// Down arrow key.
    Down,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Keypad minus.
    KpMinus,
    /// Keypad divide.
    KpDivide,
    /// Keypad 0.
    Kp0,
    /// Keypad 1.
    Kp1,
    /// Keypad 2.
    Kp2,
    /// Keypad 3.
    Kp3,
    /// Keypad 4.
    Kp4,
    /// Keypad 5.
    Kp5,
    /// Keypad 6.
    Kp6,
    /// Keypad 7.
    Kp7,
    /// Keypad 8.
    Kp8,
    /// Keypad 9.
    Kp9,
    /// Keypad multiply.
    KpMultiply,
    /// Keypad plus.
    KpPlus,
    /// Keypad enter.
    KpEnter,
    /// Keypad comma.
    KpComma,
    /// Keypad period.
    KpPeriod,
    /// F1 key.
    F1,
    /// F2 key.
    F2,
    /// F3 key.
    F3,
    /// F4 key.
    F4,
    /// F5 key.
    F5,
    /// F6 key.
    F6,
    /// F7 key.
    F7,
    /// F8 key.
    F8,
    /// F9 key.
    F9,
    /// F10 key.
    F10,
    /// Left Shift key.
    LShift,
    /// Right Shift key.
    RShift,
    /// Caps Lock key.
    CapsLock,
    /// Left Alt key.
    LAlt,
    /// Right Alt key.
    RAlt,
    /// Left Ctrl key.
    LCtrl,
    /// Right Ctrl key.
    RCtrl,
}

impl Scancode {
    /// Converts a raw SDL3 scancode to a `Scancode`, returning `None` for unmapped keys.
    pub fn from_raw(raw: SDL_Scancode) -> Option<Self> {
        Some(match raw {
            SDL_Scancode::ESCAPE => Self::Escape,
            SDL_Scancode::_1 => Self::_1,
            SDL_Scancode::_2 => Self::_2,
            SDL_Scancode::_3 => Self::_3,
            SDL_Scancode::_4 => Self::_4,
            SDL_Scancode::_5 => Self::_5,
            SDL_Scancode::_6 => Self::_6,
            SDL_Scancode::_7 => Self::_7,
            SDL_Scancode::_8 => Self::_8,
            SDL_Scancode::_9 => Self::_9,
            SDL_Scancode::_0 => Self::_0,
            SDL_Scancode::MINUS => Self::Minus,
            SDL_Scancode::EQUALS => Self::Equals,
            SDL_Scancode::BACKSLASH => Self::Backslash,
            SDL_Scancode::BACKSPACE => Self::Backspace,
            SDL_Scancode::TAB => Self::Tab,
            SDL_Scancode::Q => Self::Q,
            SDL_Scancode::W => Self::W,
            SDL_Scancode::E => Self::E,
            SDL_Scancode::R => Self::R,
            SDL_Scancode::T => Self::T,
            SDL_Scancode::Y => Self::Y,
            SDL_Scancode::U => Self::U,
            SDL_Scancode::I => Self::I,
            SDL_Scancode::O => Self::O,
            SDL_Scancode::P => Self::P,
            SDL_Scancode::RETURN => Self::Return,
            SDL_Scancode::LEFTBRACKET => Self::LeftBracket,
            SDL_Scancode::RIGHTBRACKET => Self::RightBracket,
            SDL_Scancode::GRAVE => Self::Grave,
            SDL_Scancode::SEMICOLON => Self::Semicolon,
            SDL_Scancode::APOSTROPHE => Self::Apostrophe,
            SDL_Scancode::NONUSBACKSLASH => Self::NonUsBackslash,
            SDL_Scancode::A => Self::A,
            SDL_Scancode::S => Self::S,
            SDL_Scancode::D => Self::D,
            SDL_Scancode::F => Self::F,
            SDL_Scancode::G => Self::G,
            SDL_Scancode::H => Self::H,
            SDL_Scancode::J => Self::J,
            SDL_Scancode::K => Self::K,
            SDL_Scancode::L => Self::L,
            SDL_Scancode::Z => Self::Z,
            SDL_Scancode::X => Self::X,
            SDL_Scancode::C => Self::C,
            SDL_Scancode::V => Self::V,
            SDL_Scancode::B => Self::B,
            SDL_Scancode::N => Self::N,
            SDL_Scancode::M => Self::M,
            SDL_Scancode::COMMA => Self::Comma,
            SDL_Scancode::PERIOD => Self::Period,
            SDL_Scancode::SLASH => Self::Slash,
            SDL_Scancode::SPACE => Self::Space,
            SDL_Scancode::PAGEDOWN => Self::PageDown,
            SDL_Scancode::PAGEUP => Self::PageUp,
            SDL_Scancode::INSERT => Self::Insert,
            SDL_Scancode::DELETE => Self::Delete,
            SDL_Scancode::UP => Self::Up,
            SDL_Scancode::LEFT => Self::Left,
            SDL_Scancode::RIGHT => Self::Right,
            SDL_Scancode::DOWN => Self::Down,
            SDL_Scancode::HOME => Self::Home,
            SDL_Scancode::END => Self::End,
            SDL_Scancode::KP_MINUS => Self::KpMinus,
            SDL_Scancode::KP_DIVIDE => Self::KpDivide,
            SDL_Scancode::KP_0 => Self::Kp0,
            SDL_Scancode::KP_1 => Self::Kp1,
            SDL_Scancode::KP_2 => Self::Kp2,
            SDL_Scancode::KP_3 => Self::Kp3,
            SDL_Scancode::KP_4 => Self::Kp4,
            SDL_Scancode::KP_5 => Self::Kp5,
            SDL_Scancode::KP_6 => Self::Kp6,
            SDL_Scancode::KP_7 => Self::Kp7,
            SDL_Scancode::KP_8 => Self::Kp8,
            SDL_Scancode::KP_9 => Self::Kp9,
            SDL_Scancode::KP_MULTIPLY => Self::KpMultiply,
            SDL_Scancode::KP_PLUS => Self::KpPlus,
            SDL_Scancode::KP_ENTER => Self::KpEnter,
            SDL_Scancode::KP_COMMA => Self::KpComma,
            SDL_Scancode::KP_PERIOD => Self::KpPeriod,
            SDL_Scancode::F1 => Self::F1,
            SDL_Scancode::F2 => Self::F2,
            SDL_Scancode::F3 => Self::F3,
            SDL_Scancode::F4 => Self::F4,
            SDL_Scancode::F5 => Self::F5,
            SDL_Scancode::F6 => Self::F6,
            SDL_Scancode::F7 => Self::F7,
            SDL_Scancode::F8 => Self::F8,
            SDL_Scancode::F9 => Self::F9,
            SDL_Scancode::F10 => Self::F10,
            SDL_Scancode::LSHIFT => Self::LShift,
            SDL_Scancode::RSHIFT => Self::RShift,
            SDL_Scancode::CAPSLOCK => Self::CapsLock,
            SDL_Scancode::LALT => Self::LAlt,
            SDL_Scancode::RALT => Self::RAlt,
            SDL_Scancode::LCTRL => Self::LCtrl,
            SDL_Scancode::RCTRL => Self::RCtrl,
            _ => return None,
        })
    }
}

/// Keyboard modifier flags (Shift, Ctrl, Alt, etc.).
pub struct Mod(pub u16);

impl Mod {
    /// Returns `true` if either GUI (Super/Command) key is held.
    pub fn gui(&self) -> bool {
        self.0 & 0x0C00 != 0
    }
}
