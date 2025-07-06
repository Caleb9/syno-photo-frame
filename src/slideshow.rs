use std::time::Duration;

#[cfg(test)]
use crate::test_helpers::fake_sleep as thread_sleep;
#[cfg(not(test))]
use std::thread::sleep as thread_sleep;

use anyhow::{Result, bail};
use bytes::Bytes;

use crate::{
    api_client::ApiClient,
    cli::{Order, SourceSize},
    http::{InvalidHttpResponse, StatusCode},
    rand::Random,
};

/// Holds the slideshow state and queries API to fetch photos.
#[derive(Debug)]
pub struct Slideshow<A: ApiClient, R> {
    api_client: A,
    random: R,
    /// Album photos' metadata in reverse order (so we can pop them off easily)
    photo_display_sequence: Vec<A::Photo>,
    order: Order,
    random_start: bool,
    source_size: SourceSize,
}

impl<A: ApiClient, R: Random> Slideshow<A, R> {
    pub fn new(api_client: A, random: R) -> Self {
        Self {
            api_client,
            random,
            photo_display_sequence: vec![],
            order: Order::ByDate,
            random_start: false,
            source_size: SourceSize::L,
        }
    }

    pub fn with_ordering(mut self, order: Order) -> Self {
        self.order = order;
        self
    }

    pub fn with_random_start(mut self, random_start: bool) -> Self {
        self.random_start = random_start;
        self
    }

    pub fn with_source_size(mut self, size: SourceSize) -> Self {
        self.source_size = size;
        self
    }

    pub fn get_next_photo(&mut self) -> Result<Bytes> {
        const LOOP_SLEEP_DURATION: Duration = Duration::from_secs(1);
        /* Loop here prevents display of error screen when the photo has simply been removed from
         * the album since we fetched its metadata. */
        loop {
            if self.slideshow_ended() {
                self.initialize()?;
            }

            let photo = self
                .photo_display_sequence
                .pop()
                .expect("photos should not be empty");
            let photo_bytes_result = self.api_client.get_photo_bytes(&photo, self.source_size);
            match photo_bytes_result {
                Err(error) if photo_removed(&error) => {
                    log::warn!("{error}");
                    /* Save on CPU and request flooding */
                    thread_sleep(LOOP_SLEEP_DURATION);
                    continue;
                }
                _ => break photo_bytes_result,
            }
        }
    }

    fn slideshow_ended(&self) -> bool {
        self.photo_display_sequence.is_empty()
    }

    fn initialize(&mut self) -> Result<()> {
        assert!(
            self.photo_display_sequence.is_empty(),
            "already initialized"
        );
        let photos = self.api_client.get_photo_metadata(self.order.into())?;
        if photos.is_empty() {
            bail!("Album is empty");
        }
        let item_count = photos.len();
        self.photo_display_sequence.reserve(item_count);
        match self.order {
            Order::ByDate | Order::ByName if self.random_start => {
                self.photo_display_sequence.extend(
                    photos
                        .into_iter()
                        .skip(self.random.random_range(0..item_count as u32) as usize)
                        .rev(),
                );
                /* RandomStart is only used when slideshow starts, and afterward continues in normal
                 * order */
                self.random_start = false;
            }
            Order::ByDate | Order::ByName => {
                self.photo_display_sequence.extend(photos.into_iter().rev())
            }
            Order::Random => {
                self.photo_display_sequence.extend(photos);
                self.random.shuffle(&mut self.photo_display_sequence);
            }
        }
        Ok(())
    }
}

/// Photo has been removed since we fetched its metadata, try next one.
fn photo_removed(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<InvalidHttpResponse>(),
        Some(InvalidHttpResponse(StatusCode::NOT_FOUND))
    )
}

/// These tests cover both `slideshow` and `api_client::syno_client` modules
#[cfg(test)]
mod tests {
    use super::*;

    use syno_api::dto::List;

    use crate::{
        api_client::syno_client::SynoApiClient,
        http::{CookieStore, HttpClient, Jar, MockHttpResponse, Url},
        test_helpers::rand::FakeRandom,
        test_helpers::{self, MockHttpClient},
    };

