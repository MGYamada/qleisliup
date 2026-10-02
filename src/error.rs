use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub(crate) struct Error {
    pub(crate) status: u8,
    message: String,
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn operational(message: impl Into<String>) -> Self {
        Self {
            status: 1,
            message: message.into(),
        }
    }

    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self {
            status: 2,
            message: message.into(),
        }
    }

    pub(crate) fn file(path: &Path, reason: impl fmt::Display) -> Self {
        Self::operational(format!("{}: {reason}", path.display()))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for Error {}
