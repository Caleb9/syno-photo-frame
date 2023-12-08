use std::sync::Arc;

use bytes::Bytes;

use crate::{
    api::{self, dto::Album, PhotosApiError, SharingId},
    cli::{Order, SourceSize},
    http::{Client, CookieStore, StatusCode, Url},
    ErrorToString, Random,
};

/// Holds the slideshow state and queries API to fetch photos.
#[derive(Debug)]
pub(crate) struct Slideshow {
    api_url: Url,
    sharing_id: SharingId,
    /// Indices of photos in an album in reverse order (so we can pop them off easily)
    photo_display_sequence: Vec<u32>,
    order: Order,
    source_size: SourceSize,
}

impl TryFrom<&Url> for Slideshow {
    type Error = String;

    fn try_from(share_link: &Url) -> Result<Slideshow, Self::Error> {
        let (api_url, sharing_id) = api::parse_share_link(share_link)?;

        Ok(Slideshow {
            api_url,
            sharing_id,
            photo_display_sequence: vec![],
            order: Order::ByDate,
            source_size: SourceSize::L,
        })
    }
}

impl Slideshow {
    pub(crate) fn with_ordering(mut self, order: Order) -> Self {
        self.order = order;
        self
    }

    pub(crate) fn with_source_size(mut self, size: SourceSize) -> Self {
        self.source_size = size;
        self
    }

    pub(crate) fn get_next_photo(
        &mut self,
        (client, cookie_store): (&impl Client, &Arc<dyn CookieStore>),
        random: Random,
    ) -> Result<Bytes, String> {
        if !self.is_logged_in(cookie_store) {
            api::login(client, &self.api_url, &self.sharing_id).map_err_to_string()?;
        }

        if self.slideshow_ended() {
            self.initialize(client, random)?;
        }

        let photo_index = self
            .photo_display_sequence
            .pop()
            .expect("photos should not be empty");
        let photos = api::get_album_contents(
            client,
            &self.api_url,
            &self.sharing_id,
            photo_index,
            1.try_into()?,
        )
        .map_err_to_string()?;

        if let Some(photo) = photos.first() {
            match api::get_photo(
                client,
                &self.api_url,
                &self.sharing_id,
                (
                    photo.id,
                    &photo.additional.thumbnail.cache_key,
                    self.source_size,
                ),
            ) {
                Ok(photo_bytes) => Ok(photo_bytes),
                Err(PhotosApiError::InvalidHttpResponse(StatusCode::NOT_FOUND)) => {
                    /* Photo has been removed since we fetched its metadata, try next one */
                    self.get_next_photo((client, cookie_store), random)
                }
                Err(error) => Err(error.to_string()),
            }
        } else {
            /* Photos were removed from the album since we fetched its item_count. Reinitialize */
            self.photo_display_sequence = vec![];
            self.get_next_photo((client, cookie_store), random)
        }
    }

    fn is_logged_in(&self, cookie_store: &Arc<dyn CookieStore>) -> bool {
        cookie_store.cookies(&self.api_url).is_some()
    }

    fn slideshow_ended(&self) -> bool {
        self.photo_display_sequence.is_empty()
    }

    fn initialize(
        &mut self,
        client: &impl Client,
        (rand_gen_range, rand_shuffle): Random,
    ) -> Result<(), String> {
        let item_count = self.get_photos_count(client)?;
        if item_count < 1 {
            return Err("Album is empty".to_string());
        }
        let photos_range = 0..item_count;
        match self.order {
            Order::ByDate => {
                self.photo_display_sequence = photos_range.rev().collect();
            }
            Order::RandomStart => {
                let random_start = rand_gen_range(0..item_count);
                self.photo_display_sequence =
                    photos_range.skip(random_start as usize).rev().collect();
                /* RandomStart is only used when slideshow starts, and afterward continues in normal order */
                self.order = Order::ByDate;
            }
            Order::Random => {
                self.photo_display_sequence = photos_range.collect();
                rand_shuffle(&mut self.photo_display_sequence);
            }
        }

        Ok(())
    }

