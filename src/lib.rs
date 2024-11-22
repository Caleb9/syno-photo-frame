//! # syno-photo-frame
//!
//! syno_photo_frame is a full-screen slideshow app for Synology Photos albums

use std::{
    error::Error,
    fmt::{Display, Formatter},
    ops::Range,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, Scope, ScopedJoinHandle},
    time::Duration,
};

#[cfg(not(test))]
use std::{thread::sleep as thread_sleep, time::Instant};

#[cfg(test)]
use {mock_instant::Instant, tests::fake_sleep as thread_sleep};

use crate::{
    api_client::SynoApiClient,
    cli::{Cli, Rotation},
    error::FrameError,
    http::{CookieStore, HttpClient},
    img::{DynamicImage, Framed},
    sdl::{Sdl, TextureIndex},
    slideshow::{Slideshow, SlideshowError},
    update::UpdateNotification,
};

pub mod cli;
pub mod error;
pub mod http;
pub mod logging;
pub mod sdl;

mod api_client;
mod api_crates;
mod api_photos;
mod asset;
mod img;
mod slideshow;
mod transition;
mod update;

#[cfg(test)]
mod test_helpers;

pub type FrameResult<T> = Result<T, FrameError>;

/// Functions for randomized slideshow ordering
pub type Random = (fn(Range<u32>) -> u32, fn(&mut [u32]));

#[derive(Clone, Debug)]
pub struct QuitEvent;

/// Slideshow loop
pub fn run<C: HttpClient + Sync>(
    cli: &Cli,
    (client, cookie_store): (&C, &impl CookieStore),
    sdl: &mut impl Sdl,
    random: Random,
    installed_version: &str,
) -> FrameResult<()> {
    let current_image = show_welcome_screen(cli, sdl)?;

    thread::scope::<'_, _, FrameResult<()>>(|thread_scope| {
        let (update_check_sender, update_check_receiver) = mpsc::sync_channel(1);
        if !cli.disable_update_check {
            update::check_for_updates_thread(
                client,
                installed_version,
                thread_scope,
                update_check_sender,
            );
        }

        slideshow_loop(
            cli,
            (client, cookie_store),
            sdl,
            random,
            update_check_receiver,
            current_image,
        )
    })
}

fn show_welcome_screen(cli: &Cli, sdl: &mut impl Sdl) -> FrameResult<DynamicImage> {
    let welcome_img = match &cli.splash {
        None => asset::welcome_screen(sdl.size(), cli.rotation)?,
        Some(path) => {
            let (w, h) = sdl.size();
            match img::open(path) {
                Ok(image) => image.resize_exact(w, h, image::imageops::FilterType::Nearest),
                Err(error) => {
                    log::error!("Splashscreen {}: {error}", path.to_string_lossy());
                    asset::welcome_screen(sdl.size(), cli.rotation)?
                }
            }
        }
    };
    sdl.update_texture(welcome_img.as_bytes(), TextureIndex::Current)?;
    sdl.copy_texture_to_canvas(TextureIndex::Current)?;
    sdl.present_canvas();
    Ok(welcome_img)
}

fn slideshow_loop<C: HttpClient + Sync>(
    cli: &Cli,
    http: (&C, &impl CookieStore),
    sdl: &mut impl Sdl,
    random: Random,
    update_check_receiver: Receiver<bool>,
    mut current_image: DynamicImage,
) -> FrameResult<()> {
    /* Load the first photo as soon as it's ready. */
    let mut last_change = Instant::now() - cli.photo_change_interval;
    let screen_size = sdl.size();
    let mut update_notification = UpdateNotification::new(screen_size, cli.rotation)?;
    let (photo_sender, photo_receiver) = mpsc::sync_channel(1);
    const LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);

    thread::scope::<'_, _, FrameResult<()>>(|thread_scope| {
        photo_fetcher_thread(cli, http, screen_size, random, thread_scope, photo_sender)?;

        let loop_result = loop {
            sdl.handle_quit_event()?;

            if let Ok(true) = update_check_receiver.try_recv() {
                /* Overlay a notification on the currently displayed image when an update was
                 * detected */
                update_notification.is_visible = true;
                update_notification.show_on_current_image(&mut current_image, sdl)?;
            }

            let elapsed_display_duration = Instant::now() - last_change;
            if elapsed_display_duration < cli.photo_change_interval {
                thread_sleep(LOOP_SLEEP_DURATION);
                continue;
            }

            if let Ok(next_photo_result) = photo_receiver.try_recv() {
                let next_image = match next_photo_result {
                    Err(SlideshowError::Login(error)) => {
                        /* Login error terminates the main thread loop */
                        break Err(FrameError::Login(error));
                    }
                    ok_or_other_error => load_photo_or_error_screen(
                        ok_or_other_error,
                        screen_size,
                        cli.rotation,
                        &update_notification,
                    )?,
                };
                sdl.update_texture(next_image.as_bytes(), TextureIndex::Next)?;
                cli.transition.play(sdl)?;

                last_change = Instant::now();

                sdl.swap_textures();
                current_image = next_image;
            } else {
                /* next photo is still being fetched and processed, we have to wait for it */
                thread_sleep(LOOP_SLEEP_DURATION);
            }
        };
        if loop_result.is_err() {
            /* Dropping the receiver terminates photo_fetcher_thread loop */
            drop(photo_receiver);
        }
        loop_result
    })
}

