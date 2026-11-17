//! Minimal error type shared across the crate and its binaries. Deliberately
//! dependency-free: errors are strings with context, which is all a
//! single-purpose supervisor needs.

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<regex::Error> for Error {
    fn from(e: regex::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Convenience constructor: `err!("something failed: {}", x)`.
#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::Error(format!($($arg)*))
    };
}
