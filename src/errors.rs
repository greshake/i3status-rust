use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

pub use std::error::Error as StdError;

/// Result type returned from functions that can have our `Error`s.
pub type Result<T, E = Error> = std::result::Result<T, E>;

type ErrorMsg = Cow<'static, str>;

/// Error type
#[derive(Debug, Clone)]
pub struct Error {
    pub message: Option<ErrorMsg>,
    pub cause: Option<Arc<dyn StdError + Send + Sync + 'static>>,
}

impl Error {
    pub fn new<T: Into<ErrorMsg>>(message: T) -> Self {
        Self {
            message: Some(message.into()),
            cause: None,
        }
    }
}

pub trait ErrorContext<T> {
    fn error<M: Into<ErrorMsg>>(self, message: M) -> Result<T>;
    fn or_error<M: Into<ErrorMsg>, F: FnOnce() -> M>(self, f: F) -> Result<T>;
}

impl<T, E: StdError + Send + Sync + 'static> ErrorContext<T> for Result<T, E> {
    fn error<M: Into<ErrorMsg>>(self, message: M) -> Result<T> {
        self.map_err(|e| Error {
            message: Some(message.into()),
            cause: Some(Arc::new(e)),
        })
    }

    fn or_error<M: Into<ErrorMsg>, F: FnOnce() -> M>(self, f: F) -> Result<T> {
        self.map_err(|e| Error {
            message: Some(f().into()),
            cause: Some(Arc::new(e)),
        })
    }
}

impl<T> ErrorContext<T> for Option<T> {
    fn error<M: Into<ErrorMsg>>(self, message: M) -> Result<T> {
        self.ok_or_else(|| Error {
            message: Some(message.into()),
            cause: None,
        })
    }

    fn or_error<M: Into<ErrorMsg>, F: FnOnce() -> M>(self, f: F) -> Result<T> {
        self.ok_or_else(|| Error {
            message: Some(f().into()),
            cause: None,
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.message.as_deref().unwrap_or("Error"))?;

        if let Some(cause) = &self.cause {
            write!(f, ". Cause: {cause}")?;
        }

        Ok(())
    }
}

impl From<Error> for zbus::fdo::Error {
    fn from(err: Error) -> Self {
        Self::Failed(err.to_string())
    }
}

impl StdError for Error {}

pub trait ToSerdeError<T> {
    fn serde_error<E: serde::de::Error>(self) -> Result<T, E>;
}

impl<T, F> ToSerdeError<T> for Result<T, F>
where
    F: fmt::Display,
{
    fn serde_error<E: serde::de::Error>(self) -> Result<T, E> {
        self.map_err(E::custom)
    }
}

pub struct BoxErrorWrapper(pub Box<dyn StdError + Send + Sync + 'static>);

impl fmt::Debug for BoxErrorWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for BoxErrorWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl StdError for BoxErrorWrapper {}
