use std::{
    fmt,
    fmt::{Display, Formatter},
    sync::OnceLock,
};

use anyhow::{Result, anyhow, bail};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Deserialize;
use syno_api::dto::{ApiResponse, List};

use crate::{
    api_client::{ApiClient, LoginError, Metadata, SharingId, SortBy},
    cli::SourceSize,
    http::{CookieStore, HttpClient, HttpResponse, InvalidHttpResponse, Url, read_response},
    metadata::Location,
};

pub struct SynoApiClient<'a, H, C> {
    http_client: &'a H,
    cookie_store: &'a C,
    api_url: Url,
    api_thumbnail_get_url: Url,
    sharing_id: SharingId,
    password: Option<String>,
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
            ("version", "4"),
            ("additional", "[\"thumbnail\", \"address\"]"),
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
            /* For ad-hoc discovery of API responses */
            // let text = response.text()?;
            // dbg!(&text);
            // let dto = serde_json::from_str::<ApiResponse<List<Self::Photo>>>(&text)?;
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
        let thumbnail = photo
            .additional
            .as_ref()
            .expect("expected additional")
            .thumbnail
            .as_ref()
            .expect("expected thumbnail");
        let params = [
            ("type", "unit"),
            ("id", &thumbnail.unit_id.to_string()),
            ("cache_key", &thumbnail.cache_key),
            ("_sharing_id", &self.sharing_id),
            ("size", size),
        ];
        let response = self
            .http_client
            .get(self.api_thumbnail_get_url.as_str(), &params)?;
        read_response(response, |response| {
            let bytes = response.bytes()?;
            Ok(bytes)
        })
    }
}

impl<'a, H, C> SynoApiClient<'a, H, C> {
    pub fn build(http_client: &'a H, cookie_store: &'a C, share_link: &Url) -> Result<Self> {
        let (api_url, api_thumbnail_get_url, sharing_id) = parse_share_link(share_link)?;
        Ok(Self {
            http_client,
            cookie_store,
            api_url,
            api_thumbnail_get_url,
            sharing_id,
            password: None,
        })
    }

    pub fn with_password(mut self, password: &Option<String>) -> Self {
        self.password = password.as_ref().map(|p| format!("\"{p}\""));
        self
    }
}

impl Metadata for syno_api::foto::browse::item::dto::Item {
    fn date(&self) -> DateTime<Utc> {
        match self.time.try_into() {
            Ok(time) => DateTime::from_timestamp(time, 0).unwrap_or_default(),
            Err(e) => {
                log::warn!("failed to convert time to i64: {}", e);
                DateTime::default()
            }
        }
    }

    fn location(&self) -> Location {
        let address = self
            .additional
            .as_ref()
            .and_then(|additional| additional.address.as_ref());
        if let Some(address) = address {
            /* Try finding the most specific location */
            let area = [
                &address.village,
                &address.town,
                &address.city,
                &address.county,
                &address.state,
            ]
            .iter()
            .find(|s| !s.is_empty())
            .cloned()
            .cloned();
            let country = if !address.country.is_empty() {
                Some(address.country.clone())
            } else {
                None
            };
            Location::new(area, country)
        } else {
            Default::default()
        }
    }
}

