use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Result};
use bytes::Bytes;
use regex::Regex;
use serde::Deserialize;
use syno_api::{
    dto::{ApiResponse, List},
    foto::browse::album::dto::Album,
    foto::browse::item::dto::Item,
};

use crate::cli::{Order, SourceSize};
use crate::http::{CookieStore, HttpClient, HttpResponse, Url};

use super::{ApiClient, InvalidHttpResponse, LoginError};

pub struct SynoApiClient<'a, H, C> {
    http_client: &'a H,
    cookie_store: &'a C,
    api_url: Url,
    sharing_id: SharingId,
    password: &'a Option<String>,
}

impl<H: HttpClient, C: CookieStore> ApiClient for SynoApiClient<'_, H, C> {
    fn is_logged_in(&self) -> bool {
        self.cookie_store.cookies(&self.api_url).is_some()
    }

    fn login(&self) -> Result<(), LoginError> {
        let params = [
            ("api", "SYNO.Core.Sharing.Login"),
            ("method", "login"),
            ("version", "1"),
            ("sharing_id", &self.sharing_id),
            ("password", self.password.as_deref().unwrap_or_default()),
        ];
        let response = self
            .http_client
            .post(self.api_url.as_str(), &params, None)
            .map_err(LoginError)?;
        let status = response.status();
        if status.is_success() {
            let dto = response.json::<ApiResponse<Login>>().map_err(LoginError)?;
            if dto.success {
                Ok(())
            } else {
                Err(LoginError(anyhow!(InvalidApiResponse(
                    "login",
                    dto.error.unwrap().code
                ))))
            }
        } else {
            Err(LoginError(anyhow!(InvalidHttpResponse(status))))
        }
    }

    fn get_album_contents_count(&self) -> Result<u32> {
        let params = [
            ("api", syno_api::foto::browse::album::API),
            ("method", "get"),
            ("version", "1"),
        ];
        let response = self.http_client.post(
            self.api_url.as_str(),
            &params,
            Some(("X-SYNO-SHARING", &self.sharing_id)),
        )?;
        read_response(response, |response| {
            let dto = response.json::<ApiResponse<List<Album>>>()?;
            if !dto.success {
                bail!(InvalidApiResponse("get", dto.error.unwrap().code))
            } else {
                let album = dto
                    .data
                    .expect("data field should be populated for successful response")
                    .list
                    .pop();
                if let Some(Album { item_count, .. }) = album {
                    Ok(item_count)
                } else {
                    bail!("Album not found")
                }
            }
        })
    }

    fn get_album_contents(&self, offset: u32, limit: Limit, sort_by: SortBy) -> Result<Vec<Item>> {
        let params = [
            ("api", syno_api::foto::browse::item::API),
            ("method", "list"),
            ("version", "1"),
            ("additional", "[\"thumbnail\"]"),
            ("offset", &offset.to_string()),
            ("limit", &limit.to_string()),
            ("sort_by", &sort_by.to_string()),
            ("sort_direction", "asc"),
        ];
        let response = self.http_client.post(
            self.api_url.as_str(),
            &params,
            Some(("X-SYNO-SHARING", &self.sharing_id)),
        )?;
        read_response(response, |response| {
            let dto = response.json::<ApiResponse<List<Item>>>()?;
            if !dto.success {
                bail!(InvalidApiResponse("list", dto.error.unwrap().code))
            } else {
                Ok(dto
                    .data
                    .expect("data field should be populated for successful response")
                    .list)
            }
        })
    }

    fn get_photo(&self, photo_id: u32, cache_key: &str, source_size: SourceSize) -> Result<Bytes> {
        let size = match source_size {
            SourceSize::S => "sm",
            SourceSize::M => "m",
            SourceSize::L => "xl",
        };
        let params = [
            ("api", "SYNO.Foto.Thumbnail"),
            ("method", "get"),
            ("version", "2"),
            ("_sharing_id", &self.sharing_id),
            ("id", &photo_id.to_string()),
            ("cache_key", cache_key),
            ("type", "unit"),
            ("size", size),
        ];
        let response = self.http_client.get(self.api_url.as_str(), &params)?;
        read_response(response, |response| {
            // TODO deal with a situation when the API returns a response with successful status code
            // but the body contains JSON with an error instead of an image
            let bytes = response.bytes()?;
            Ok(bytes)
        })
    }
}
fn read_response<R, S, T>(response: R, on_success: S) -> Result<T>
where
    R: HttpResponse,
    S: FnOnce(R) -> Result<T>,
{
    let status = response.status();
    if status.is_success() {
        on_success(response)
    } else {
        bail!(InvalidHttpResponse(status))
    }
}

impl<'a, H: HttpClient, C: CookieStore> SynoApiClient<'a, H, C> {
    pub fn build(http_client: &'a H, cookie_store: &'a C, share_link: &Url) -> Result<Self> {
        let (api_url, sharing_id) = parse_share_link(share_link)?;
        Ok(Self {
            http_client,
            cookie_store,
            api_url,
            sharing_id,
            password: &None,
        })
    }

    pub fn with_password(mut self, password: &'a Option<String>) -> Self {
        self.password = password;
        self
    }
}

/// Returns Synology Photos API URL and sharing id extracted from album share link
fn parse_share_link(share_link: &Url) -> Result<(Url, SharingId)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(https?://.+)/([^/]+)/?$").unwrap());
    let Some(captures) = re.captures(share_link.as_str()) else {
        bail!("Invalid share link: {}", share_link)
    };
    let api_url = Url::parse(&format!("{}/webapi/entry.cgi", &captures[1]))?;
    Ok((api_url, SharingId(captures[2].to_owned())))
}

#[derive(Debug, Deserialize)]
pub struct Login {/* Empty brackets are needed for the deserializer to work */}

#[derive(Debug)]
struct SharingId(pub String);

impl Deref for SharingId {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Synology Photos API accepts limit values between 0 and 5000
#[derive(Clone, Copy, Debug)]
pub struct Limit(u32);

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

#[derive(Clone, Copy, Debug)]
pub enum SortBy {
    TakenTime,
    FileName,
}

impl From<Order> for SortBy {
    fn from(value: Order) -> Self {
        match value {
            /* Random is not an option in the API. Randomization is implemented client-side and
             * essentially makes the sort_by query parameter irrelevant. */
            Order::ByDate | Order::Random => SortBy::TakenTime,
            Order::ByName => SortBy::FileName,
        }
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

#[derive(Debug)]
pub struct InvalidApiResponse(&'static str, u16);

impl Display for InvalidApiResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid Synology API '{}' response code: {}",
            self.0, self.1
        )
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
