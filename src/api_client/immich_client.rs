use std::sync::OnceLock;

use anyhow::{bail, Result};
use bytes::Bytes;
use regex::Regex;

use crate::{
    api_client::{
        immich_client::dto::{Album, AlbumInfo, Asset, AssetsInfo},
        ApiClient, SharingId, SortBy,
    },
    cli::SourceSize,
    http::{read_response, HttpClient, HttpResponse, Url},
    LoginError,
};

pub struct ImmichApiClient<'a, H> {
    http_client: &'a H,
    api_url: Url,
    sharing_id: SharingId,
    password: &'a Option<String>,
}

impl<H: HttpClient> ApiClient for ImmichApiClient<'_, H> {
    type Photo = Asset;

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
        let Album { id, .. } = self.get_my_shared_link_album()?;
        let url = Url::parse(&format!("{}/albums/{id}", self.api_url))?;
        let response = self
            .http_client
            .get(url.as_str(), &[("key", &self.sharing_id)])?;
        read_response(response, |r| {
            let mut dto = r.json::<AssetsInfo>()?;
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
    fn get_my_shared_link_album(&self) -> Result<Album> {
        let url = Url::parse(&format!("{}/shared-links/me", self.api_url))?;
        let response = self.http_client.get(
            url.as_str(),
            &[
                ("key", &self.sharing_id),
                ("password", self.password.as_deref().unwrap_or_default()),
            ],
        )?;
        read_response(response, |r| {
            let dto = r.json::<AlbumInfo>()?;
            Ok(dto.album)
        })
    }

    fn sort_assets(assets: &mut [Asset], sort_by: SortBy) {
        assets.sort_by(|a, b| match (&a.exif_info, &b.exif_info) {
            (Some(a_exif), Some(b_exif)) if matches!(sort_by, SortBy::TakenTime) => {
                a_exif.date_time_original.cmp(&b_exif.date_time_original)
            }
            _ => a.original_file_name.cmp(&b.original_file_name),
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
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct AlbumInfo {
        pub album: Album,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Album {
        pub id: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct AssetsInfo {
        pub assets: Vec<Asset>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Asset {
        pub id: String,
        pub original_file_name: String,
        pub exif_info: Option<ExifInfo>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ExifInfo {
        pub date_time_original: String,
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
