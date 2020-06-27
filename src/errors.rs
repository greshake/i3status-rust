pub use std::error::Error as StdError;
use std::fmt;

pub use self::Error::{BlockError, ConfigurationError, InternalError, InvalidFormatter};
use crate::formatter::VarFormatter;

/// Result type returned from functions that can have our `Error`s.
pub type Result<T> = ::std::result::Result<T, Error>;

pub trait ResultExtBlock<T, E> {
    fn block_error(self, block: &str, message: &str) -> Result<T>;
}

pub trait ResultExtInternal<T, E> {
    // TODO: this incitates to make many unecessary allocations through "format!", it may be
    //       relevant to change this API? (using map_err?)
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
    InvalidFormatter {
        formatter: VarFormatter,
        var: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlockError(block, message) => write!(f, "Error in block '{}': {}", block, message),
            ConfigurationError(message, _) => write!(f, "Configuration error: {}", message),
            InternalError(context, message, _) => {
                write!(f, "Internal error in context '{}': {}", context, message)
            }
            InvalidFormatter { formatter, var } => {
                write!(f, "Invalid formatter for '{}': {:?}", var, formatter)
            }
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)?;

        match self {
            ConfigurationError(_, (cause, _)) => write!(f, "\nCause: {}", cause),
            InternalError(_, _, Some((cause, _))) => write!(f, "\nCause: {}", cause),
            _ => Ok(()),
        }
    }
}

impl StdError for Error {
    fn description(&self) -> &'static str {
        match self {
            BlockError(_, _) => "Block error occurred in block '{}'",
            ConfigurationError(_, _) => "Configuration error occurred",
            InternalError(_, _, _) => "Internal error occurred",
            InvalidFormatter { .. } => "Invalid formatter for variable",
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
