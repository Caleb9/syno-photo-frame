use std::fmt::Formatter;

use anyhow::Result;
use bytes::Bytes;
use syno_api::foto::browse::item::dto::Item;

use crate::api_client::syno_client::{Limit, SortBy};
use crate::cli::SourceSize;
use crate::http::StatusCode;

pub mod syno_client;

pub trait ApiClient {
    fn is_logged_in(&self) -> bool;

    fn login(&self) -> Result<(), LoginError>;

    fn get_album_contents_count(&self) -> Result<u32>;

    fn get_album_contents(&self, offset: u32, limit: Limit, sort_by: SortBy) -> Result<Vec<Item>>;

    fn get_photo(&self, photo_id: u32, cache_key: &str, source_size: SourceSize) -> Result<Bytes>;
}

#[derive(Debug)]
pub struct LoginError(pub anyhow::Error);

impl std::fmt::Display for LoginError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug)]
pub struct InvalidHttpResponse(pub StatusCode);

impl std::fmt::Display for InvalidHttpResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid HTTP response code: {}", self.0)
    }
}
