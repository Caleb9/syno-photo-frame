use std::sync::OnceLock;

use bytes::Bytes;
use regex::Regex;

use crate::api_photos;
use crate::api_photos::{dto, Limit, PhotosApiError, SharingId, SortBy};
use crate::cli::SourceSize;
use crate::error::ErrorToString;
use crate::http::{CookieStore, HttpClient, Url};

pub trait ApiClient {
    fn is_logged_in(&self) -> bool;

    fn login(&self) -> Result<(), PhotosApiError>;

    fn get_album_contents_count(&self) -> Result<Vec<dto::Album>, PhotosApiError>;

    fn get_album_contents(
        &self,
        offset: u32,
        limit: Limit,
        sort_by: SortBy,
    ) -> Result<Vec<dto::Photo>, PhotosApiError>;

    fn get_photo(&self, metadata: (i32, &str, SourceSize)) -> Result<Bytes, PhotosApiError>;
}

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

    fn login(&self) -> Result<(), PhotosApiError> {
        api_photos::login(
            self.http_client,
            &self.api_url,
            &self.sharing_id,
            self.password,
        )
    }

    fn get_album_contents_count(&self) -> Result<Vec<dto::Album>, PhotosApiError> {
        api_photos::get_album_contents_count(self.http_client, &self.api_url, &self.sharing_id)
    }

    fn get_album_contents(
        &self,
        offset: u32,
        limit: Limit,
        sort_by: SortBy,
    ) -> Result<Vec<dto::Photo>, PhotosApiError> {
        api_photos::get_album_contents(
            self.http_client,
            &self.api_url,
            &self.sharing_id,
            offset,
            limit,
            sort_by,
        )
    }

    fn get_photo(&self, metadata: (i32, &str, SourceSize)) -> Result<Bytes, PhotosApiError> {
        api_photos::get_photo(self.http_client, &self.api_url, &self.sharing_id, metadata)
    }
}

impl<'a, H: HttpClient, C: CookieStore> SynoApiClient<'a, H, C> {
    pub fn build(
        http_client: &'a H,
        cookie_store: &'a C,
        share_link: &Url,
    ) -> Result<Self, String> {
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
fn parse_share_link(share_link: &Url) -> Result<(Url, SharingId), String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(https?://.+)/([^/]+)/?$").unwrap());
    let Some(captures) = re.captures(share_link.as_str()) else {
        return Err(format!("Invalid share link: {share_link}"));
    };
    let api_url = Url::parse(&format!("{}/webapi/entry.cgi", &captures[1])).map_err_to_string()?;
    Ok((api_url, SharingId(captures[2].to_owned())))
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
