use std::sync::OnceLock;

use anyhow::{Result, anyhow, bail};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::json;

use crate::{
    LoginError,
    api_client::{
        ApiClient, SharingId, SortBy,
        immich_client::dto::{
            AlbumResponseDto, AssetResponseDto, ExifResponseDto, GetAlbumInfoResponseDto,
            GetAssetInfoResponseDto, GetServerVersionResponseDto, SearchAssetsResponseDto,
            SharedLinkResponseDto,
        },
    },
    cli::SourceSize,
    http::{HttpClient, HttpResponse, Query, Url, read_response},
    metadata::{Location, Metadata},
};

pub struct ImmichApiClient<'a, H> {
    http_client: &'a H,
    api_url: Url,
    sharing_id: SharingId,
    password: &'a Option<String>,
}

impl<H: HttpClient> ApiClient for ImmichApiClient<'_, H> {
    type Photo = AssetResponseDto;

    fn is_logged_in(&self) -> bool {
        false
    }

    fn login(&self) -> Result<(), LoginError> {
        let server_version = self.get_server_version().map_err(LoginError)?;
        if !server_version.is_supported() {
            return Err(LoginError(anyhow!(
                "Immich server version 2.x.x, 3.0.2 or higher is required."
            )));
        }
        /* Immich does not need logging in. Check if the shared link is pointing to an album,
         * and if not, return LoginError so the app terminates. */
        self.get_my_shared_link_album().map_err(LoginError)?;
        Ok(())
    }

    fn get_photo_metadata(&self, sort_by: SortBy) -> Result<Vec<Self::Photo>> {
        let AlbumResponseDto { id, .. } = self.get_my_shared_link_album()?;
        let server_version = self.get_server_version()?;
        let mut assets = if server_version.major >= 3 {
            self.get_photo_metadata_v3(&id)
        } else {
            debug_assert_eq!(server_version.major, 2);
            self.get_photo_metadata_v2(&id)
        }?;
        Self::sort_assets(&mut assets, sort_by);
        Ok(assets)
    }

    fn get_exif(&self, Self::Photo { id, .. }: &Self::Photo) -> Result<impl Metadata> {
        let url = Url::parse(&format!("{}/assets/{id}", self.api_url))?;
        let response = self
            .http_client
            .get(url.as_str(), &[("key", &self.sharing_id)])?;
        read_response(response, HttpResponse::json::<GetAssetInfoResponseDto>)
    }

    fn get_photo_bytes(
        &self,
        Self::Photo { id, thumbhash, .. }: &Self::Photo,
        _: SourceSize,
    ) -> Result<Bytes> {
        let url = Url::parse(&format!("{}/assets/{id}/thumbnail", self.api_url))?;
        let response = self.http_client.get(
            url.as_str(),
            &[
                ("key", &self.sharing_id),
                ("size", "preview"),
                ("c", thumbhash),
            ],
        )?;
        read_response(response, HttpResponse::bytes)
    }
}

impl<H: HttpClient> ImmichApiClient<'_, H> {
    fn get_server_version(&self) -> Result<GetServerVersionResponseDto> {
        let url = Url::parse(&format!("{}/server/version", self.api_url))?;
        let response = self.http_client.get(url.as_str(), &[])?;
        read_response(response, HttpResponse::json)
    }

    fn get_my_shared_link_album(&self) -> Result<AlbumResponseDto> {
        let response = if let Some(password) = self.password {
            let url = Url::parse(&format!("{}/shared-links/login", self.api_url))?;
            self.http_client.post(
                url.as_str(),
                &[("password", password)],
                Query(&[("key", &self.sharing_id)]),
                None,
            )
        } else {
            let url = Url::parse(&format!("{}/shared-links/me", self.api_url))?;
            self.http_client
                .get(url.as_str(), &[("key", &self.sharing_id)])
        }?;
        read_response(response, |r| Ok(r.json::<SharedLinkResponseDto>()?.album))
    }

    fn get_photo_metadata_v2(&self, id: &str) -> Result<Vec<AssetResponseDto>> {
        let url = Url::parse(&format!("{}/albums/{id}", self.api_url))?;
        let response = self
            .http_client
            .get(url.as_str(), &[("key", &self.sharing_id)])?;
        read_response(response, |r| {
            Ok(r.json::<GetAlbumInfoResponseDto>()?.assets)
        })
    }

    fn get_photo_metadata_v3(&self, id: &str) -> Result<Vec<AssetResponseDto>> {
        let url = Url::parse(&format!("{}/search/metadata", self.api_url))?;
        let response = self.http_client.post_json(
            url.as_str(),
            &[("key", self.sharing_id.as_str())],
            &json!({"albumIds": [id]}),
        )?;
        read_response(response, |r| {
            Ok(r.json::<SearchAssetsResponseDto>()?.assets.items)
        })
    }

    fn sort_assets(assets: &mut [AssetResponseDto], sort_by: SortBy) {
        assets.sort_by(|a, b| {
            if matches!(sort_by, SortBy::TakenTime) {
                a.local_date_time.cmp(&b.local_date_time)
            } else {
                a.original_file_name.cmp(&b.original_file_name)
            }
        })
    }
}

