//! Errors

use std::{
    error::Error,
    fmt::{Display, Formatter},
    sync::mpsc::{SendError, TrySendError},
};

use crate::api_photos::PhotosApiError;

#[derive(Debug)]
pub enum SynoPhotoFrameError {
    /// Error logging in to Synology Photos that results in app termination
    Login(PhotosApiError),
    /// Any other error that is not login and doesn't result in a panic will be logged and handled
    Other(String),
}

impl Error for SynoPhotoFrameError {}

impl From<String> for SynoPhotoFrameError {
    fn from(value: String) -> Self {
        SynoPhotoFrameError::Other(value)
    }
}

impl<T> From<SendError<T>> for SynoPhotoFrameError {
    fn from(value: SendError<T>) -> Self {
        SynoPhotoFrameError::Other(value.to_string())
    }
}

impl<T> From<TrySendError<T>> for SynoPhotoFrameError {
    fn from(value: TrySendError<T>) -> Self {
        SynoPhotoFrameError::Other(value.to_string())
    }
}

impl Display for SynoPhotoFrameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SynoPhotoFrameError::Login(error) => write!(f, "{error}"),
            SynoPhotoFrameError::Other(error) => write!(f, "{error}"),
        }
    }
}

/// Maps [Result<T,E>] to [Result<T,String>]
pub trait ErrorToString<T> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: Display> ErrorToString<T> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
