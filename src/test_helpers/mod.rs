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

pub mod rand {
    use crate::rand::Random;
    use std::cell::RefCell;
    use std::ops::Range;

    #[derive(Debug, Default)]
    pub struct FakeRandom {
        random_sequence: RefCell<Vec<u32>>,
        shuffle_swap_sequence: Vec<(usize, usize)>,
    }

    impl FakeRandom {
        pub fn with_random_sequence(self, sequence: Vec<u32>) -> Self {
            self.random_sequence.replace(sequence);
            self.random_sequence.borrow_mut().reverse();
            self
        }

        pub fn with_shuffle_result(mut self, swaps: Vec<(usize, usize)>) -> Self {
            self.shuffle_swap_sequence = swaps;
            self
        }
    }

    impl Random for FakeRandom {
        fn gen_range(&self, _: Range<u32>) -> u32 {
            self.random_sequence
                .borrow_mut()
                .pop()
                .expect("should not be empty")
        }

        fn shuffle<T>(&self, slice: &mut [T]) {
            self.shuffle_swap_sequence
                .iter()
                .for_each(|s| slice.swap(s.0, s.1));
        }
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

pub fn is_list_form(form: &[(&str, &str)]) -> bool {
    form.eq(&[
        ("api", syno_api::foto::browse::item::API),
        ("method", "list"),
        ("version", "1"),
        ("additional", "[\"thumbnail\"]"),
        ("offset", "0"),
        ("limit", "5000"),
        ("sort_by", "takentime"),
        ("sort_direction", "asc"),
    ])
}

pub fn is_get_photo_form(
    form: &[(&str, &str)],
    sharing_id: &str,
    photo_id: &str,
    cache_key: &str,
    size: &str,
) -> bool {
    form.eq(&[
        ("api", "SYNO.Foto.Thumbnail"),
        ("method", "get"),
        ("version", "2"),
        ("_sharing_id", sharing_id),
        ("id", photo_id),
        ("cache_key", cache_key),
        ("type", "unit"),
        ("size", size),
    ])
}