impl<'a, H> ImmichApiClient<'a, H> {
    pub fn build(http_client: &'a H, share_link: &Url) -> Result<Self> {
        let (api_url, sharing_id) = parse_share_link(share_link)?;
        Ok(Self {
            http_client,
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

impl Metadata for GetAssetInfoResponseDto {
    fn date(&self) -> DateTime<Utc> {
        self.local_date_time
    }

    fn location(&self) -> Location {
        let ExifResponseDto { city, country, .. } = &self.exif_info;
        Location::new(city.clone(), country.clone())
    }
}

/// Returns Immich API URL and sharing id extracted from an album share link
fn parse_share_link(share_link: &Url) -> Result<(Url, SharingId)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(https?://.+)/share/([^/]+)/?$").unwrap());
    let Some(captures) = re.captures(share_link.as_str()) else {
        bail!("Invalid share link: {}", share_link)
    };
    let api_url = Url::parse(&format!("{}/api", &captures[1]))?;
    Ok((api_url, SharingId(captures[2].to_owned())))
}

impl GetServerVersionResponseDto {
    const V300: Self = Self {
        major: 3,
        minor: 0,
        patch: 0,
    };

    const V301: Self = Self {
        major: 3,
        minor: 0,
        patch: 1,
    };

    fn is_supported(&self) -> bool {
        self.major == 2 || (self.major == 3 && self != &Self::V300 && self != &Self::V301)
    }
}

mod dto {
    use chrono::{DateTime, Utc};
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    pub struct GetServerVersionResponseDto {
        pub major: u32,
        pub minor: u32,
        pub patch: u32,
    }

    #[derive(Debug, Deserialize)]
    pub struct SharedLinkResponseDto {
        pub album: AlbumResponseDto,
    }

    #[derive(Debug, Deserialize)]
    pub struct AlbumResponseDto {
        pub id: String,
    }

    /// Immich API v2.x.x returns assets from GET /albums/{id} endpoint
    #[derive(Debug, Deserialize)]
    pub struct GetAlbumInfoResponseDto {
        pub assets: Vec<AssetResponseDto>,
    }

    /// Immich API > v3.0.2 returns assets from POST /search/metadata endpoint
    #[derive(Debug, Deserialize)]
    pub struct SearchAssetsResponseDto {
        pub assets: SearchAssetResponseDto,
    }

    #[derive(Debug, Deserialize)]
    pub struct SearchAssetResponseDto {
        pub items: Vec<AssetResponseDto>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AssetResponseDto {
        pub id: String,
        /// Used for sorting by taken time
        pub original_file_name: String,
        /// Time adjusted to time-zone where the photo has been taken, used for sorting by taken
        /// date
        pub local_date_time: DateTime<Utc>,
        pub thumbhash: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct GetAssetInfoResponseDto {
        /// Time adjusted to time-zone where the photo has been taken, used for displaying on-screen
        /// info
        pub local_date_time: DateTime<Utc>,
        pub exif_info: ExifResponseDto,
    }

    #[derive(Debug, Deserialize)]
    pub struct ExifResponseDto {
        pub city: Option<String>,
        pub country: Option<String>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_share_link_is_ok_for_valid_link() {
        test_case(
            "http://test.immich.addr:2283/share/fake-Sharing-Id",
            "http://test.immich.addr:2283/api",
        );
        test_case(
            "https://test.immich.addr/fake-path/share/fake-Sharing-Id",
            "https://test.immich.addr/fake-path/api",
        );

        fn test_case(share_link: &str, expected_api_url: &str) {
            let link = Url::parse(share_link).unwrap();

            let result = parse_share_link(&link);

            assert!(result.is_ok());
            let (api_url, sharing_id) = result.unwrap();
            assert_eq!(api_url.as_str(), expected_api_url);
            assert_eq!(sharing_id.0, "fake-Sharing-Id");
        }
    }
}