/// Returns Synology Photos API URL and sharing id extracted from album share link
fn parse_share_link(share_link: &Url) -> Result<(Url, Url, SharingId)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE
        .get_or_init(|| Regex::new(r"^(https?://.+)/([[:alpha:]]{2}/sharing)/([^/]+)/?$").unwrap());
    let Some(captures) = re.captures(share_link.as_str()) else {
        bail!("Invalid share link: {}", share_link)
    };
    let api_url = Url::parse(&format!(
        "{}/{}/webapi/entry.cgi",
        &captures[1], &captures[2]
    ))?;
    let api_thumbnail_get_url =
        Url::parse(&format!("{}/synofoto/api/v2/p/Thumbnail/get", &captures[1]))?;
    Ok((
        api_url,
        api_thumbnail_get_url,
        SharingId(captures[3].to_owned()),
    ))
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
    use reqwest::cookie::Jar;
    use syno_api::foto::browse::item::dto::{Additional, Address, Item};

    use crate::test_helpers::{self, MockHttpClient};

    use super::*;

    #[test]
    fn parse_share_link_is_ok_for_valid_link() {
        test_case(
            "https://test.dsm.addr:5001/aa/sharing/FakeSharingId",
            "https://test.dsm.addr:5001/aa/sharing/webapi/entry.cgi",
            "https://test.dsm.addr:5001/synofoto/api/v2/p/Thumbnail/get",
        );
        test_case(
            "http://test.dsm.addr/photo/aa/sharing/FakeSharingId",
            "http://test.dsm.addr/photo/aa/sharing/webapi/entry.cgi",
            "http://test.dsm.addr/photo/synofoto/api/v2/p/Thumbnail/get",
        );

        fn test_case(
            share_link: &str,
            expected_api_url: &str,
            expected_api_thumbnail_get_url: &str,
        ) {
            let link = Url::parse(share_link).unwrap();

            let result = parse_share_link(&link);

            assert!(result.is_ok());
            let (api_url, api_thumbnail_get_url, sharing_id) = result.unwrap();
            assert_eq!(api_url.as_str(), expected_api_url);
            assert_eq!(
                api_thumbnail_get_url.as_str(),
                expected_api_thumbnail_get_url
            );
            assert_eq!(sharing_id.0, "FakeSharingId");
        }
    }

    #[test]
    fn login_password_param_is_quoted() {
        const PASSWORD: &str = "P455w0rd";
        let mut http_client = MockHttpClient::new();
        http_client
            .expect_post()
            .withf(|_, form, _| form.contains(&("password", &format!("\"{PASSWORD}\""))))
            .return_once(move |_, _, _| Ok(test_helpers::new_success_response_with_json(Login {})));
        let password = Some(PASSWORD.to_owned());
        let cookie_store = Jar::default();
        let sut = SynoApiClient::build(
            &http_client,
            &cookie_store,
            &Url::parse("http://dummy/aa/sharing/FakeSharingId").unwrap(),
        )
        .unwrap()
        .with_password(&password);

        let _ = sut.login();

        http_client.checkpoint();
    }

    #[test]
    fn metadata_location_prioritizes_village() {
        let photo = Item {
            additional: Some(Additional {
                address: Some(Address {
                    village: "Tiny Village".to_string(),
                    town: "Small Town".to_string(),
                    city: "Big City".to_string(),
                    county: "Bounty County".to_string(),
                    state: "Solid State".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = photo.location().area;

        assert_eq!(result, Some("Tiny Village".to_string()));
    }

    #[test]
    fn metadata_location_prioritizes_town() {
        let photo = Item {
            additional: Some(Additional {
                address: Some(Address {
                    town: "Small Town".to_string(),
                    city: "Big City".to_string(),
                    county: "Bounty County".to_string(),
                    state: "Solid State".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = photo.location().area;

        assert_eq!(result, Some("Small Town".to_string()));
    }

    #[test]
    fn metadata_location_prioritizes_city() {
        let photo = Item {
            additional: Some(Additional {
                address: Some(Address {
                    city: "Big City".to_string(),
                    county: "Bounty County".to_string(),
                    state: "Solid State".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = photo.location().area;

        assert_eq!(result, Some("Big City".to_string()));
    }

    #[test]
    fn metadata_location_prioritizes_county() {
        let photo = Item {
            additional: Some(Additional {
                address: Some(Address {
                    county: "Bounty County".to_string(),
                    state: "Solid State".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = photo.location().area;

        assert_eq!(result, Some("Bounty County".to_string()));
    }

    #[test]
    fn metadata_location_prioritizes_state() {
        let photo = Item {
            additional: Some(Additional {
                address: Some(Address {
                    state: "Solid State".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = photo.location().area;

        assert_eq!(result, Some("Solid State".to_string()));
    }
}
