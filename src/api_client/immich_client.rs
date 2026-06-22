use std::sync::OnceLock;

use crate::{
    LoginError,
    api_client::{
        ApiClient, Metadata, SharingId, SortBy,
        immich_client::dto::{
            AlbumResponseDto, AssetResponseDto, BucketAssetsDto, BucketResponseDto,
            ExifResponseDto, MySharedLink,
        },
    },
    cli::SourceSize,
    http::{HttpClient, HttpResponse, Url, read_response},
    metadata::Location,
};
use anyhow::{Result, bail};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use regex::Regex;
use reqwest::cookie::CookieStore;

pub struct ImmichApiClient<'a, H, C> {
    http_client: &'a H,
    cookie_store: &'a C,
    api_url: Url,
    sharing_id: SharingId,
    password: &'a Option<String>,
}

impl<H: HttpClient, C: CookieStore> ApiClient for ImmichApiClient<'_, H, C> {
    type Photo = AssetResponseDto;

    fn is_logged_in(&self) -> bool {
        // TODO: Immich sets cookies with expiration time of 1 day. How to check if the cookie is
        // still valid for a large album which doesn't reinitialize before the expiration time?
        self.cookie_store.cookies(&self.api_url).is_some()
    }

    fn login(&self) -> Result<(), LoginError> {
        self.get_my_shared_link_album().map_err(LoginError)?;
        Ok(())
    }

    fn get_photo_metadata(&self, sort_by: SortBy) -> Result<Vec<Self::Photo>> {
        let AlbumResponseDto { id, .. } = self.get_my_shared_link_album()?;
        let url = Url::parse(&format!("{}/timeline/buckets", self.api_url))?;
        let (key, album_id, order) = (
            ("key", self.sharing_id.as_str()),
            ("albumId", id.as_str()),
            ("order", "desc"),
        );
        let response = self
            .http_client
            .get(url.as_str(), &[album_id, key, order])?;
        let buckets: Vec<BucketResponseDto> = read_response(response, HttpResponse::json)?;
        let bucket_assets: Vec<BucketAssetsDto> = buckets
            .iter()
            .map(|b| {
                let url = Url::parse(&format!("{}/timeline/bucket", self.api_url))?;
                let response = self.http_client.get(
                    url.as_str(),
                    &[album_id, key, order, ("timeBucket", &b.time_bucket)],
                )?;
                read_response(response, HttpResponse::json)
            })
            .collect::<Result<_>>()?;
        let mut assets: Vec<AssetResponseDto> = bucket_assets
            .iter()
            .flat_map(|a| a.id.iter())
            .map(|id| {
                /* TODO: this is pretty inefficient, there's a separate request sent sequentially
                 * for each asset in the album! Find out if it's possible to batch them together. */
                let url = Url::parse(&format!("{}/assets/{id}", self.api_url))?;
                let response = self.http_client.get(url.as_str(), &[key])?;
                read_response(response, HttpResponse::json)
            })
            .collect::<Result<_>>()?;
        Self::sort_assets(&mut assets, sort_by);
        Ok(assets)
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

impl<H: HttpClient, C> ImmichApiClient<'_, H, C> {
    fn get_my_shared_link_album(&self) -> Result<AlbumResponseDto> {
        let response = if let Some(password) = self.password {
            let url = Url::parse(&format!("{}/shared-links/login", self.api_url))?;
            self.http_client.post(
                url.as_str(),
                &[("password", password)],
                Some(&[("key", &self.sharing_id)]),
                None,
            )?
        } else {
            let url = Url::parse(&format!("{}/shared-links/me", self.api_url))?;
            self.http_client
                .get(url.as_str(), &[("key", &self.sharing_id)])?
        };
        read_response(response, |r| Ok(r.json::<MySharedLink>()?.album))
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

impl<'a, H, C> ImmichApiClient<'a, H, C> {
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
    #[serde(rename_all = "camelCase")]
    pub struct BucketResponseDto {
        pub time_bucket: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct BucketAssetsDto {
        pub id: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AssetResponseDto {
        pub id: String,
        /// Used for sorting by taken time
        pub original_file_name: String,
        /// Time adjusted to time-zone where the photo has been taken, used for displaying on-screen
        /// info
        pub local_date_time: DateTime<Utc>,
        pub exif_info: ExifResponseDto,
        pub thumbhash: String,
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
