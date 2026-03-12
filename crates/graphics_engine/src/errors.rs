/// Crate-wide error type.
pub enum Error {
    /// An error with context.
    Context(common::ContextError),
    /// An error with a message.
    Message(common::StringError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(e) => std::fmt::Display::fmt(e, f),
            Self::Message(e) => std::fmt::Display::fmt(e, f),
        }
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(e) => std::fmt::Debug::fmt(e, f),
            Self::Message(e) => std::fmt::Debug::fmt(e, f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(e) => e.source(),
            Self::Message(e) => e.source(),
        }
    }
}

impl From<common::ContextError> for Error {
    fn from(e: common::ContextError) -> Self {
        Self::Context(e)
    }
}

impl From<common::StringError> for Error {
    fn from(e: common::StringError) -> Self {
        Self::Message(e)
    }
}
