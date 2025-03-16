use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use bytes::Bytes;
use ftp::FtpStream;
use url_parse::url::Url;
use url_parse::core::Parser;

use crate::{
    cli::Order,
    Random,
};

#[derive(Clone, Copy, Debug)]
pub enum SortBy {
    TakenTime,
    FileName,
}

use lazy_regex::*;

/// Holds the slideshow state and queries API to fetch photos.
#[derive(Debug)]
pub struct Slideshow<'a> {
    server: Url,
    folder: String,
    user: &'a Option<String>,
    password: &'a Option<String>,
    /// Indices of photos in an album in reverse order (so we can pop them off easily)
    photo_display_sequence: Vec<u32>,
    order: Order,
    random_start: bool,
}

#[derive(Debug)]
pub enum SlideshowError {
    Other(String),
}

impl<'a> Slideshow<'a> {
    pub fn build(server: &'a String, folder: &'a String, user: &'a Option<String>) -> Result<Slideshow<'a>, String> {
        let server_url = Parser::new(None).parse(server).unwrap();
        Ok(Slideshow {
            server: server_url,
            folder: folder.clone(),
            user,
            password: &None,
            photo_display_sequence: vec![],
            order: Order::ByDate,
            random_start: false,
        })
    }

    pub fn with_password(mut self, password: &'a Option<String>) -> Self {
        self.password = password;
        self
    }

    pub fn with_ordering(mut self, order: Order) -> Self {
        self.order = order;
        self
    }

    pub fn with_random_start(mut self, random_start: bool) -> Self {
        self.random_start = random_start;
        self
    }

    fn get_photos_count(&self) -> u32 {
        // Filter for jpegs
        let pattern = regex!(r#"^.+\.(?i:jpg|jpeg)"#);
        // Create a connection to FTP server
        let ftp_connect = self.server.host_str().unwrap();
        let mut ftp_stream = FtpStream::connect(format!("{}:21", ftp_connect)).unwrap();
        let _ = ftp_stream.login(self.user.clone().unwrap().as_str(), self.password.clone().unwrap().as_str()).unwrap();

        
        // Change into a new directory, relative to the one we are currently in.
        let _ = ftp_stream.cwd(&self.folder).unwrap();

        // Fetch list of Photos
        let mut photos = ftp_stream.nlst(None).unwrap();
        photos.retain(| filename | pattern.is_match(filename));

        // Terminate the connection to the server.
        let _ = ftp_stream.quit();
        photos.len() as u32
    }

    pub fn get_photo(&mut self, photo_index: u32) -> Result<Bytes, ()> {
        // Filter for jpegs
        let pattern = regex!(r#"^.+\.(?i:jpg|jpeg)"#);
        // Create a connection to an FTP server and authenticate to it.
        let ftp_connect = self.server.host_str().unwrap();
        let mut ftp_stream = FtpStream::connect(format!("{}:21", ftp_connect)).unwrap();
        let _ = ftp_stream.login(self.user.clone().unwrap().as_str(), self.password.clone().unwrap().as_str()).unwrap();

        
        // Change into a new directory, relative to the one we are currently in.
        let _ = ftp_stream.cwd(&self.folder).unwrap();

        // Fetch list of Photos
        let mut photos = ftp_stream.nlst(None).unwrap();
        photos.retain(| filename | pattern.is_match(filename));

        // Retrieve (GET) a file from the FTP server in the current working directory.
        let remote_file = Bytes::from(ftp_stream.simple_retr(photos.get(photo_index as usize).unwrap()).unwrap().into_inner());


        // Terminate the connection to the server.
        let _ = ftp_stream.quit();
        Ok(remote_file)
    }

    pub fn get_next_photo(
        &mut self,
        random: Random,
    ) -> Result<Bytes, SlideshowError> {
        loop {
            if self.slideshow_ended() {
                self.initialize(random)?;
            }

            let photo_index = self
                .photo_display_sequence
                .pop()
                .expect("photos should not be empty");

            let photo_bytes_result = self.get_photo(photo_index);
            match photo_bytes_result {
                Ok(photo_bytes) => break Ok(photo_bytes),
                Err(_) => { 
                    /* Photos were removed from the album since we fetched its item_count. Reinitialize */
                    self.photo_display_sequence.clear();
                    continue; 
                },
            }
        }
    }

    fn slideshow_ended(&self) -> bool {
        self.photo_display_sequence.is_empty()
    }

    fn initialize(
        &mut self,
        (rand_gen_range, rand_shuffle): Random,
    ) -> Result<(), String> {
        assert!(
            self.photo_display_sequence.is_empty(),
            "already initialized"
        );
        let item_count = self.get_photos_count();
        if item_count < 1 {
            return Err("Album is empty".to_string());
        }
        self.photo_display_sequence.reserve(item_count as usize);
        let photos_range = 0..item_count;
        match self.order {
            Order::ByDate | Order::ByName => {
                if self.random_start {
                    self.photo_display_sequence.extend(
                        photos_range
                            .skip(rand_gen_range(0..item_count) as usize)
                            .rev(),
                    );
                    /* RandomStart is only used when slideshow starts, and afterward continues in normal order */
                    self.random_start = false;
                } else {
                    self.photo_display_sequence.extend(photos_range.rev());
                }
            }
            Order::Random => {
                self.photo_display_sequence.extend(photos_range);
                rand_shuffle(&mut self.photo_display_sequence)
            }
        }

        Ok(())
    }
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

impl Error for SlideshowError {}

impl Display for SlideshowError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SlideshowError::Other(error) => write!(f, "{error}"),
        }
    }
}

impl From<String> for SlideshowError {
    fn from(value: String) -> Self {
        SlideshowError::Other(value)
    }
}
