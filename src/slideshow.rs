use std::{ops::Range, sync::Arc};

use bytes::Bytes;

use crate::{
    api::{self, dto::Album, dto::Photo, PhotosApiError, SharingId},
    http::{Client, CookieStore, Response, StatusCode, Url},
    ErrorToString,
};

static BATCH_SIZE: u32 = 10;

/// Holds the slideshow state: batch of metadata to identify photos in the API and currently displayed photo index.
#[derive(Debug)]
pub(crate) struct Slideshow {
    api_url: Url,
    sharing_id: SharingId,
    next_batch_offset: u32,
    photos_batch: Vec<Photo>,
    photo_index: usize,
}

impl TryFrom<&Url> for Slideshow {
    type Error = String;

    fn try_from(share_link: &Url) -> Result<Slideshow, Self::Error> {
        let (api_url, sharing_id) = api::parse_share_link(share_link)?;

        Ok(Slideshow {
            api_url,
            sharing_id,
            next_batch_offset: 0,
            photos_batch: vec![],
            photo_index: 0,
        })
    }
}

impl Slideshow {
    pub(crate) fn get_next_photo<C: Client>(
        &mut self,
        (client, cookie_store): (&C, &Arc<dyn CookieStore>),
        random: Option<fn(Range<u32>) -> u32>,
    ) -> Result<Bytes, String> {
        let post = |url: &str, form: &[(&str, &str)], header: Option<(&str, &str)>| {
            client.post(url, form, header)
        };

        if cookie_store.cookies(&self.api_url).is_none() {
            api::login(&post, &self.api_url, &self.sharing_id).map_err_to_string()?;
        }

        if self.slideshow_ended() {
            self.initialize(&post, random)?;
        }

        if self.need_next_batch() {
            /* Fetch next 10 photo DTOs (containing metadata needed to get the actual photo bytes). This way we avoid
             * sending 2 requests for every photo. */
            self.photos_batch = api::get_album_contents(
                &post,
                &self.api_url,
                &self.sharing_id,
                self.next_batch_offset,
                BATCH_SIZE,
            )
            .map_err_to_string()?;
            self.next_batch_offset += BATCH_SIZE;
            self.photo_index = 0;
        }

        if !self.photos_batch.is_empty() {
            let photo = &self.photos_batch[self.photo_index];
            match api::get_photo(
                &|url, form| client.get(url, form),
                &self.api_url,
                &self.sharing_id,
                photo,
            ) {
                Err(error) => {
                    if let PhotosApiError::InvalidHttpResponse(StatusCode::NOT_FOUND) = error {
                        /* Photo has been removed since we fetched its metadata, try next one */
                        self.photo_index += 1;
                        self.get_next_photo((client, cookie_store), None)
                    } else {
                        Err(error.to_string())
                    }
                }
                Ok(photo_bytes) => {
                    self.photo_index += 1;
                    Ok(photo_bytes)
                }
            }
        } else {
            Err("Album is empty".to_string())
        }
    }

    fn slideshow_ended(&self) -> bool {
        self.photos_batch.len() < BATCH_SIZE as usize && self.need_next_batch()
    }

    fn initialize<F, R>(
        &mut self,
        post: &F,
        random: Option<fn(Range<u32>) -> u32>,
    ) -> Result<(), String>
    where
        F: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
        R: Response,
    {
        if let Some(random) = random {
            let albums = api::get_album_contents_count(&post, &self.api_url, &self.sharing_id)
                .map_err_to_string()?;
            if let Some(Album { item_count }) = albums.first() {
                self.next_batch_offset = random(0..*item_count);
            } else {
                return Err("Could not get album's item count".to_string());
            }
        } else {
            self.next_batch_offset = 0;
        }
        Ok(())
    }

