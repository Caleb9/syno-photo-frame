use std::sync::OnceLock;

use anyhow::{Result, bail};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use regex::Regex;

use crate::{
    LoginError,
    api_client::{
        ApiClient, Metadata, SharingId, SortBy,
        immich_client::dto::{
            AlbumInfo, AlbumResponseDto, AssetResponseDto, ExifResponseDto, MySharedLink,
        },
    },
    cli::SourceSize,
    http::{HttpClient, HttpResponse, Url, read_response},
    metadata::Location,
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
        /* Immich does not need logging in. Check if shared link is pointing to an album,
         * and if not, return LoginError so the app terminates. */
        self.get_my_shared_link_album().map_err(LoginError)?;
        Ok(())
    }

    fn get_photo_metadata(&self, sort_by: SortBy) -> Result<Vec<Self::Photo>> {
        let AlbumResponseDto { id, .. } = self.get_my_shared_link_album()?;
        let url = Url::parse(&format!("{}/albums/{id}", self.api_url))?;
        let response = self
            .http_client
            .get(url.as_str(), &[("key", &self.sharing_id)])?;
        read_response(response, |r| {
            let mut dto = r.json::<AlbumInfo>()?;
            Self::sort_assets(&mut dto.assets, sort_by);
            Ok(dto.assets)
        })
    }

    fn get_photo_bytes(
        &self,
        Self::Photo { id, .. }: &Self::Photo,
        _: SourceSize,
    ) -> Result<Bytes> {
        let url = Url::parse(&format!("{}/assets/{id}/thumbnail", self.api_url))?;
        let response = self.http_client.get(
            url.as_str(),
            &[("key", &self.sharing_id), ("size", "preview")],
        )?;
        read_response(response, |r| {
            let bytes = r.bytes()?;
            Ok(bytes)
        })
    }
}

impl<H: HttpClient> ImmichApiClient<'_, H> {
    fn get_my_shared_link_album(&self) -> Result<AlbumResponseDto> {
        let url = Url::parse(&format!("{}/shared-links/me", self.api_url))?;
        let response = self.http_client.get(
            url.as_str(),
            &[
                ("key", &self.sharing_id),
                ("password", self.password.as_deref().unwrap_or_default()),
            ],
        )?;
        read_response(response, |r| {
            let dto = r.json::<MySharedLink>()?;
            Ok(dto.album)
        })
    }

    fn sort_assets(assets: &mut [AssetResponseDto], sort_by: SortBy) {
        assets.sort_by(|a, b| {
            if matches!(sort_by, SortBy::TakenTime) {
                a.exif_info
                    .date_time_original
                    .cmp(&b.exif_info.date_time_original)
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

impl Metadata for AssetResponseDto {
    fn date(&self) -> DateTime<Utc> {
        self.local_date_time
    }

    fn location(&self) -> Location {
        let ExifResponseDto { city, country, .. } = &self.exif_info;
        Location::new(city.clone(), country.clone())
    }
}

/// Returns Immich API URL and sharing id extracted from album share link
fn parse_share_link(share_link: &Url) -> Result<(Url, SharingId)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(https?://.+)/share/([^/]+)/?$").unwrap());
    let Some(captures) = re.captures(share_link.as_str()) else {
        bail!("Invalid share link: {}", share_link)
    };
    let api_url = Url::parse(&format!("{}/api", &captures[1]))?;
    Ok((api_url, SharingId(captures[2].to_owned())))
}

mod dto {
    use chrono::{DateTime, Utc};
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct MySharedLink {
        pub album: AlbumResponseDto,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AlbumResponseDto {
        pub id: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct AlbumInfo {
        pub assets: Vec<AssetResponseDto>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AssetResponseDto {
        pub id: String,
        /// Used for sorting by taken time
        pub original_file_name: String,
        /// Time adjusted to timezone where photo has been taken, used for displaying on screen
        pub local_date_time: DateTime<Utc>,
        pub exif_info: ExifResponseDto,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ExifResponseDto {
        pub date_time_original: Option<DateTime<Utc>>,
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
