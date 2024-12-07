use anyhow::{anyhow, bail, Result};
use bytes::Bytes;
use syno_api::foto::browse::item::dto::Item;

use crate::{
    api_client::{ApiClient, InvalidHttpResponse},
    cli::{Order, SourceSize},
    http::StatusCode,
    Random,
};

/// Holds the slideshow state and queries API to fetch photos.
#[derive(Debug)]
pub struct Slideshow<A> {
    api_client: A,
    /// Indices of photos in an album in reverse order (so we can pop them off easily)
    photo_display_sequence: Vec<u32>,
    order: Order,
    random_start: bool,
    source_size: SourceSize,
}

impl<A: ApiClient> Slideshow<A> {
    pub fn new(api_client: A) -> Self {
        Self {
            api_client,
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

    pub fn get_next_photo(&mut self, random: Random) -> Result<Bytes> {
        if !self.api_client.is_logged_in() {
            self.api_client.login().map_err(|e| anyhow!(e))?;
        }
        loop {
            if self.slideshow_ended() {
                self.initialize(random)?;
            }

            let photo_index = self
                .photo_display_sequence
                .pop()
                .expect("photos should not be empty");
            let photo = self
                .api_client
                .get_album_contents(photo_index, 1.try_into().unwrap(), self.order.into())?
                .pop();

            if photo.is_none() {
                /* Photos were removed from the album since we fetched its item_count. Reinitialize */
                self.photo_display_sequence.clear();
                continue;
            }
            let Item { id, additional, .. } = photo.unwrap();
            let photo_bytes_result = self.api_client.get_photo(
                id,
                &additional
                    .expect("expected additional")
                    .thumbnail
                    .expect("expected thumbnail")
                    .cache_key,
                self.source_size,
            );
            match photo_bytes_result {
                Err(error) if Self::photo_removed(&error) => {
                    continue;
                }
                _ => break photo_bytes_result,
            }
        }
    }

    fn slideshow_ended(&self) -> bool {
        self.photo_display_sequence.is_empty()
    }

    fn initialize(&mut self, (rand_gen_range, rand_shuffle): Random) -> Result<()> {
        assert!(
            self.photo_display_sequence.is_empty(),
            "already initialized"
        );
        let item_count = self.api_client.get_album_contents_count()?;
        if item_count < 1 {
            bail!("Album is empty");
        }
        self.photo_display_sequence.reserve(item_count as usize);
        let photos_range = 0..item_count;
        match self.order {
            Order::ByDate | Order::ByName if self.random_start => {
                self.photo_display_sequence.extend(
                    photos_range
                        .skip(rand_gen_range(0..item_count) as usize)
                        .rev(),
                );
                /* RandomStart is only used when slideshow starts, and afterward continues in normal
                 * order */
                self.random_start = false;
            }
            Order::ByDate | Order::ByName => self.photo_display_sequence.extend(photos_range.rev()),
            Order::Random => {
                self.photo_display_sequence.extend(photos_range);
                rand_shuffle(&mut self.photo_display_sequence)
            }
        }
        Ok(())
    }

    /// Photo has been removed since we fetched its metadata, try next one.
    fn photo_removed(error: &anyhow::Error) -> bool {
        matches!(
            error.downcast_ref::<InvalidHttpResponse>(),
            Some(InvalidHttpResponse(StatusCode::NOT_FOUND))
        )
    }
}

/// These tests cover both `slideshow` and `api_client::syno_client` modules
#[cfg(test)]
mod tests {
    use super::*;

    use syno_api::{dto::List, foto::browse::album::dto::Album};

    use crate::{
        api_client::syno_client::Login,
        api_client::syno_client::SynoApiClient,
        http::HttpClient,
        http::{CookieStore, Jar, MockHttpResponse, Url},
        test_helpers::{self, MockHttpClient},
    };

    #[test]
    fn when_default_order_then_get_next_photo_starts_by_sending_login_request_and_fetches_first_photo(
    ) {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_post()
            .withf(|url, form, _| {
                url == EXPECTED_API_URL && test_helpers::is_login_form(form, "FakeSharingId")
            })
            .return_once(|_, _, _| Ok(test_helpers::new_success_response_with_json(Login {})));
        const PHOTO_COUNT: u32 = 3;
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == EXPECTED_API_URL
                    && test_helpers::is_get_count_form(form)
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![album_with_item_count(PHOTO_COUNT)],
                }))
            });
        const FIRST_PHOTO_INDEX: u32 = 0;
        const FIRST_PHOTO_ID: u32 = 1;
        const FIRST_PHOTO_CACHE_KEY: &str = "photo1";
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == EXPECTED_API_URL
                    && is_list_form(form, &FIRST_PHOTO_INDEX.to_string(), "1")
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![test_helpers::new_photo_dto(
                        FIRST_PHOTO_ID,
                        FIRST_PHOTO_CACHE_KEY,
                    )],
                }))
            });
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == EXPECTED_API_URL
                    && is_get_photo_query(
                        query,
                        &FIRST_PHOTO_ID.to_string(),
                        "FakeSharingId",
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
        let mut slideshow = new_slideshow(&client_mock, &cookie_store, SHARE_LINK);

        /* Act */
        let result = slideshow.get_next_photo(DUMMY_RANDOM);

        /* Assert */
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Bytes::from_static(&[42, 1, 255, 50]));

        const EXPECTED_REMAINING_DISPLAY_SEQUENCE: [u32; 2] = [2, 1];
        assert_eq!(
            slideshow.photo_display_sequence,
            EXPECTED_REMAINING_DISPLAY_SEQUENCE
        );

        client_mock.checkpoint();
    }

    #[test]
    fn when_random_start_then_get_next_photo_starts_by_sending_login_request_and_fetches_random_photo(
    ) {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| test_helpers::is_login_form(form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_success_response_with_json(Login {})));
        const PHOTO_COUNT: u32 = 142;
        client_mock
            .expect_post()
            .withf(|_, form, _| test_helpers::is_get_count_form(form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![album_with_item_count(PHOTO_COUNT)],
                }))
            });
        const FAKE_RANDOM_NUMBER: u32 = 42;
        const RANDOM_PHOTO_ID: u32 = 43;
        const RANDOM_PHOTO_CACHE_KEY: &str = "photo43";
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(form, &FAKE_RANDOM_NUMBER.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![test_helpers::new_photo_dto(
                        RANDOM_PHOTO_ID,
                        RANDOM_PHOTO_CACHE_KEY,
                    )],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| {
                is_get_photo_query(
                    query,
                    &RANDOM_PHOTO_ID.to_string(),
                    "FakeSharingId",
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

        let random_mock: Random = (
            |range| {
                assert_eq!(range, 0..PHOTO_COUNT);
                FAKE_RANDOM_NUMBER
            },
            |_| (),
        );
        let cookie_store = Jar::default();
        let mut slideshow =
            new_slideshow(&client_mock, &cookie_store, SHARE_LINK).with_random_start(true);

        /* Act */
        let result = slideshow.get_next_photo(random_mock);

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
                .withf(|_, form, _| test_helpers::is_login_form(form, "FakeSharingId"))
                .return_once(|_, _, _| Ok(test_helpers::new_success_response_with_json(Login {})));
            const PHOTO_COUNT: u32 = 142;
            client_mock
                .expect_post()
                .withf(|_, form, _| test_helpers::is_get_count_form(form))
                .return_once(|_, _, _| {
                    Ok(test_helpers::new_success_response_with_json(List {
                        list: vec![album_with_item_count(PHOTO_COUNT)],
                    }))
                });
            client_mock
                .expect_post()
                .withf(|_, form, _| is_list_form(form, "0", "1"))
                .return_once(|_, _, _| {
                    Ok(test_helpers::new_success_response_with_json(List {
                        list: vec![test_helpers::new_photo_dto(43, "photo43")],
                    }))
                });
            client_mock
                .expect_get()
                .withf(move |_, query| {
                    is_get_photo_query(query, "43", "FakeSharingId", "photo43", expected_size_param)
                })
                .return_once(|_, _| {
                    let mut get_photo_response = test_helpers::new_ok_response();
                    get_photo_response
                        .expect_bytes()
                        .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                    Ok(get_photo_response)
                });
            let cookie_store = Jar::default();
            let mut slideshow = new_slideshow(&client_mock, &cookie_store, SHARE_LINK)
                .with_source_size(source_size);

            /* Act */
            let result = slideshow.get_next_photo(DUMMY_RANDOM);

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
        const NEXT_PHOTO_INDEX: u32 = 2;
        const NEXT_PHOTO_ID: u32 = 3;
        const NEXT_PHOTO_CACHE_KEY: &str = "photo3";
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_list_form(form, &NEXT_PHOTO_INDEX.to_string(), "1")
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![test_helpers::new_photo_dto(
                        NEXT_PHOTO_ID,
                        NEXT_PHOTO_CACHE_KEY,
                    )],
                }))
            });
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_get_photo_query(
                        query,
                        &NEXT_PHOTO_ID.to_string(),
                        "FakeSharingId",
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
        let mut slideshow = new_slideshow(&client_mock, &cookie_store, SHARE_LINK);
        slideshow.photo_display_sequence = vec![3, NEXT_PHOTO_INDEX];

        /* Act */
        let result = slideshow.get_next_photo(DUMMY_RANDOM);

        /* Assert */
        assert!(result.is_ok());
        assert_eq!(slideshow.photo_display_sequence, vec![3]);
    }

    #[test]
    fn get_next_photo_skips_to_next_photo_when_cached_dto_is_not_found_because_photo_was_removed_from_album(
    ) {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        const NEXT_PHOTO_INDEX: u32 = 1;
        const NEXT_NEXT_PHOTO_INDEX: u32 = 2;
        const NEXT_PHOTO_ID: u32 = 2;
        const NEXT_PHOTO_CACHE_KEY: &str = "photo2";
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(form, &NEXT_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![test_helpers::new_photo_dto(
                        NEXT_PHOTO_ID,
                        NEXT_PHOTO_CACHE_KEY,
                    )],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| {
                is_get_photo_query(
                    query,
                    &NEXT_PHOTO_ID.to_string(),
                    "FakeSharingId",
                    NEXT_PHOTO_CACHE_KEY,
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
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(form, &NEXT_NEXT_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![test_helpers::new_photo_dto(3, "photo3")],
                }))
            });
        const NEXT_NEXT_PHOTO_ID: i32 = 3;
        const NEXT_NEXT_PHOTO_CACHE_KEY: &str = "photo3";
        client_mock
            .expect_get()
            .withf(|_, query| {
                is_get_photo_query(
                    query,
                    &NEXT_NEXT_PHOTO_ID.to_string(),
                    "FakeSharingId",
                    NEXT_NEXT_PHOTO_CACHE_KEY,
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
        let mut slideshow = new_slideshow(&client_mock, &cookie_store, SHARE_LINK);
        slideshow.photo_display_sequence = vec![3, NEXT_NEXT_PHOTO_INDEX, NEXT_PHOTO_INDEX];

        /* Act */
        let result = slideshow.get_next_photo(DUMMY_RANDOM);

        /* Assert */
        assert!(result.is_ok());
        assert_eq!(slideshow.photo_display_sequence, vec![3]);
    }

    #[test]
    fn when_random_order_then_photo_display_sequence_is_shuffled() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| test_helpers::is_login_form(form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_success_response_with_json(Login {})));
        const PHOTO_COUNT: u32 = 5;
        client_mock
            .expect_post()
            .withf(|_, form, _| test_helpers::is_get_count_form(form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![album_with_item_count(PHOTO_COUNT)],
                }))
            });
        const FIRST_PHOTO_INDEX: u32 = 3;
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(form, &FIRST_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![test_helpers::new_photo_dto(4, "photo4")],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| is_get_photo_query(query, "4", "FakeSharingId", "photo4", "xl"))
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                Ok(get_photo_response)
            });

        let random_mock: Random = (
            |_| 0,
            |slice| {
                slice[0] = 5;
                slice[1] = 2;
                slice[2] = 4;
                slice[3] = 1;
                slice[4] = FIRST_PHOTO_INDEX;
            },
        );

        let cookie_store = Jar::default();
        let mut slideshow =
            new_slideshow(&client_mock, &cookie_store, SHARE_LINK).with_ordering(Order::Random);

        /* Act */
        let result = slideshow.get_next_photo(random_mock);

        assert!(result.is_ok());
        assert_eq!(slideshow.photo_display_sequence, vec![5, 2, 4, 1]);
    }

    /// Tests that when photos were removed, slideshow gets re-initialized when reaching the end of the album
    #[test]
    fn get_next_photo_reinitializes_when_display_sequence_is_shorter_than_photo_album() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        const NEXT_PHOTO_INDEX: u32 = 3;
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(form, &NEXT_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: Vec::<Item>::new(), // EMPTY
                }))
            });
        const NEW_PHOTO_COUNT: u32 = 3;
        client_mock
            .expect_post()
            .withf(|_, form, _| test_helpers::is_get_count_form(form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![album_with_item_count(NEW_PHOTO_COUNT)],
                }))
            });

        const FIRST_PHOTO_INDEX: u32 = 0;
        const FIRST_PHOTO_ID: u32 = 1;
        const FIRST_PHOTO_CACHE_KEY: &str = "photo1";
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(form, &FIRST_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![test_helpers::new_photo_dto(
                        FIRST_PHOTO_ID,
                        FIRST_PHOTO_CACHE_KEY,
                    )],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| {
                is_get_photo_query(
                    query,
                    &FIRST_PHOTO_ID.to_string(),
                    "FakeSharingId",
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
        let mut slideshow = new_slideshow(&client_mock, &cookie_store, SHARE_LINK);
        slideshow.photo_display_sequence = vec![5, 4, NEXT_PHOTO_INDEX];

        /* Act */
        let result = slideshow.get_next_photo(DUMMY_RANDOM);

        /* Assert */
        assert!(result.is_ok());
        const EXPECTED_REINITIALIZED_DISPLAY_SEQUENCE: [u32; 2] = [2, 1];
        assert_eq!(
            slideshow.photo_display_sequence,
            EXPECTED_REINITIALIZED_DISPLAY_SEQUENCE
        );
    }

    const DUMMY_RANDOM: Random = (|_| 42, |_| ());

    fn new_slideshow<'a, H: HttpClient, C: CookieStore>(
        http_client: &'a H,
        cookie_store: &'a C,
        share_link: &str,
    ) -> Slideshow<SynoApiClient<'a, H, C>> {
        let share_link = Url::parse(share_link).unwrap();
        let api_client = SynoApiClient::build(http_client, cookie_store, &share_link).unwrap();
        Slideshow::new(api_client)
    }

    fn album_with_item_count(item_count: u32) -> Album {
        Album {
            id: u32::default(),
            r#type: String::default(),
            item_count,
            name: String::default(),
            owner_user_id: u32::default(),
            passphrase: String::default(),
            shared: bool::default(),
            temporary_shared: Option::default(),
            sort_by: String::default(),
            sort_direction: String::default(),
            create_time: u64::default(),
            start_time: u64::default(),
            end_time: u64::default(),
            freeze_album: Option::default(),
            version: u32::default(),
        }
    }

    fn is_list_form(form: &[(&str, &str)], offset: &str, limit: &str) -> bool {
        form.eq(&[
            ("api", "SYNO.Foto.Browse.Item"),
            ("method", "list"),
            ("version", "1"),
            ("additional", "[\"thumbnail\"]"),
            ("offset", offset),
            ("limit", limit),
            ("sort_by", "takentime"),
            ("sort_direction", "asc"),
        ])
    }

    fn is_get_photo_query(
        query: &[(&str, &str)],
        id: &str,
        sharing_id: &str,
        cache_key: &str,
        size: &str,
    ) -> bool {
        query.eq(&[
            ("api", "SYNO.Foto.Thumbnail"),
            ("method", "get"),
            ("version", "2"),
            ("_sharing_id", sharing_id),
            ("id", id),
            ("cache_key", cache_key),
            ("type", "unit"),
            ("size", size),
        ])
    }

    fn logged_in_cookie_store(url: &str) -> impl CookieStore {
        test_helpers::new_cookie_store(Some(url))
    }
}
