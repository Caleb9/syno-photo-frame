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
