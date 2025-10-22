use std::{fmt, ops::Deref, sync::OnceLock};

use anyhow::{Result, bail};
use bytes::Bytes;
use regex::Regex;

use crate::{
    cli::{Backend, Order, SourceSize},
    http::Url,
    metadata::Metadata,
};

pub mod immich_client;
pub mod syno_client;

pub trait ApiClient {
    type Photo: Metadata + Send;

    fn is_logged_in(&self) -> bool;

    fn login(&self) -> Result<(), LoginError>;

    fn get_photo_metadata(&self, sort_by: SortBy) -> Result<Vec<Self::Photo>>;

    fn get_photo_bytes(&self, photo: &Self::Photo, source_size: SourceSize) -> Result<Bytes>;
}

#[derive(Debug)]
pub struct LoginError(pub anyhow::Error);

impl fmt::Display for LoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for LoginError {}

#[derive(Debug)]
struct SharingId(String);

impl Deref for SharingId {
    type Target = String;

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

impl fmt::Display for SortBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

pub fn detect_backend(share_link: &Url) -> Result<Backend> {
    static SYNO_LINK_RE: OnceLock<Regex> = OnceLock::new();
    let syno_link_re = SYNO_LINK_RE
        .get_or_init(|| Regex::new(r"^https?://.+/[[:word:]]{2}/sharing/[^/]+/?$").unwrap());
    if syno_link_re.is_match(share_link.as_str()) {
        return Ok(Backend::Synology);
    }

    static IMMICH_LINK_RE: OnceLock<Regex> = OnceLock::new();
    let immich_link_re =
        IMMICH_LINK_RE.get_or_init(|| Regex::new(r"^https?://.+/share/[^/]+/?$").unwrap());
    if immich_link_re.is_match(share_link.as_str()) {
        return Ok(Backend::Immich);
    }

    bail!(
        "Unable to detect the backend type from share link. \
         Try specifying the type explicitly using the --backend option"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_synology_share_link_with_alias_then_detect_backend_returns_synology() {
        const SHARE_LINK: &str = "https://fake.dsm/photo/mo/sharing/fakeSharingId";

        let result = detect_backend(&Url::parse(SHARE_LINK).unwrap());

        assert!(matches!(result, Ok(Backend::Synology)));
    }

    #[test]
    fn when_synology_share_link_with_port_then_detect_backend_returns_synology() {
        const SHARE_LINK: &str = "http://fake.dsm:5000/mo/sharing/fakeSharingId";

        let result = detect_backend(&Url::parse(SHARE_LINK).unwrap());

        assert!(matches!(result, Ok(Backend::Synology)));
    }

    #[test]
    fn when_immich_share_link_without_path_then_detect_backend_returns_immich() {
        const SHARE_LINK: &str = "http://fake.immich:2283/share/fake-sharing-link";

        let result = detect_backend(&Url::parse(SHARE_LINK).unwrap());

        assert!(matches!(result, Ok(Backend::Immich)));
    }

    #[test]
    fn when_immich_share_link_with_path_then_detect_backend_returns_immich() {
        const SHARE_LINK: &str = "https://fake.immich/some-path/share/fake-sharing-link";

        let result = detect_backend(&Url::parse(SHARE_LINK).unwrap());

        assert!(matches!(result, Ok(Backend::Immich)));
    }

    #[test]
    fn when_invalid_share_link_then_detect_backend_returns_error() {
        const SHARE_LINK: &str = "http://fake.backend/unknown/path/fake-sharing-link";

        let result = detect_backend(&Url::parse(SHARE_LINK).unwrap());

        assert!(result.is_err());
    }
}
