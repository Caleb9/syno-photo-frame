use anyhow::Result;
use mockall::mock;
use serde::de::DeserializeOwned;
use syno_api::{
    dto::ApiResponse,
    foto::browse::item::dto::Item,
    foto::browse::item::dto::{Additional, Thumbnail},
};

use crate::http::{self, CookieStore, Jar, MockHttpResponse, StatusCode, Url};

mock! {
    pub HttpClient {}

    impl http::HttpClient for HttpClient {
        type Response = MockHttpResponse;

        fn post<'a>(
            &self,
            url: &str,
            form: &[(&'a str, &'a str)],
            header: Option<(&'a str, &'a str)>,
        ) -> Result<MockHttpResponse>;

        fn get<'a>(&self, url: &str, query: &[(&'a str, &'a str)]) -> Result<MockHttpResponse>;
    }
}

/// When `is_logged_in_to_url` is set to Some value, cookie store will simulate logged in state
pub fn new_cookie_store(is_logged_in_to_url: Option<&str>) -> impl CookieStore {
    let cookie_store = Jar::default();
    if let Some(url) = is_logged_in_to_url {
        cookie_store.add_cookie_str("sharing_id=FakeSharingId", &Url::parse(url).unwrap());
    }
    cookie_store
}

pub fn new_ok_response() -> MockHttpResponse {
    let mut response = MockHttpResponse::new();
    response.expect_status().return_const(StatusCode::OK);
    response
}

pub fn new_success_response_with_json<T: DeserializeOwned + Send + 'static>(
    data: T,
) -> MockHttpResponse {
    let mut response = new_ok_response();
    response.expect_json::<ApiResponse<T>>().return_once(|| {
        Ok(ApiResponse {
            success: true,
            error: None,
            data: Some(data),
        })
    });
    response
}

pub fn new_photo_dto(id: u32, cache_key: &str) -> Item {
    Item {
        id,
        additional: Some(Additional {
            thumbnail: Some(Thumbnail {
                cache_key: cache_key.to_string(),
            }),
        }),
        ..Default::default()
    }
}

pub fn is_login_form(form: &[(&str, &str)], sharing_id: &str) -> bool {
    form.eq(&[
        ("api", "SYNO.Core.Sharing.Login"),
        ("method", "login"),
        ("version", "1"),
        ("sharing_id", sharing_id),
        ("password", ""),
    ])
}

pub fn is_get_count_form(form: &[(&str, &str)]) -> bool {
    form.eq(&[
        ("api", syno_api::foto::browse::album::API),
        ("method", "get"),
        ("version", "1"),
    ])
}
