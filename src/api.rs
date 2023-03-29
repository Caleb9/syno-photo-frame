use std::{error::Error, fmt::Display};

use bytes::Bytes;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::{self, StatusCode, Url};

use PhotosApiError::{InvalidApiResponse, InvalidHttpResponse};

use crate::{http::Response, ErrorToString};

use self::dto::Photo;

#[derive(Debug)]
pub enum PhotosApiError {
    Reqwest(String),
    InvalidHttpResponse(StatusCode),
    InvalidApiResponse(&'static str, i32),
}

/// Returns Synology Photos API URL and sharing id extracted from album share link
pub fn parse_share_link(share_link: &Url) -> Result<(Url, String), String> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^(https?://.+)/([^/]+)$").unwrap();
    }
    if let Some(captures) = RE.captures(share_link.as_str()) {
        let api_url =
            Url::parse(&format!("{}/webapi/entry.cgi", &captures[1])).map_err_to_string()?;
        Ok((api_url, captures[2].to_owned()))
    } else {
        Err(format!("Invalid share link {share_link}"))
    }
}

pub fn login<F, R>(post: &F, api_url: &Url, sharing_id: &str) -> Result<(), PhotosApiError>
where
    F: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
    R: Response,
{
    let params = [
        ("api", "SYNO.Core.Sharing.Login"),
        ("method", "login"),
        ("version", "1"),
        ("sharing_id", sharing_id),
    ];
    let response = post(api_url.as_str(), &params, None)?;
    read_response(response, |response| {
        let dto = response.json::<dto::ApiResponse<dto::Login>>()?;
        if !dto.success {
            Err(InvalidApiResponse("Login", dto.error.unwrap().code))
        } else {
            Ok(())
        }
    })
}

pub fn get_album_contents<F, R>(
    post: &F,
    api_url: &Url,
    sharing_id: &str,
    offset: u32,
    limit: u32,
) -> Result<Vec<Photo>, PhotosApiError>
where
    F: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
    R: Response,
{
    let params = [
        ("api", "SYNO.Foto.Browse.Item"),
        ("method", "list"),
        ("version", "1"),
        ("additional", "[\"thumbnail\"]"),
        ("offset", &offset.to_string()),
        ("limit", &limit.to_string()),
        ("sort_by", "takentime"),
        ("sort_direction", "asc"),
    ];
    let response = post(
        api_url.as_str(),
        &params,
        Some(("X-SYNO-SHARING", sharing_id)),
    )?;
    read_response(response, |response| {
        let dto = response.json::<dto::ApiResponse<dto::List<dto::Photo>>>()?;
        if !dto.success {
            Err(InvalidApiResponse("List", dto.error.unwrap().code))
        } else {
            Ok(dto
                .data
                .expect("data field should be populated for successful response")
                .list)
        }
    })
}

pub fn get_photo<F, R>(
    get: &F,
    api_url: &Url,
    sharing_id: &str,
    photo_dto: &dto::Photo,
) -> Result<Bytes, PhotosApiError>
where
    F: Fn(&str, &[(&str, &str)]) -> Result<R, String>,
    R: Response,
{
    let params = [
        ("api", "SYNO.Foto.Thumbnail"),
        ("method", "get"),
        ("version", "2"),
        ("_sharing_id", sharing_id),
        ("id", &photo_dto.id.to_string()),
        ("cache_key", &photo_dto.additional.thumbnail.cache_key),
        ("type", "unit"),
        ("size", "xl"),
    ];
    let response = get(api_url.as_str(), &params)?;
    read_response(response, |response| {
        let bytes = response.bytes()?;
        Ok(bytes)
    })
}

fn read_response<F, R, T>(response: R, on_success: F) -> Result<T, PhotosApiError>
where
    F: FnOnce(R) -> Result<T, PhotosApiError>,
    R: Response,
{
    let status = response.status();
    if !status.is_success() {
        Err(InvalidHttpResponse(status))
    } else {
        on_success(response)
    }
}

impl Display for PhotosApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            PhotosApiError::Reqwest(ref reqwest_error) => write!(f, "{reqwest_error}"),
            InvalidHttpResponse(ref status) => {
                write!(f, "Invalid HTTP response {status}")
            }
            InvalidApiResponse(ref request, ref code) => {
                write!(f, "{request}: invalid Synology API response {code}")
            }
        }
    }
}

impl Error for PhotosApiError {}

impl From<String> for PhotosApiError {
    fn from(value: String) -> Self {
        PhotosApiError::Reqwest(value)
    }
}

pub mod dto {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct ApiResponse<T> {
        pub success: bool,
        pub error: Option<ApiError>,
        pub data: Option<T>,
    }

    #[derive(Debug, Deserialize)]
    pub struct ApiError {
        pub code: i32,
    }

    #[derive(Debug, Deserialize)]
    pub struct Login {}

    #[derive(Debug, Deserialize)]
    pub struct List<T> {
        pub list: Vec<T>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Photo {
        pub id: i32,
        pub additional: Additional,
    }

    #[derive(Debug, Deserialize)]
    pub struct Additional {
        pub thumbnail: Thumbnail,
    }

    #[derive(Debug, Deserialize)]
    pub struct Thumbnail {
        pub cache_key: String,
    }
}
