//! Errors

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::api_photos::PhotosApiError;

#[derive(Debug)]
pub enum SynoPhotoFrameError {
    Login(PhotosApiError),
    Other(String),
}

impl From<String> for SynoPhotoFrameError {
    fn from(value: String) -> Self {
        SynoPhotoFrameError::Other(value)
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

impl Error for SynoPhotoFrameError {}

/// Maps [Result<T,E>] to [Result<T,String>]
pub trait ErrorToString<T> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: Display> ErrorToString<T> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
