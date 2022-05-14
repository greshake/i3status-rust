//! Error type and extension traits for `Result`/`Option`.
//!
//! A note for block authors:
//! - Just `use crate::errors::*;`
//! - In your code, use `.error_msg(<msg>)?` when you want to propagate an error from an external
//!   library or you just want to add more context to the error. Similar to `anyhow`'s
//!   `.context()`.
//! - Use `.map_error_msg(|_| format!(<msg>))` if you want to include additional info in your
//!   context message. Similar to `anyhow`'s `.with_context()`.
//!
//! Perhaps it's better to rename `error_msg` and `map_error_msg` to `context` and `with_context`.

use std::borrow::Cow;
use std::fmt;

pub type ErrMsg = Cow<'static, str>;

pub trait ErrBounds: fmt::Debug + fmt::Display + Send + Sync + 'static {}
impl<T: fmt::Debug + fmt::Display + Send + Sync + 'static> ErrBounds for T {}

/// A set of errors that can occur during the runtime of i3status-rs.
#[derive(Debug)]
pub enum Error {
    /// Error that occurred in a block.
    InBlock(&'static str, Box<Self>),
    /// A wrapped error with a context message.
    Wrapped(ErrMsg, Box<dyn ErrBounds>),
    /// Simple text error message.
    Message(ErrMsg),
    /// Errors from `curl`. Used in weather block.
    Curl(curl::Error),
}

/// Result type returned from functions that can have our `Error`s.
pub type Result<T, E = Error> = std::result::Result<T, E>;

pub trait ResultExt<T, E> {
    /// Wrap the error with a context message.
    fn error_msg<M: Into<ErrMsg>>(self, msg: M) -> Result<T>;

    /// Same as `error_msg` but accepts a closure.
    fn map_error_msg<M: Into<ErrMsg>, F: FnOnce(&E) -> M>(self, f: F) -> Result<T>;
}

impl<T, E: ErrBounds> ResultExt<T, E> for Result<T, E> {
    fn error_msg<M: Into<ErrMsg>>(self, msg: M) -> Result<T> {
        self.map_err(|e| Error::Wrapped(msg.into(), Box::new(e)))
    }

    fn map_error_msg<M: Into<ErrMsg>, F: FnOnce(&E) -> M>(self, f: F) -> Result<T> {
        self.map_err(|e| Error::Wrapped(f(&e).into(), Box::new(e)))
    }
}

pub trait OptionExt<T> {
    /// Convert an `Option` to `Result` with a given message if `None`.
    fn error_msg<M: Into<ErrMsg>>(self, msg: M) -> Result<T>;

    /// Same as `error_msg` but accepts a closure.
    fn map_error_msg<M: Into<ErrMsg>, F: FnOnce() -> M>(self, f: F) -> Result<T>;
}

impl<T> OptionExt<T> for ::std::option::Option<T> {
    fn error_msg<M: Into<ErrMsg>>(self, msg: M) -> Result<T> {
        self.ok_or_else(|| Error::Message(msg.into()))
    }

    fn map_error_msg<M: Into<ErrMsg>, F: FnOnce() -> M>(self, f: F) -> Result<T> {
        self.ok_or_else(|| Error::Message(f().into()))
    }
}

impl Error {
    /// Create a new error with a given message.
    pub fn new<M: Into<ErrMsg>>(msg: M) -> Self {
        Self::Message(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InBlock(block, error) => {
                write!(f, "in block '{block}': {error}")
            }
            Self::Wrapped(msg, inner) => {
                write!(f, "{msg} (Cause: {inner})")
            }
            Self::Message(msg) => {
                write!(f, "{msg}")
            }
            Self::Curl(curl) => {
                write!(f, "curl: {curl}")
            }
        }
    }
}

impl std::error::Error for Error {}

pub trait ResultSpec<T> {
    /// Notify that an error occured in a given block.
    fn in_block(self, block: &'static str) -> Result<T>;
}

impl<T> ResultSpec<T> for Result<T> {
    fn in_block(self, block: &'static str) -> Result<T> {
        self.map_err(|e| Error::InBlock(block, Box::new(e)))
    }
}