    #[test]
    fn when_default_order_then_get_next_photo_fetches_first_photo() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        const EXPECTED_THUMBNAIL_API_URL: &str =
            "http://fake.dsm.addr/synofoto/api/v2/p/Thumbnail/get";
        let mut client_mock = MockHttpClient::new();
        const FIRST_PHOTO_ID: u32 = 1;
        const FIRST_PHOTO_CACHE_KEY: &str = "photo1";
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == EXPECTED_API_URL
                    && test_helpers::is_list_form(form)
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![
                        test_helpers::new_photo_dto(FIRST_PHOTO_ID, FIRST_PHOTO_CACHE_KEY),
                        test_helpers::new_photo_dto(2, "photo2"),
                        test_helpers::new_photo_dto(3, "photo3"),
                    ],
                }))
            });
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == EXPECTED_THUMBNAIL_API_URL
                    && test_helpers::is_get_photo_form(
                        query,
                        "FakeSharingId",
                        &FIRST_PHOTO_ID.to_string(),
                        FIRST_PHOTO_CACHE_KEY,
                        "xl",
                    )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                Ok(get_photo_response)
            });
        let cookie_store = Jar::default();
        let mut slideshow = new_syno_slideshow(
            &client_mock,
            FakeRandom::default(),
            &cookie_store,
            SHARE_LINK,
        );

        /* Act */
        let result = slideshow.get_next_photo();

        /* Assert */
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Bytes::from_static(&[42, 1, 255, 50]));

        let expected_remaining_display_sequence = [
            test_helpers::new_photo_dto(3, "photo3"),
            test_helpers::new_photo_dto(2, "photo2"),
        ];
        assert_eq!(
            slideshow.photo_display_sequence,
            expected_remaining_display_sequence
        );

        client_mock.checkpoint();
    }

    #[test]
    fn when_random_start_then_get_next_photo_fetches_random_photo() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        let mut client_mock = MockHttpClient::new();
        const FAKE_RANDOM_NUMBER: u32 = 2;
        const RANDOM_PHOTO_ID: u32 = 43;
        const RANDOM_PHOTO_CACHE_KEY: &str = "photo43";
        client_mock
            .expect_post()
            .withf(|_, form, _| test_helpers::is_list_form(form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![
                        test_helpers::new_photo_dto(1, "photo1"),
                        test_helpers::new_photo_dto(2, "photo2"),
                        test_helpers::new_photo_dto(RANDOM_PHOTO_ID, RANDOM_PHOTO_CACHE_KEY),
                    ],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| {
                test_helpers::is_get_photo_form(
                    query,
                    "FakeSharingId",
                    &RANDOM_PHOTO_ID.to_string(),
                    RANDOM_PHOTO_CACHE_KEY,
                    "xl",
                )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                Ok(get_photo_response)
            });

        let random_mock = FakeRandom::default().with_random_sequence(vec![FAKE_RANDOM_NUMBER]);
        let cookie_store = Jar::default();
        let mut slideshow =
            new_syno_slideshow(&client_mock, random_mock, &cookie_store, SHARE_LINK)
                .with_random_start(true);

        /* Act */
        let result = slideshow.get_next_photo();

        /* Assert */
        assert!(result.is_ok());
        client_mock.checkpoint();
    }

    #[test]
    fn when_source_size_specified_then_get_next_photo_fetches_photo_of_specific_size() {
        test_case(SourceSize::S, "sm");
        test_case(SourceSize::M, "m");
        test_case(SourceSize::L, "xl");

        fn test_case(source_size: SourceSize, expected_size_param: &'static str) {
            /* Arrange */
            const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
            let mut client_mock = MockHttpClient::new();
            client_mock
                .expect_post()
                .withf(|_, form, _| test_helpers::is_list_form(form))
                .return_once(|_, _, _| {
                    Ok(test_helpers::new_success_response_with_json(List {
                        list: vec![test_helpers::new_photo_dto(43, "photo43")],
                    }))
                });
            client_mock
                .expect_get()
                .withf(move |_, query| {
                    test_helpers::is_get_photo_form(
                        query,
                        "FakeSharingId",
                        "43",
                        "photo43",
                        expected_size_param,
                    )
                })
                .return_once(|_, _| {
                    let mut get_photo_response = test_helpers::new_ok_response();
                    get_photo_response
                        .expect_bytes()
                        .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                    Ok(get_photo_response)
                });
            let cookie_store = Jar::default();
            let mut slideshow = new_syno_slideshow(
                &client_mock,
                FakeRandom::default(),
                &cookie_store,
                SHARE_LINK,
            )
            .with_source_size(source_size);

            /* Act */
            let result = slideshow.get_next_photo();

            /* Assert */
            assert!(result.is_ok());
            client_mock.checkpoint();
        }
    }

    #[test]
    fn get_next_photo_advances_to_next_photo() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        const NEXT_PHOTO_ID: u32 = 3;
        const NEXT_PHOTO_CACHE_KEY: &str = "photo3";
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == "http://fake.dsm.addr/synofoto/api/v2/p/Thumbnail/get"
                    && test_helpers::is_get_photo_form(
                        query,
                        "FakeSharingId",
                        &NEXT_PHOTO_ID.to_string(),
                        NEXT_PHOTO_CACHE_KEY,
                        "xl",
                    )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });
        let cookie_store = logged_in_cookie_store(EXPECTED_API_URL);
        let mut slideshow = new_syno_slideshow(
            &client_mock,
            FakeRandom::default(),
            &cookie_store,
            SHARE_LINK,
        );
        slideshow.photo_display_sequence = vec![
            test_helpers::new_photo_dto(1, "photo1"),
            test_helpers::new_photo_dto(NEXT_PHOTO_ID, NEXT_PHOTO_CACHE_KEY),
        ];

        /* Act */
        let result = slideshow.get_next_photo();

        /* Assert */
        assert!(result.is_ok());
        assert_eq!(
            slideshow.photo_display_sequence,
            vec![test_helpers::new_photo_dto(1, "photo1")]
        );
    }

    #[test]
    fn get_next_photo_skips_the_photo_when_cached_dto_is_not_found_because_photo_was_removed_from_album()
     {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        const MISSING_PHOTO_ID: u32 = 1;
        const MISSING_PHOTO_CACHE_KEY: &str = "missing_photo";
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_get()
            .withf(|_, query| {
                test_helpers::is_get_photo_form(
                    query,
                    "FakeSharingId",
                    &MISSING_PHOTO_ID.to_string(),
                    MISSING_PHOTO_CACHE_KEY,
                    "xl",
                )
            })
            .return_once(|_, _| {
                let mut not_found_response = MockHttpResponse::new();
                not_found_response
                    .expect_status()
                    .returning(|| StatusCode::NOT_FOUND);
                Ok(not_found_response)
            });
        const NEXT_PHOTO_ID: u32 = 2;
        const NEXT_PHOTO_CACHE_KEY: &str = "photo2";
        client_mock
            .expect_get()
            .withf(|_, query| {
                test_helpers::is_get_photo_form(
                    query,
                    "FakeSharingId",
                    &NEXT_PHOTO_ID.to_string(),
                    NEXT_PHOTO_CACHE_KEY,
                    "xl",
                )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });

        const NEXT_NEXT_PHOTO_ID: u32 = 3;
        const NEXT_NEXT_PHOTO_CACHE_KEY: &str = "photo3";
        let cookie_store = logged_in_cookie_store(EXPECTED_API_URL);
        let mut slideshow = new_syno_slideshow(
            &client_mock,
            FakeRandom::default(),
            &cookie_store,
            SHARE_LINK,
        );
        slideshow.photo_display_sequence = vec![
            test_helpers::new_photo_dto(NEXT_NEXT_PHOTO_ID, NEXT_NEXT_PHOTO_CACHE_KEY),
            test_helpers::new_photo_dto(NEXT_PHOTO_ID, NEXT_PHOTO_CACHE_KEY),
            test_helpers::new_photo_dto(MISSING_PHOTO_ID, MISSING_PHOTO_CACHE_KEY),
        ];

        /* Act */
        let result = slideshow.get_next_photo();

        /* Assert */
        assert!(result.is_ok());
        assert_eq!(
            slideshow.photo_display_sequence,
            vec![test_helpers::new_photo_dto(
                NEXT_NEXT_PHOTO_ID,
                NEXT_NEXT_PHOTO_CACHE_KEY
            )]
        );
    }

    #[test]
    fn when_random_order_then_photo_display_sequence_is_shuffled() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| test_helpers::is_list_form(form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![
                        test_helpers::new_photo_dto(0, "photo0"),
                        test_helpers::new_photo_dto(1, "photo1"),
                        test_helpers::new_photo_dto(2, "photo2"),
                        test_helpers::new_photo_dto(3, "photo3"),
                        test_helpers::new_photo_dto(4, "photo4"),
                    ],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| {
                test_helpers::is_get_photo_form(query, "FakeSharingId", "1", "photo1", "xl")
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                Ok(get_photo_response)
            });

        let random_mock = FakeRandom::default().with_shuffle_result(vec![
            (0, 3), /* 3 1 2 0 4 */
            (1, 4), /* 3 4 2 0 1 */
        ]);

        let cookie_store = Jar::default();
        let mut slideshow =
            new_syno_slideshow(&client_mock, random_mock, &cookie_store, SHARE_LINK)
                .with_ordering(Order::Random);

        /* Act */
        let result = slideshow.get_next_photo();

        assert!(result.is_ok());
        assert_eq!(
            slideshow.photo_display_sequence,
            vec![
                test_helpers::new_photo_dto(3, "photo3"),
                test_helpers::new_photo_dto(4, "photo4"),
                test_helpers::new_photo_dto(2, "photo2"),
                test_helpers::new_photo_dto(0, "photo0"),
                // photo1 popped
            ]
        );
    }

    /// Tests that when photos were removed, slideshow gets re-initialized when reaching the end of the album
    #[test]
    fn get_next_photo_reinitializes_when_display_sequence_is_empty() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        let mut client_mock = MockHttpClient::new();

        const FIRST_PHOTO_ID: u32 = 1;
        const SECOND_PHOTO_ID: u32 = 1;
        const FIRST_PHOTO_CACHE_KEY: &str = "photo1";
        const SECOND_PHOTO_CACHE_KEY: &str = "photo1";
        client_mock
            .expect_post()
            .withf(|_, form, _| test_helpers::is_list_form(form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![
                        test_helpers::new_photo_dto(FIRST_PHOTO_ID, FIRST_PHOTO_CACHE_KEY),
                        test_helpers::new_photo_dto(SECOND_PHOTO_ID, SECOND_PHOTO_CACHE_KEY),
                    ],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| {
                test_helpers::is_get_photo_form(
                    query,
                    "FakeSharingId",
                    &FIRST_PHOTO_ID.to_string(),
                    FIRST_PHOTO_CACHE_KEY,
                    "xl",
                )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });
        let cookie_store = logged_in_cookie_store(EXPECTED_API_URL);
        let mut slideshow = new_syno_slideshow(
            &client_mock,
            FakeRandom::default(),
            &cookie_store,
            SHARE_LINK,
        );
        slideshow.photo_display_sequence = vec![];

        /* Act */
        let result = slideshow.get_next_photo();

        /* Assert */
        assert!(result.is_ok());
        assert_eq!(
            slideshow.photo_display_sequence,
            vec![test_helpers::new_photo_dto(
                SECOND_PHOTO_ID,
                SECOND_PHOTO_CACHE_KEY,
            )]
        );
    }

    fn new_syno_slideshow<'a, H: HttpClient, C: CookieStore, R: Random>(
        http_client: &'a H,
        random: R,
        cookie_store: &'a C,
        share_link: &str,
    ) -> Slideshow<SynoApiClient<'a, H, C>, R> {
        let share_link = Url::parse(share_link).unwrap();
        let api_client = SynoApiClient::build(http_client, cookie_store, &share_link).unwrap();
        Slideshow::new(api_client, random)
    }

    fn logged_in_cookie_store(url: &str) -> impl CookieStore {
        test_helpers::new_cookie_store(Some(url))
    }
}
