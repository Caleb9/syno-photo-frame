use std::{error::Error, fmt::Display};

use bytes::Bytes;
use lazy_static::lazy_static;
use regex::Regex;

use crate::{
    http::{Response, StatusCode, Url},
    ErrorToString,
};

use PhotosApiError::{InvalidApiResponse, InvalidHttpResponse};

#[derive(Debug)]
pub(crate) enum PhotosApiError {
    Reqwest(String),
    InvalidHttpResponse(StatusCode),
    InvalidApiResponse(&'static str, i32),
}

#[derive(Debug)]
pub(crate) struct SharingId(String);

/// Returns Synology Photos API URL and sharing id extracted from album share link
pub(crate) fn parse_share_link(share_link: &Url) -> Result<(Url, SharingId), String> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^(https?://.+)/([^/]+)/?$").unwrap();
    }
    if let Some(captures) = RE.captures(share_link.as_str()) {
        let api_url =
            Url::parse(&format!("{}/webapi/entry.cgi", &captures[1])).map_err_to_string()?;
        Ok((api_url, SharingId(captures[2].to_owned())))
    } else {
        Err(format!("Invalid share link: {share_link}"))
    }
}

pub(crate) fn login<P, R>(
    post: &P,
    api_url: &Url,
    sharing_id: &SharingId,
) -> Result<(), PhotosApiError>
where
    P: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
    R: Response,
{
    let params = [
        ("api", "SYNO.Core.Sharing.Login"),
        ("method", "login"),
        ("version", "1"),
        ("sharing_id", &sharing_id.0),
    ];
    let response = post(api_url.as_str(), &params, None)?;
    read_response(response, |response| {
        let dto = response.json::<dto::ApiResponse<dto::Login>>()?;
        if !dto.success {
            Err(InvalidApiResponse("login", dto.error.unwrap().code))
        } else {
            Ok(())
        }
    })
}

pub(crate) fn get_album_contents_count<P, R>(
    post: &P,
    api_url: &Url,
    sharing_id: &SharingId,
) -> Result<Vec<dto::Album>, PhotosApiError>
where
    P: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
    R: Response,
{
    let params = [
        ("api", "SYNO.Foto.Browse.Album"),
        ("method", "get"),
        ("version", "1"),
    ];
    let response = post(
        api_url.as_str(),
        &params,
        Some(("X-SYNO-SHARING", &sharing_id.0)),
    )?;
    read_response(response, |response| {
        let dto = response.json::<dto::ApiResponse<dto::List<dto::Album>>>()?;
        if !dto.success {
            Err(InvalidApiResponse("get", dto.error.unwrap().code))
        } else {
            Ok(dto
                .data
                .expect("data field should be populated for successful response")
                .list)
        }
    })
}

pub(crate) fn get_album_contents<P, R>(
    post: &P,
    api_url: &Url,
    sharing_id: &SharingId,
    offset: u32,
    limit: u32,
) -> Result<Vec<dto::Photo>, PhotosApiError>
where
    P: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
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
        Some(("X-SYNO-SHARING", &sharing_id.0)),
    )?;
    read_response(response, |response| {
        let dto = response.json::<dto::ApiResponse<dto::List<dto::Photo>>>()?;
        if !dto.success {
            Err(InvalidApiResponse("list", dto.error.unwrap().code))
        } else {
            Ok(dto
                .data
                .expect("data field should be populated for successful response")
                .list)
        }
    })
}

pub(crate) fn get_photo<G, R>(
    get: &G,
    api_url: &Url,
    sharing_id: &SharingId,
    photo_dto: &dto::Photo,
) -> Result<Bytes, PhotosApiError>
where
    G: Fn(&str, &[(&str, &str)]) -> Result<R, String>,
    R: Response,
{
    let params = [
        ("api", "SYNO.Foto.Thumbnail"),
        ("method", "get"),
        ("version", "2"),
        ("_sharing_id", &sharing_id.0),
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

fn read_response<S, R, T>(response: R, on_success: S) -> Result<T, PhotosApiError>
where
    S: FnOnce(R) -> Result<T, PhotosApiError>,
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
        match self {
            PhotosApiError::Reqwest(ref reqwest_error) => write!(f, "{reqwest_error}"),
            InvalidHttpResponse(ref status) => {
                write!(f, "Invalid HTTP response code: {status}")
            }
            InvalidApiResponse(ref request, ref code) => {
                write!(f, "Invalid Synology API '{request}' response code: {code}")
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

impl Display for SharingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub(crate) mod dto {
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
    pub struct Album {
        pub item_count: u32,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_share_link_is_ok_for_valid_link() {
        test_case(
            "https://test.dsm.addr:5001/aa/sharing/FakeSharingId",
            "https://test.dsm.addr:5001/aa/sharing/webapi/entry.cgi",
        );
        test_case(
            "https://test.dsm.addr/photo/aa/sharing/FakeSharingId",
            "https://test.dsm.addr/photo/aa/sharing/webapi/entry.cgi",
        );

        fn test_case(share_link: &str, expected_api_url: &str) {
            let link = Url::parse(share_link).unwrap();

            let result = parse_share_link(&link);

            assert!(result.is_ok());
            let (api_url, sharing_id) = result.unwrap();
            assert_eq!(api_url.as_str(), expected_api_url);
            assert_eq!(sharing_id.0, "FakeSharingId");
        }
    }
}
