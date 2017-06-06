pub use std::error::Error as StdError;
use std::fmt;

pub use self::Error::{BlockError, InternalError};

/// Result type returned from functions that can have our `Error`s.
pub type Result<T> = ::std::result::Result<T, Error>;

pub trait ResultExt<T, E> {
    fn block_error(self, block: &str, message: &str) -> Result<T>;
    fn internal_error(self, context: &str, message: &str) -> Result<T>;
}

impl<T, E> ResultExt<T, E> for ::std::result::Result<T, E> {
    fn block_error(self, block: &str, message: &str) -> Result<T> {
        self.map_err(|_| BlockError(block.to_owned(), message.to_owned()))
    }

    fn internal_error(self, context: &str, message: &str) -> Result<T> {
        self.map_err(|_| InternalError(context.to_owned(), message.to_owned()))
    }
}

pub trait OptionExt<T> {
    fn block_error(self, block: &str, message: &str) -> Result<T>;
    fn internal_error(self, context: &str, message: &str) -> Result<T>;
}

impl<T> OptionExt<T> for ::std::option::Option<T>
{
    fn block_error(self, block: &str, message: &str) -> Result<T> {
        self.ok_or_else(|| BlockError(block.to_owned(), message.to_owned()))
    }

    fn internal_error(self, context: &str, message: &str) -> Result<T> {
        self.ok_or_else(|| InternalError(context.to_owned(), message.to_owned()))
    }
}

/// A set of errors that can occur during the runtime of i3status-rs.
#[derive(Debug)]
pub enum Error {
    BlockError(String, String),
    InternalError(String, String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BlockError(ref block, ref message) => f.write_str(&format!("Error in block '{}': {}", block, message)),
            InternalError(ref context, ref message) => f.write_str(&format!("Internal error in context '{}': {}", context, message)),
        }
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            BlockError(_, _) => "Block error occured",
            InternalError(_, _) => "Internal error occured",
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match *self {
            _ => None,
        }
    }
}

impl<T> From<::std::sync::mpsc::SendError<T>> for Error
where
    T: fmt::Display
{
    fn from(err: ::std::sync::mpsc::SendError<T>) -> Error {
        InternalError("unknown".to_owned(), format!("send error for '{}'", err.0))
    }
}