    fn need_next_batch(&self) -> bool {
        self.photo_index == self.photos_batch.len()
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

    #[test]
    fn get_next_photo_starts_by_sending_login_request_and_fetches_album_contents_and_first_photo() {
        /* Arrange */
        let mut slideshow = new_slideshow("http://fake.dsm.addr/aa/sharing/FakeSharingId");
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|url, form, _| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_login_form(&form, "FakeSharingId")
            })
            .return_once(|_, _, _| Ok(test_helpers::new_response_with_json(dto::Login {})));
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_list_form(&form, "0", "10")
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![
                        test_helpers::new_photo_dto(1, "photo1"),
                        test_helpers::new_photo_dto(2, "photo2"),
                        test_helpers::new_photo_dto(3, "photo3"),
                    ],
                }))
            });
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_get_photo_query(&query, "1", "FakeSharingId", "photo1")
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
        let result = slideshow.get_next_photo((&client_mock, &cookie_store), None);

        /* Assert */
        assert_eq!(slideshow.photos_batch.len(), 3);
        assert_eq!(slideshow.photo_index, 1);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Bytes::from_static(&[42, 1, 255, 50]));

        client_mock.checkpoint();
    }

    #[test]
    fn get_next_photo_starts_by_sending_login_request_and_fetches_album_contents_and_random_photo()
    {
        /* Arrange */
        let mut slideshow = new_slideshow("http://fake.dsm.addr/aa/sharing/FakeSharingId");
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|_, form, _| is_login_form(&form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_response_with_json(dto::Login {})));
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_get_count_form(&form)
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![dto::Album { item_count: 142 }],
                }))
            });
        const FAKE_RANDOM_NUMBER: u32 = 42;
        client_mock
            .expect_post()
            .withf(|_, form, _| is_list_form(&form, &FAKE_RANDOM_NUMBER.to_string(), "10"))
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![
                        test_helpers::new_photo_dto(1, "photo1"),
                        test_helpers::new_photo_dto(2, "photo2"),
                        test_helpers::new_photo_dto(3, "photo3"),
                    ],
                }))
            });
        client_mock
            .expect_get()
            .withf(|_, query| is_get_photo_query(&query, "1", "FakeSharingId", "photo1"))
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[42, 1, 255, 50])));
                Ok(get_photo_response)
            });
        let cookie_store = Arc::new(Jar::default()) as Arc<dyn CookieStore>;

        let random_mock = |range| {
            assert_eq!(range, 0..142);
            FAKE_RANDOM_NUMBER
        };

        /* Act */
        let result = slideshow.get_next_photo((&client_mock, &cookie_store), Some(random_mock));

        /* Assert */
        assert!(result.is_ok());
        client_mock.checkpoint();
    }

    #[test]
    fn get_next_photo_advances_to_next_photo_in_previously_fetched_batch() {
        /* Arrange */
        let mut slideshow = new_slideshow("http://fake.dsm.addr/aa/sharing/FakeSharingId");
        slideshow.next_batch_offset = 10;
        slideshow.photo_index = 1;
        slideshow.photos_batch = vec![
            test_helpers::new_photo_dto(1, "photo1"),
            test_helpers::new_photo_dto(2, "photo2"),
            test_helpers::new_photo_dto(3, "photo3"),
        ];
        let mut client_mock = MockClient::new();
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_get_photo_query(&query, "2", "FakeSharingId", "photo2")
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });
        let cookie_store = test_helpers::new_cookie_store(Some(
            "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi",
        ));

        /* Act */
        let result = slideshow.get_next_photo((&client_mock, &cookie_store), None);

        /* Assert */
        assert_eq!(slideshow.photo_index, 2);
        assert!(result.is_ok());
    }

    #[test]
    fn get_next_photo_restarts_after_last_photo() {
        /* Arrange */
        let mut slideshow = new_slideshow("http://fake.dsm.addr/aa/sharing/FakeSharingId");
        slideshow.next_batch_offset = 10;
        slideshow.photo_index = 3;
        slideshow.photos_batch = vec![
            test_helpers::new_photo_dto(1, "photo1"),
            test_helpers::new_photo_dto(2, "photo2"),
            test_helpers::new_photo_dto(3, "photo3"),
        ];
        let mut client_mock = MockClient::new();
        client_mock
            .expect_post()
            .withf(|url, form, header| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_list_form(&form, "0", "10")
                    && *header == Some(("X-SYNO-SHARING", "FakeSharingId"))
            })
            .return_once(|_, _, _| {
                Ok(test_helpers::new_response_with_json(dto::List {
                    list: vec![
                        test_helpers::new_photo_dto(1, "photo1"),
                        test_helpers::new_photo_dto(2, "photo2"),
                        test_helpers::new_photo_dto(3, "photo3"),
                    ],
                }))
            });
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_get_photo_query(&query, "1", "FakeSharingId", "photo1")
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });
        let cookie_store = test_helpers::new_cookie_store(Some(
            "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi",
        ));

        /* Act */
        let result = slideshow.get_next_photo((&client_mock, &cookie_store), None);

        /* Assert */
        assert_eq!(slideshow.photo_index, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn get_next_photo_skips_to_next_photo_when_cached_dto_is_not_found_because_photo_was_removed_from_album(
    ) {
        /* Arrange */
        let mut slideshow = new_slideshow("http://fake.dsm.addr/aa/sharing/FakeSharingId");
        slideshow.next_batch_offset = 10;
        slideshow.photo_index = 1;
        slideshow.photos_batch = vec![
            test_helpers::new_photo_dto(1, "photo1"),
            test_helpers::new_photo_dto(2, "photo2"),
            test_helpers::new_photo_dto(3, "photo3"),
        ];
        let mut client_mock = MockClient::new();
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_get_photo_query(&query, "2", "FakeSharingId", "photo2")
            })
            .return_once(|_, _| {
                let mut not_found_response = MockResponse::new();
                not_found_response
                    .expect_status()
                    .returning(|| StatusCode::NOT_FOUND);
                Ok(not_found_response)
            });
        client_mock
            .expect_get()
            .withf(|url, query| {
                url == "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi"
                    && is_get_photo_query(&query, "3", "FakeSharingId", "photo3")
            })
            .return_once(|_, _| {
                let mut get_photo_response = test_helpers::new_success_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });
        let cookie_store = test_helpers::new_cookie_store(Some(
            "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi",
        ));

        /* Act */
        let result = slideshow.get_next_photo((&client_mock, &cookie_store), None);

        /* Assert */
        assert_eq!(slideshow.photo_index, 3);
        assert!(result.is_ok());
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
    ) -> bool {
        query.eq(&[
            ("api", "SYNO.Foto.Thumbnail"),
            ("method", "get"),
            ("version", "2"),
            ("_sharing_id", sharing_id),
            ("id", id),
            ("cache_key", cache_key),
            ("type", "unit"),
            ("size", "xl"),
        ])
    }
}
