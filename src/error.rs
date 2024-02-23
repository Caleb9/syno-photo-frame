//! Errors

use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use crate::{api_photos::PhotosApiError, transition::TransitionError, QuitEvent};

#[derive(Clone, Debug)]
pub enum FrameError {
    /// Error logging in to Synology Photos that results in app termination
    Login(PhotosApiError),
    /// Any other error that is not login and doesn't result in a panic will be logged and handled
    Other(String),
    /// Quit event signaling the app shutdown
    Quit(QuitEvent),
}

impl Error for FrameError {}

impl Display for FrameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Login(error) => write!(f, "{error}"),
            FrameError::Other(error) => write!(f, "{error}"),
            FrameError::Quit(quit_event) => write!(f, "{quit_event}"),
        }
    }
}

impl From<String> for FrameError {
    fn from(value: String) -> Self {
        FrameError::Other(value)
    }
}

impl From<TransitionError> for FrameError {
    fn from(value: TransitionError) -> Self {
        match value {
            TransitionError::Sdl(error) => FrameError::Other(error),
            TransitionError::Quit(quit_event) => FrameError::Quit(quit_event),
        }
    }
}

impl From<QuitEvent> for FrameError {
    fn from(value: QuitEvent) -> Self {
        FrameError::Quit(value)
    }
}

/// Maps [Result<T, E>] to [Result<T, String>]
pub trait ErrorToString<T> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: Display> ErrorToString<T> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
