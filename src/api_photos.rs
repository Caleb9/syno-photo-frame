use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    ops::Deref,
};

use bytes::Bytes;

use crate::{
    cli::SourceSize,
    http::{HttpClient, HttpResponse, StatusCode, Url},
};

use PhotosApiError::{InvalidApiResponse, InvalidHttpResponse};

#[derive(Clone, Debug)]
pub enum PhotosApiError {
    Reqwest(String),
    InvalidHttpResponse(StatusCode),
    InvalidApiResponse(&'static str, i32),
}

// TODO: move to api_client
#[derive(Debug)]
pub struct SharingId(pub String);

pub fn login(
    client: &impl HttpClient,
    api_url: &Url,
    sharing_id: &SharingId,
    password: &Option<String>,
) -> Result<(), PhotosApiError> {
    let params = [
        ("api", "SYNO.Core.Sharing.Login"),
        ("method", "login"),
        ("version", "1"),
        ("sharing_id", sharing_id),
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

pub fn get_album_contents_count(
    client: &impl HttpClient,
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
        Some(("X-SYNO-SHARING", sharing_id)),
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
pub struct Limit(u32);

#[derive(Clone, Copy, Debug)]
pub enum SortBy {
    TakenTime,
    FileName,
}

/// Gets metadata for photos contained in an album
pub fn get_album_contents(
    client: &impl HttpClient,
    api_url: &Url,
    sharing_id: &SharingId,
    offset: u32,
    limit: Limit,
    sort_by: SortBy,
) -> Result<Vec<dto::Photo>, PhotosApiError> {
    let params = [
        ("api", "SYNO.Foto.Browse.Item"),
        ("method", "list"),
        ("version", "1"),
        ("additional", "[\"thumbnail\"]"),
        ("offset", &offset.to_string()),
        ("limit", &limit.to_string()),
        ("sort_by", &sort_by.to_string()),
        ("sort_direction", "asc"),
    ];
    let response = client.post(
        api_url.as_str(),
        &params,
        Some(("X-SYNO-SHARING", sharing_id)),
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
pub fn get_photo(
    client: &impl HttpClient,
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
        ("_sharing_id", sharing_id),
        ("id", &photo_id.to_string()),
        ("cache_key", photo_cache_key),
        ("type", "unit"),
        ("size", size),
    ];
    let response = client.get(api_url.as_str(), &params)?;
    read_response(response, |response| {
        // TODO deal with a situation when the API returns a response with successful status code
        // but the body contains JSON with an error instead of an image
        let bytes = response.bytes()?;
        Ok(bytes)
    })
}

fn read_response<R, S, T>(response: R, on_success: S) -> Result<T, PhotosApiError>
where
    R: HttpResponse,
    S: FnOnce(R) -> Result<T, PhotosApiError>,
{
    let status = response.status();
    if status.is_success() {
        on_success(response)
    } else {
        Err(InvalidHttpResponse(status))
    }
}

impl Deref for SharingId {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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

impl Deref for Limit {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for SortBy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SortBy::FileName => "filename",
                SortBy::TakenTime => "takentime",
            }
        )
    }
}

impl Error for PhotosApiError {}

impl Display for PhotosApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PhotosApiError::Reqwest(reqwest_error) => write!(f, "{reqwest_error}"),
            InvalidHttpResponse(status) => {
                write!(f, "Invalid HTTP response code: {status}")
            }
            InvalidApiResponse(request, code) => {
                write!(f, "Invalid Synology API '{request}' response code: {code}")
            }
        }
    }
}

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
    pub struct Login {/* Empty brackets are needed for the deserializer to work */}

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
