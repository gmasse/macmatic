//! Error reporting

use std::fmt;
use std::error;

/// Enumeration of different error kinds.
#[derive(Debug)]
pub enum ErrorKind {
    /// Occurs when template is not found in the window.
    ImageNotFound,
    /// Occurs when error is raised at CoreFoundation level.
    CoreFoundation,
    /// Allows to raise OpenCV errors directly.
    Opencv(opencv::Error),
}

/// Structure representing an error.
#[derive(Debug)]
pub struct Error {
    /// A known kind of error.
    pub kind: ErrorKind,
    /// Details relative to the error.
    pub message: String,
}

impl fmt::Display for Error {
    /// Formats the error for display.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ErrorKind::ImageNotFound => write!(f, "Image not found: {}", self.message),
            ErrorKind::CoreFoundation => write!(f, "Core Foundation: {}", self.message),
            ErrorKind::Opencv(ref e) => write!(f, "OpenCV Error: {}", e),
        }
    }
}

impl error::Error for Error { }

impl From<opencv::Error> for Error {
    /// Converts an OpenCV error to this error type.
    fn from(err: opencv::Error) -> Error {
        Error {
            message: err.to_string(),
            kind: ErrorKind::Opencv(err),
        }
    }
}