    fn get_photos_count(&self, client: &impl Client) -> Result<u32, String> {
        let albums = api::get_album_contents_count(client, &self.api_url, &self.sharing_id)
            .map_err_to_string()?;
        if let Some(Album { item_count }) = albums.first() {
            Ok(*item_count)
        } else {
            Err("Album not found".to_string())
        }
    }
}

/// These tests cover both `slideshow` and `api` modules
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::dto,
        http::{Jar, MockResponse},
        test_helpers::{self, MockClient},
    };

    const DUMMY_RANDOM: Random = (|_| 42, |_| ());

    #[test]
    fn when_default_order_then_get_next_photo_starts_by_sending_login_request_and_fetches_first_photo(
    ) {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        let mut slideshow = new_slideshow(SHARE_LINK);
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|url, form, _| url == EXPECTED_API_URL && is_login_form(&form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_response_with_json(dto::Login {})));
        const PHOTO_COUNT: u32 = 3;
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == EXPECTED_API_URL
                    && is_get_count_form(&form)
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![Album {
                        item_count: PHOTO_COUNT,
                    }],
                }))
            });
        const FIRST_PHOTO_INDEX: u32 = 0;
        const FIRST_PHOTO_ID: i32 = 1;
        const FIRST_PHOTO_CACHE_KEY: &str = "photo1";
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == EXPECTED_API_URL
                    && is_list_form(&form, &FIRST_PHOTO_INDEX.to_string(), "1")
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
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
                        &query,
                        &FIRST_PHOTO_ID.to_string(),
                        "FakeSharingId",
                        FIRST_PHOTO_CACHE_KEY,
                        "xl",
                    )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                Ok(get_photo_response)
            });
        let cookie_store = Arc::new(Jar::default()) as Arc<dyn CookieStore>;

        /* Act */
        let result = slideshow.get_next_photo((&client_mock, &cookie_store), DUMMY_RANDOM);

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
    fn when_random_start_order_then_get_next_photo_starts_by_sending_login_request_and_fetches_random_photo(
    ) {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        let mut slideshow = new_slideshow(SHARE_LINK).with_ordering(Order::RandomStart);
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| is_login_form(&form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_response_with_json(dto::Login {})));
        const PHOTO_COUNT: u32 = 142;
        client_mock
            .expect_post()
            .withf(|_, form, _| is_get_count_form(&form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![dto::Album {
                        item_count: PHOTO_COUNT,
                    }],
                }))
            });
        const FAKE_RANDOM_NUMBER: u32 = 42;
        const RANDOM_PHOTO_ID: i32 = 43;
        const RANDOM_PHOTO_CACHE_KEY: &str = "photo43";
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(&form, &FAKE_RANDOM_NUMBER.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
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
                    &query,
                    &RANDOM_PHOTO_ID.to_string(),
                    "FakeSharingId",
                    RANDOM_PHOTO_CACHE_KEY,
                    "xl",
                )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                Ok(get_photo_response)
            });
        let cookie_store = Arc::new(Jar::default()) as Arc<dyn CookieStore>;

        let random_mock: Random = (
            |range| {
                assert_eq!(range, 0..PHOTO_COUNT);
                FAKE_RANDOM_NUMBER
            },
            |_| (),
        );

        /* Act */
        let result = slideshow.get_next_photo((&client_mock, &cookie_store), random_mock);

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
            let mut slideshow = new_slideshow(SHARE_LINK).with_source_size(source_size);
            let mut client_mock = MockClient::new();
            client_mock
                .expect_post()
                .withf(|_, form, _| is_login_form(&form, "FakeSharingId"))
                .return_once(|_, _, _| Ok(test_helpers::new_response_with_json(dto::Login {})));
            const PHOTO_COUNT: u32 = 142;
            client_mock
                .expect_post()
                .withf(|_, form, _| is_get_count_form(&form))
                .return_once(|_, _, _| {
                    Ok(test_helpers::new_response_with_json(dto::List {
                        list: vec![dto::Album {
                            item_count: PHOTO_COUNT,
                        }],
                    }))
                });
            client_mock
                .expect_post()
                .withf(|_, form, _| is_list_form(&form, "0", "1"))
                .return_once(|_, _, _| {
                    Ok(test_helpers::new_response_with_json(dto::List {
                        list: vec![test_helpers::new_photo_dto(43, "photo43")],
                    }))
                });
            client_mock
                .expect_get()
                .withf(move |_, query| {
                    is_get_photo_query(
                        &query,
                        "43",
                        "FakeSharingId",
                        "photo43",
                        &expected_size_param,
                    )
                })
                .return_once(|_, _| {
                    let mut get_photo_response = test_helpers::new_success_response();
                    get_photo_response
                        .expect_bytes()
                        .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                    Ok(get_photo_response)
                });
            let cookie_store = Arc::new(Jar::default()) as Arc<dyn CookieStore>;

            /* Act */
            let result = slideshow.get_next_photo((&client_mock, &cookie_store), DUMMY_RANDOM);

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
        let mut slideshow = new_slideshow(SHARE_LINK);
        const NEXT_PHOTO_INDEX: u32 = 2;
        slideshow.photo_display_sequence = vec![3, NEXT_PHOTO_INDEX];
        const NEXT_PHOTO_ID: i32 = 3;
        const NEXT_PHOTO_CACHE_KEY: &str = "photo3";
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_list_form(&form, &NEXT_PHOTO_INDEX.to_string(), "1")
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
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
                        &query,
                        &NEXT_PHOTO_ID.to_string(),
                        "FakeSharingId",
                        &NEXT_PHOTO_CACHE_KEY,
                        "xl",
                    )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });

        /* Act */
        let result = slideshow.get_next_photo(
            (&client_mock, &logged_in_cookie_store(EXPECTED_API_URL)),
            DUMMY_RANDOM,
        );

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
        let mut slideshow = new_slideshow(SHARE_LINK);
        const NEXT_PHOTO_INDEX: u32 = 1;
        const NEXT_NEXT_PHOTO_INDEX: u32 = 2;
        slideshow.photo_display_sequence = vec![3, NEXT_NEXT_PHOTO_INDEX, NEXT_PHOTO_INDEX];
        const NEXT_PHOTO_ID: i32 = 2;
        const NEXT_PHOTO_CACHE_KEY: &str = "photo2";
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(&form, &NEXT_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
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
                    &query,
                    &NEXT_PHOTO_ID.to_string(),
                    "FakeSharingId",
                    NEXT_PHOTO_CACHE_KEY,
                    "xl",
                )
            })
            .return_once(|_, _| {
                let mut not_found_response = MockResponse::new();
                not_found_response
                    .expect_status()
                    .returning(|| StatusCode::NOT_FOUND);
                Ok(not_found_response)
            });
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(&form, &NEXT_NEXT_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![test_helpers::new_photo_dto(3, "photo3")],
                }))
            });
        const NEXT_NEXT_PHOTO_ID: i32 = 3;
        const NEXT_NEXT_PHOTO_CACHE_KEY: &str = "photo3";
        client_mock
            .expect_get()
            .withf(|_, query| {
                is_get_photo_query(
                    &query,
                    &NEXT_NEXT_PHOTO_ID.to_string(),
                    "FakeSharingId",
                    NEXT_NEXT_PHOTO_CACHE_KEY,
                    "xl",
                )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });

        /* Act */
        let result = slideshow.get_next_photo(
            (&client_mock, &logged_in_cookie_store(EXPECTED_API_URL)),
            DUMMY_RANDOM,
        );

        /* Assert */
        assert!(result.is_ok());
        assert_eq!(slideshow.photo_display_sequence, vec![3]);
    }

    #[test]
    fn when_random_order_then_photo_display_sequence_is_shuffled() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        let mut slideshow = new_slideshow(SHARE_LINK).with_ordering(Order::Random);
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| is_login_form(&form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_response_with_json(dto::Login {})));
        const PHOTO_COUNT: u32 = 5;
        client_mock
            .expect_post()
            .withf(|_, form, _| is_get_count_form(&form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![dto::Album {
                        item_count: PHOTO_COUNT,
                    }],
                }))
            });
        const FIRST_PHOTO_INDEX: u32 = 3;
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(&form, &FIRST_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![test_helpers::new_photo_dto(4, "photo4")],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| is_get_photo_query(&query, "4", "FakeSharingId", "photo4", "xl"))
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                Ok(get_photo_response)
            });
        let cookie_store = Arc::new(Jar::default()) as Arc<dyn CookieStore>;

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

        /* Act */
        let result = slideshow.get_next_photo((&client_mock, &cookie_store), random_mock);

        assert!(result.is_ok());
        assert_eq!(slideshow.photo_display_sequence, vec![5, 2, 4, 1]);
    }

    /// Tests that when photos were removed, slideshow gets re-initialized when reaching the end of the album
    #[test]
    fn get_next_photo_reinitializes_when_display_sequence_is_shorter_than_photo_album() {
        /* Arrange */
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";
        let mut slideshow = new_slideshow(SHARE_LINK);
        const NEXT_PHOTO_INDEX: u32 = 3;
        slideshow.photo_display_sequence = vec![5, 4, NEXT_PHOTO_INDEX];
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(&form, &NEXT_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(
                    test_helpers::new_response_with_json::<dto::List<dto::Photo>>(dto::List {
                        list: vec![], // EMPTY
                    }),
                )
            });
        const NEW_PHOTO_COUNT: u32 = 3;
        client_mock
            .expect_post()
            .withf(|_, form, _| is_get_count_form(&form))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![dto::Album {
                        item_count: NEW_PHOTO_COUNT,
                    }],
                }))
            });

        const FIRST_PHOTO_INDEX: u32 = 0;
        const FIRST_PHOTO_ID: i32 = 1;
        const FIRST_PHOTO_CACHE_KEY: &str = "photo1";
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(&form, &FIRST_PHOTO_INDEX.to_string(), "1"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
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
                    &query,
                    &FIRST_PHOTO_ID.to_string(),
                    "FakeSharingId",
                    FIRST_PHOTO_CACHE_KEY,
                    "xl",
                )
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });

        /* Act */
        let result = slideshow.get_next_photo(
            (&client_mock, &logged_in_cookie_store(EXPECTED_API_URL)),
            DUMMY_RANDOM,
        );

        /* Assert */
        assert!(result.is_ok());
        const EXPECTED_REINITIALIZED_DISPLAY_SEQUENCE: [u32; 2] = [2, 1];
        assert_eq!(
            slideshow.photo_display_sequence,
            EXPECTED_REINITIALIZED_DISPLAY_SEQUENCE
        );
    }

    fn new_slideshow(share_link: &str) -> Slideshow {
        let share_link = Url::parse(share_link).unwrap();

        Slideshow::try_from(&share_link).unwrap()
    }

    fn is_login_form(form: &[(&str, &str)], sharing_id: &str) -> bool {
        form.eq(&[
            ("api", "SYNO.Core.Sharing.Login"),
            ("method", "login"),
            ("version", "1"),
            ("sharing_id", sharing_id),
        ])
    }

    fn is_get_count_form(form: &[(&str, &str)]) -> bool {
        form.eq(&[
            ("api", "SYNO.Foto.Browse.Album"),
            ("method", "get"),
            ("version", "1"),
        ])
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

    fn logged_in_cookie_store(url: &str) -> Arc<dyn CookieStore> {
        test_helpers::new_cookie_store(Some(url))
    }
}
