pub use std::error::Error as StdError;
use std::fmt;

pub use self::Error::{BlockError, ConfigurationError, InternalError};

/// Result type returned from functions that can have our `Error`s.
pub type Result<T> = ::std::result::Result<T, Error>;

pub trait ResultExtBlock<T, E> {
    fn block_error(self, block: &str, message: &str) -> Result<T>;
}

pub trait ResultExtInternal<T, E> {
    fn configuration_error(self, message: &str) -> Result<T>;
    fn internal_error(self, context: &str, message: &str) -> Result<T>;
}

impl<T, E> ResultExtBlock<T, E> for ::std::result::Result<T, E> {
    fn block_error(self, block: &str, message: &str) -> Result<T> {
        self.map_err(|_| BlockError(block.to_owned(), message.to_owned()))
    }
}

impl<T, E> ResultExtInternal<T, E> for ::std::result::Result<T, E>
where
    E: fmt::Display + fmt::Debug,
{
    fn configuration_error(self, message: &str) -> Result<T> {
        self.map_err(|e| {
            ConfigurationError(message.to_owned(), (format!("{}", e), format!("{:?}", e)))
        })
    }

    fn internal_error(self, context: &str, message: &str) -> Result<T> {
        self.map_err(|e| {
            InternalError(
                context.to_owned(),
                message.to_owned(),
                Some((format!("{}", e), format!("{:?}", e))),
            )
        })
    }
}

pub trait OptionExt<T> {
    fn block_error(self, block: &str, message: &str) -> Result<T>;
    fn internal_error(self, context: &str, message: &str) -> Result<T>;
}

impl<T> OptionExt<T> for ::std::option::Option<T> {
    fn block_error(self, block: &str, message: &str) -> Result<T> {
        self.ok_or_else(|| BlockError(block.to_owned(), message.to_owned()))
    }

    fn internal_error(self, context: &str, message: &str) -> Result<T> {
        self.ok_or_else(|| InternalError(context.to_owned(), message.to_owned(), None))
    }
}

/// A set of errors that can occur during the runtime of i3status-rs.
pub enum Error {
    BlockError(String, String),
    ConfigurationError(String, (String, String)),
    InternalError(String, String, Option<(String, String)>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BlockError(ref block, ref message) => {
                f.write_str(&format!("Error in block '{}': {}", block, message))
            }
            ConfigurationError(ref message, _) => {
                f.write_str(&format!("Configuration error: {}", message))
            }
            InternalError(ref context, ref message, _) => f.write_str(&format!(
                "Internal error in context '{}': {}",
                context, message
            )),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BlockError(ref block, ref message) => {
                f.write_str(&format!("Error in block '{}': {}", block, message))
            }
            ConfigurationError(ref message, (ref cause, _)) => f.write_str(&format!(
                "Configuration error: {}.\nCause: {}",
                message, cause
            )),
            InternalError(ref context, ref message, Some((ref cause, _))) => f.write_str(&format!(
                "Internal error in context '{}': {}.\nCause: {}",
                context, message, cause
            )),
            InternalError(ref context, ref message, None) => f.write_str(&format!(
                "Internal error in context '{}': {}",
                context, message
            )),
        }
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            BlockError(_, _) => "Block error occurred in block '{}'",
            ConfigurationError(_, _) => "Configuration error occurred",
            InternalError(_, _, _) => "Internal error occurred",
        }
    }

    fn cause(&self) -> Option<&dyn StdError> {
        None
    }
}

impl<T> From<::crossbeam_channel::SendError<T>> for Error
where
    T: Send,
{
    fn from(_err: ::crossbeam_channel::SendError<T>) -> Error {
        InternalError("unknown".to_string(), "send error".to_string(), None)
    }
}
