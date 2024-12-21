use std::{
    fmt,
    fmt::{Display, Formatter},
    sync::OnceLock,
};

use anyhow::{anyhow, bail, Result};
use bytes::Bytes;
use regex::Regex;
use serde::Deserialize;
use syno_api::dto::{ApiResponse, List};

use crate::{
    api_client::{ApiClient, LoginError, SharingId, SortBy},
    cli::SourceSize,
    http::{read_response, CookieStore, HttpClient, HttpResponse, InvalidHttpResponse, Url},
};

pub struct SynoApiClient<'a, H, C> {
    http_client: &'a H,
    cookie_store: &'a C,
    api_url: Url,
    sharing_id: SharingId,
    password: &'a Option<String>,
}

impl<H: HttpClient, C: CookieStore> ApiClient for SynoApiClient<'_, H, C> {
    type Photo = syno_api::foto::browse::item::dto::Item;

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

    fn get_photo_metadata(&self, sort_by: SortBy) -> Result<Vec<Self::Photo>> {
        let params = [
            ("api", syno_api::foto::browse::item::API),
            ("method", "list"),
            ("version", "1"),
            ("additional", "[\"thumbnail\"]"),
            ("offset", "0"),
            ("limit", "5000"), // Limit imposed by API
            ("sort_by", &sort_by.to_string()),
            ("sort_direction", "asc"),
        ];
        let response = self.http_client.post(
            self.api_url.as_str(),
            &params,
            Some(("X-SYNO-SHARING", &self.sharing_id)),
        )?;
        read_response(response, |response| {
            let dto = response.json::<ApiResponse<List<Self::Photo>>>()?;
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

    fn get_photo_bytes(&self, photo: &Self::Photo, source_size: SourceSize) -> Result<Bytes> {
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
            ("id", &photo.id.to_string()),
            (
                "cache_key",
                &photo
                    .additional
                    .as_ref()
                    .expect("expected additional")
                    .thumbnail
                    .as_ref()
                    .expect("expected thumbnail")
                    .cache_key,
            ),
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

impl<'a, H, C> SynoApiClient<'a, H, C> {
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
            "http://test.dsm.addr/photo/aa/sharing/FakeSharingId",
            "http://test.dsm.addr/photo/aa/sharing/webapi/entry.cgi",
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
