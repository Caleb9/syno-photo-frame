use std::sync::Arc;

use mockall::mock;
use serde::de::DeserializeOwned;

use crate::{
    api::dto,
    http::{Client, CookieStore, Jar, MockResponse, StatusCode, Url},
};

mock! {
    pub(crate) Client {}

    impl Client for Client {
        type Response = MockResponse;

        fn post<'a>(
            &self,
            url: &str,
            form: &[(&'a str, &'a str)],
            header: Option<(&'a str, &'a str)>,
        ) -> Result<MockResponse, String>;
        fn get<'a>(&self, url: &str, query: &[(&'a str, &'a str)]) -> Result<MockResponse, String>;
    }

    impl Clone for Client {
        fn clone(&self) -> Self;
    }
}

/// When `is_logged_in_to_url` is set to Some value, cookie store will simulate logged in state
pub(crate) fn new_cookie_store(is_logged_in_to_url: Option<&str>) -> Arc<dyn CookieStore> {
    let cookie_store = Arc::new(Jar::default());
    if let Some(url) = is_logged_in_to_url {
        cookie_store.add_cookie_str("sharing_id=FakeSharingId", &Url::parse(url).unwrap());
    }
    cookie_store as Arc<dyn CookieStore>
}

pub(crate) fn new_success_response() -> MockResponse {
    let mut response = MockResponse::new();
    response.expect_status().returning(|| StatusCode::OK);
    response
}

pub(crate) fn new_response_with_json<T: DeserializeOwned + Send + 'static>(
    data: T,
) -> MockResponse {
    let mut response = new_success_response();
    response
        .expect_json::<dto::ApiResponse<T>>()
        .return_once(|| {
            Ok(dto::ApiResponse {
                success: true,
                error: None,
                data: Some(data),
            })
        });
    response
}

pub(crate) fn new_photo_dto(id: i32, cache_key: &str) -> dto::Photo {
    dto::Photo {
        id,
        additional: dto::Additional {
            thumbnail: dto::Thumbnail {
                cache_key: cache_key.to_string(),
            },
        },
    }
}