fn photo_fetcher_thread<'a, C: HttpClient + Sync>(
    cli: &'a Cli,
    (http_client, cookie_store): (&'a C, &'a impl CookieStore),
    screen_size: (u32, u32),
    random: Random,
    thread_scope: &'a Scope<'a, '_>,
    photo_sender: SyncSender<Result<DynamicImage, SlideshowError>>,
) -> Result<ScopedJoinHandle<'a, ()>, String> {
    let api_client = SynoApiClient::build(http_client, cookie_store, &cli.share_link)?
        .with_password(&cli.password);
    let mut slideshow = Slideshow::new(api_client)
        .with_ordering(cli.order)
        .with_random_start(cli.random_start)
        .with_source_size(cli.source_size);
    Ok(thread_scope.spawn(move || loop {
        let photo_result = slideshow
            .get_next_photo(random)
            .and_then(|bytes| img::load_from_memory(&bytes).map_err(SlideshowError::Other))
            .map(|image| {
                image.fit_to_screen_and_add_background(screen_size, cli.rotation, cli.background)
            });
        /* Blocks until photo is received by the main thread */
        let send_result = photo_sender.send(photo_result);
        if send_result.is_err() {
            break;
        }
    }))
}

fn load_photo_or_error_screen(
    next_photo_result: Result<DynamicImage, SlideshowError>,
    screen_size: (u32, u32),
    rotation: Rotation,
    update_notification: &UpdateNotification,
) -> FrameResult<DynamicImage> {
    let mut next_image = match next_photo_result {
        Ok(photo) => photo,
        Err(SlideshowError::Other(error)) => {
            /* Any non-login error gets logged and an error screen is displayed. */
            log::error!("{error}");
            asset::error_screen(screen_size, rotation)?
        }
        Err(SlideshowError::Login(_)) => {
            panic!("Login error should have been handled in the slideshow_loop")
        }
    };
    if update_notification.is_visible {
        update_notification.overlay(&mut next_image);
    }
    Ok(next_image)
}

impl Display for QuitEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Quit")
    }
}

impl Error for QuitEvent {}

#[cfg(test)]
mod tests {
    use crate::{
        api_photos::{dto, PhotosApiError},
        cli::Parser,
        http::{Jar, MockHttpResponse, StatusCode},
        sdl::MockSdl,
        test_helpers::MockClient,
    };

    use mock_instant::MockClock;

    use super::*;

    #[test]
    fn when_login_fails_with_api_error_then_loop_terminates() {
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";

        let mut client_stub = MockClient::new();
        client_stub
            .expect_post()
            .withf(|url, form, _| {
                url == EXPECTED_API_URL && test_helpers::is_login_form(form, "FakeSharingId")
            })
            .returning(|_, _, _| {
                let mut error_response = test_helpers::new_ok_response();
                error_response
                    .expect_json::<dto::ApiResponse<dto::Login>>()
                    .return_once(|| {
                        Ok(dto::ApiResponse {
                            success: false,
                            error: Some(dto::ApiError { code: 42 }),
                            data: None,
                        })
                    });
                Ok(error_response)
            });
        /* Avoid overflow when setting initial last_change */
        const DISPLAY_INTERVAL: u64 = 30;
        MockClock::set_time(Duration::from_secs(DISPLAY_INTERVAL));
        let mut sdl_stub = MockSdl::new().with_default_expectations();
        let cli_command = format!(
            "syno-photo-frame {SHARE_LINK} \
             --interval {DISPLAY_INTERVAL} \
             --disable-update-check"
        );

        let result = run(
            &Cli::parse_from(cli_command.split(' ')),
            (&client_stub, &Jar::default()),
            &mut sdl_stub,
            DUMMY_RANDOM,
            "1.2.3",
        );

        assert!(matches!(
            result,
            Err(FrameError::Login(PhotosApiError::InvalidApiResponse(_, 42)))
        ));
    }

