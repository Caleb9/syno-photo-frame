use std::{error::Error, fmt::Display, sync::OnceLock};

use bytes::Bytes;
use regex::Regex;

use crate::{
    cli::SourceSize,
    error::ErrorToString,
    http::{Client, Response, StatusCode, Url},
};

use PhotosApiError::{InvalidApiResponse, InvalidHttpResponse};

#[derive(Debug)]
pub(crate) struct SharingId(String);

impl Display for SharingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Returns Synology Photos API URL and sharing id extracted from album share link
pub(crate) fn parse_share_link(share_link: &Url) -> Result<(Url, SharingId), String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(https?://.+)/([^/]+)/?$").unwrap());
    let Some(captures) = re.captures(share_link.as_str()) else {
        return Err(format!("Invalid share link: {share_link}"));
    };
    let api_url = Url::parse(&format!("{}/webapi/entry.cgi", &captures[1])).map_err_to_string()?;
    Ok((api_url, SharingId(captures[2].to_owned())))
}

pub(crate) fn login(
    client: &impl Client,
    api_url: &Url,
    sharing_id: &SharingId,
    password: &Option<String>,
) -> Result<(), PhotosApiError> {
    let params = [
        ("api", "SYNO.Core.Sharing.Login"),
        ("method", "login"),
        ("version", "1"),
        ("sharing_id", &sharing_id.0),
        ("password", password.as_deref().unwrap_or_default()),
    ];
    let response = client.post(api_url.as_str(), &params, None)?;
    read_response(response, |response| {
        let dto = response.json::<dto::ApiResponse<dto::Login>>()?;
        if !dto.success {
            Err(InvalidApiResponse("login", dto.error.unwrap().code))
        } else {
            Ok(())
        }
    })
}

pub(crate) fn get_album_contents_count(
    client: &impl Client,
    api_url: &Url,
    sharing_id: &SharingId,
) -> Result<Vec<dto::Album>, PhotosApiError> {
    let params = [
        ("api", "SYNO.Foto.Browse.Album"),
        ("method", "get"),
        ("version", "1"),
    ];
    let response = client.post(
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

/// Synology Photos API accepts limit values between 0 and 5000
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limit(u32);

impl TryFrom<u32> for Limit {
    type Error = &'static str;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value > 5000 {
            Err("Limit only accepts values up to 5000")
        } else {
            Ok(Limit(value))
        }
    }
}

impl Display for Limit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Gets metadata for photos contained in an album
pub(crate) fn get_album_contents(
    client: &impl Client,
    api_url: &Url,
    sharing_id: &SharingId,
    offset: u32,
    limit: Limit,
) -> Result<Vec<dto::Photo>, PhotosApiError> {
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
    let response = client.post(
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

/// Gets JPEG photo bytes
pub(crate) fn get_photo(
    client: &impl Client,
    api_url: &Url,
    sharing_id: &SharingId,
    (photo_id, photo_cache_key, source_size): (i32, &str, SourceSize),
) -> Result<Bytes, PhotosApiError> {
    let size = match source_size {
        SourceSize::S => "sm",
        SourceSize::M => "m",
        SourceSize::L => "xl",
    };
    let params = [
        ("api", "SYNO.Foto.Thumbnail"),
        ("method", "get"),
        ("version", "2"),
        ("_sharing_id", &sharing_id.0),
        ("id", &photo_id.to_string()),
        ("cache_key", photo_cache_key),
        ("type", "unit"),
        ("size", size),
    ];
    let response = client.get(api_url.as_str(), &params)?;
    read_response(response, |response| {
        let bytes = response.bytes()?;
        Ok(bytes)
    })
}

fn read_response<R, S, T>(response: R, on_success: S) -> Result<T, PhotosApiError>
where
    R: Response,
    S: FnOnce(R) -> Result<T, PhotosApiError>,
{
    let status = response.status();
    if !status.is_success() {
        Err(InvalidHttpResponse(status))
    } else {
        on_success(response)
    }
}

#[derive(Debug)]
pub enum PhotosApiError {
    Reqwest(String),
    InvalidHttpResponse(StatusCode),
    InvalidApiResponse(&'static str, i32),
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
