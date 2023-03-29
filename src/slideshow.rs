use std::sync::Arc;

use bytes::Bytes;
use reqwest::{cookie::CookieStore, StatusCode, Url};

use crate::{
    api::{self, dto::Photo, PhotosApiError},
    http::Response,
    ErrorToString,
};

static BATCH_SIZE: u32 = 10;

pub struct Slideshow<P, G> {
    cookie_store: Arc<dyn CookieStore>,
    post: P,
    get: G,
    api_url: Url,
    sharing_id: String,
    next_batch_offset: u32,
    photos_batch: Vec<Photo>,
    photo_index: usize,
}

impl<P, G, R> Slideshow<P, G>
where
    P: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
    G: Fn(&str, &[(&str, &str)]) -> Result<R, String>,
    R: Response,
{
    pub fn new(
        (cookie_store, post, get): (Arc<dyn CookieStore>, P, G),
        share_link: &Url,
    ) -> Result<Self, String> {
        let (api_url, sharing_id) = api::parse_share_link(share_link)?;

        Ok(Slideshow {
            cookie_store,
            post,
            get,
            api_url,
            sharing_id,
            next_batch_offset: 0,
            photos_batch: vec![],
            photo_index: 0,
        })
    }

    pub fn get_next_photo(&mut self) -> Result<Bytes, String> {
        if let None = self.cookie_store.cookies(&self.api_url) {
            api::login(&self.post, &self.api_url, &self.sharing_id).map_err_to_string()?;
        }

        if self.slideshow_ended() {
            self.next_batch_offset = 0;
        }

        if self.need_next_batch() {
            /* Fetch next 10 photo DTOs (containing metadata needed to get the actual photo bytes). This way we avoid
             * sending 2 requests for every photo. */
            self.photos_batch = api::get_album_contents(
                &self.post,
                &self.api_url,
                &self.sharing_id,
                self.next_batch_offset,
                BATCH_SIZE,
            )
            .map_err_to_string()?;
            self.next_batch_offset += BATCH_SIZE;
            self.photo_index = 0;
        }

        if self.photos_batch.len() > 0 {
            let photo = &self.photos_batch[self.photo_index];
            match api::get_photo(&self.get, &self.api_url, &self.sharing_id, photo) {
                Err(error) => {
                    if let PhotosApiError::InvalidHttpResponse(StatusCode::NOT_FOUND) = error {
                        /* Photo has been removed since we fetched its metadata, try next one */
                        self.photo_index += 1;
                        return self.get_next_photo();
                    } else {
                        return Err(error.to_string());
                    }
                }
                Ok(photo_bytes) => {
                    self.photo_index += 1;
                    return Ok(photo_bytes);
                }
            }
        } else {
            return Err("Album is empty".to_string());
        }
    }

    fn slideshow_ended(&self) -> bool {
        self.photos_batch.len() < BATCH_SIZE as usize && self.need_next_batch()
    }

    fn need_next_batch(&self) -> bool {
        self.photo_index == self.photos_batch.len()
    }
}