    #[test]
    fn when_login_fails_with_http_error_then_loop_terminates() {
        let mut client_stub = MockClient::new();
        client_stub.expect_post().returning(|_, _, _| {
            let mut error_response = MockHttpResponse::new();
            error_response
                .expect_status()
                .return_const(StatusCode::FORBIDDEN);
            Ok(error_response)
        });
        /* Avoid overflow when setting initial last_change */
        const DISPLAY_INTERVAL: u64 = 30;
        MockClock::set_time(Duration::from_secs(DISPLAY_INTERVAL));
        let mut sdl_stub = MockSdl::new().with_default_expectations();
        let cli_command = format!(
            "syno-photo-frame http://fake.dsm.addr/aa/sharing/FakeSharingId \
            --interval {DISPLAY_INTERVAL} \
            --disable-update-check"
        );

        let result = run(
            &Cli::parse_from(cli_command.split(' ')),
            (&client_stub, &Jar::default()),
            &mut sdl_stub,
            DUMMY_RANDOM,
            "1.2.3",
        );

        assert!(matches!(
            result,
            Err(FrameError::Login(PhotosApiError::InvalidHttpResponse(
                StatusCode::FORBIDDEN
            )))
        ));
    }

    #[test]
    fn when_getting_photo_fails_loop_continues() {
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";

        let mut client_stub = MockClient::new();
        client_stub
            .expect_post()
            .withf(|_, form, _| test_helpers::is_login_form(form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_success_response_with_json(dto::Login {})));
        /* Simulate failing get_album_contents_count request */
        client_stub
            .expect_post()
            .withf(|_, form, _| test_helpers::is_get_count_form(form))
            .returning(|_, _, _| {
                let mut error_response = MockHttpResponse::new();
                error_response
                    .expect_status()
                    .return_const(StatusCode::NOT_FOUND);
                Ok(error_response)
            });

        /* Avoid overflow when setting initial last_change */
        const DISPLAY_INTERVAL: u64 = 30;
        MockClock::set_time(Duration::from_secs(DISPLAY_INTERVAL));
        let mut sdl_stub = MockSdl::new();
        {
            sdl_stub.expect_size().return_const((198, 102));
            sdl_stub
                .expect_copy_texture_to_canvas()
                .return_const(Ok(()));
            sdl_stub.expect_fill_canvas().return_const(Ok(()));
            sdl_stub.expect_present_canvas().return_const(());
        }
        sdl_stub.expect_update_texture().return_once(|_, _| {
            MockClock::advance(Duration::from_secs(1));
            Ok(())
        });
        sdl_stub.expect_handle_quit_event().returning(|| {
            /* Until update_texture is called (with an error image) and advances the time, return
             * Ok. Afterward, break the loop with a simulated Quit event to finish the test */
            if MockClock::time() <= Duration::from_secs(DISPLAY_INTERVAL) {
                Ok(())
            } else {
                Err(QuitEvent)
            }
        });
        let cli_command = format!(
            "syno-photo-frame {SHARE_LINK} \
            --interval {DISPLAY_INTERVAL} \
            --disable-update-check"
        );

        let result = run(
            &Cli::parse_from(cli_command.split(' ')),
            (&client_stub, &Jar::default()),
            &mut sdl_stub,
            DUMMY_RANDOM,
            "1.2.3",
        );

        /* If failed request bubbled up its error and broke the main slideshow loop, we would
         * observe it here as the error type would be different from Quit */
        assert!(matches!(result, Err(FrameError::Quit(QuitEvent))));
    }

    pub fn fake_sleep(_: Duration) {}

    const DUMMY_RANDOM: Random = (|_| 42, |_| ());

    impl MockSdl {
        pub fn with_default_expectations(mut self) -> Self {
            self.expect_size().return_const((198, 102));
            self.expect_update_texture().return_const(Ok(()));
            self.expect_copy_texture_to_canvas().return_const(Ok(()));
            self.expect_fill_canvas().return_const(Ok(()));
            self.expect_present_canvas().return_const(());
            self.expect_handle_quit_event().return_const(Ok(()));
            self
        }
    }
}
