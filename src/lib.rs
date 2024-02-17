//! # syno-photo-frame
//!
//! syno_photo_frame is a full-screen slideshow app for Synology Photos albums

use std::{
    ops::Range,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, Scope, ScopedJoinHandle},
    time::{Duration, Instant},
};

use crate::{
    cli::{Cli, Rotation},
    error::SynoPhotoFrameError,
    http::{Client, CookieStore},
    img::{DynamicImage, Framed},
    sdl::{Sdl, TextureIndex},
    slideshow::Slideshow,
    update::UpdateNotification,
};

pub mod cli;
pub mod error;
pub mod http;
pub mod logging;
pub mod sdl;

mod api_crates;
mod api_photos;
mod asset;
mod img;
mod slideshow;
mod transition;
mod update;

#[cfg(test)]
mod test_helpers;

/// Functions for randomized slideshow ordering
pub type Random = (fn(Range<u32>) -> u32, fn(&mut [u32]));

type Result<T> = core::result::Result<T, SynoPhotoFrameError>;

/// Slideshow loop
pub fn run<C: Client + Clone + Send>(
    cli: &Cli,
    (client, cookie_store): (&C, &impl CookieStore),
    sdl: &mut impl Sdl,
    (sleep, random): (fn(Duration), Random),
    installed_version: &str,
) -> Result<()> {
    let current_image = show_welcome_screen(sdl, cli)?;

    thread::scope::<'_, _, Result<()>>(|thread_scope| {
        let (update_tx, update_rx) = mpsc::sync_channel(1);
        if !cli.disable_update_check {
            update::check_for_updates_thread(client, installed_version, thread_scope, update_tx);
        }

        slideshow_loop(
            (client, cookie_store),
            (sdl, current_image),
            cli,
            (sleep, random),
            update_rx,
        )
    })
}

fn show_welcome_screen(sdl: &mut impl Sdl, cli: &Cli) -> Result<DynamicImage> {
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

fn slideshow_loop<C: Client + Clone + Send>(
    http: (&C, &impl CookieStore),
    (sdl, mut current_image): (&mut impl Sdl, DynamicImage),
    cli: &Cli,
    (sleep, random): (fn(Duration), Random),
    update_rx: Receiver<bool>,
) -> Result<()> {
    /* Load the first photo as soon as it's ready. */
    let mut last_change = Instant::now() - cli.photo_change_interval;
    let mut update_notification = UpdateNotification::new(sdl.size(), cli.rotation)?;
    let screen_size = sdl.size();
    let (photo_tx, photo_rx) = mpsc::sync_channel(1);
    const LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);

    thread::scope::<'_, _, Result<()>>(|thread_scope| {
        photo_fetcher_thread(http, screen_size, cli, random, thread_scope, photo_tx)?;

        loop {
            sdl.handle_quit_event();

            if let Ok(true) = update_rx.try_recv() {
                update_notification.is_visible = true;
                update_notification.show_on_current_image(&mut current_image, sdl)?;
            }

            let elapsed_display_duration = Instant::now() - last_change;
            if elapsed_display_duration < cli.photo_change_interval {
                sleep(LOOP_SLEEP_DURATION);
                continue;
            }

            if let Ok(next_photo_result) = photo_rx.try_recv() {
                let mut next_photo =
                    load_photo_or_error_screen(next_photo_result, screen_size, cli.rotation)?;
                if update_notification.is_visible {
                    update_notification.overlay(&mut next_photo);
                }
                sdl.update_texture(next_photo.as_bytes(), TextureIndex::Next)?;
                cli.transition.play(sdl)?;

                last_change = Instant::now();

                sdl.swap_textures();
                current_image = next_photo;
            } else {
                sleep(LOOP_SLEEP_DURATION);
            }
        }
    })
}

fn photo_fetcher_thread<'a, C: Client + Clone + Send>(
    (client, cookie_store): (&'a C, &'a impl CookieStore),
    screen_size: (u32, u32),
    cli: &'a Cli,
    random: Random,
    thread_scope: &'a Scope<'a, '_>,
    photo_tx: SyncSender<Result<DynamicImage>>,
) -> Result<ScopedJoinHandle<'a, Result<()>>> {
    let mut slideshow = new_slideshow(cli)?;
    let client = client.clone();
    Ok(thread_scope.spawn(move || loop {
        let photo_result = slideshow
            .get_next_photo((&client, cookie_store), random)
            .and_then(|bytes| img::load_from_memory(&bytes).map_err(SynoPhotoFrameError::Other))
            .map(|image| image.fit_to_screen_and_add_background(screen_size, cli.rotation));
        match photo_result {
            login_error @ Err(SynoPhotoFrameError::Login(_)) => {
                /* Send the login error and break the loop to avoid deadlock and terminate the app */
                photo_tx.send(login_error)?;
                break Ok(());
            }
            /* Blocks until photo is received by the main thread */
            ok_or_not_login_error => photo_tx.send(ok_or_not_login_error)?,
        }
    }))
}

/// Return a photo or error screen from Result, unless it's a login error, in which case return
/// early from the slideshow loop.
fn load_photo_or_error_screen(
    next_photo_result: Result<DynamicImage>,
    screen_size: (u32, u32),
    rotation: Rotation,
) -> Result<DynamicImage> {
    match next_photo_result {
        login_error @ Err(SynoPhotoFrameError::Login(_)) => {
            /* Login error terminates the main thread loop */
            login_error
        }
        Err(SynoPhotoFrameError::Other(error)) => {
            /* Any other error gets logged and an error screen is displayed. */
            log::error!("{error}");
            Ok(asset::error_screen(screen_size, rotation)?)
        }
        photo @ Ok(_) => photo,
    }
}

fn new_slideshow(cli: &Cli) -> Result<Slideshow> {
    Ok(Slideshow::new(&cli.share_link)?
        .with_password(&cli.password)
        .with_ordering(cli.order)
        .with_random_start(cli.random_start)
        .with_source_size(cli.source_size))
}

#[cfg(test)]
mod tests {
    use crate::{
        api_photos::dto,
        cli::Parser,
        http::{Jar, MockResponse, StatusCode},
        sdl::MockSdl,
        test_helpers::MockClient,
    };

    use super::*;

    #[test]
    fn when_login_fails_with_api_error_then_loop_terminates() {
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";

        let mut client_stub = MockClient::new();
        client_stub.expect_clone().returning(|| {
            let mut error_response = MockResponse::new();
            error_response.expect_status().return_const(StatusCode::OK);
            error_response
                .expect_json::<dto::ApiResponse<dto::Login>>()
                .return_once(|| {
                    Ok(dto::ApiResponse {
                        success: false,
                        error: Some(dto::ApiError { code: 42 }),
                        data: None,
                    })
                });
            let mut client_clone = MockClient::new();
            client_clone
                .expect_post()
                .withf(|url, form, _| {
                    url == EXPECTED_API_URL && test_helpers::is_login_form(form, "FakeSharingId")
                })
                .return_once(|_, _, _| Ok(error_response));
            client_clone
        });
        let cli_command = format!("syno-photo-frame {SHARE_LINK} --disable-update-check");

        let result = run(
            &Cli::parse_from(cli_command.split(' ')),
            (&client_stub, &Jar::default()),
            &mut MockSdl::new().with_default_expectations(),
            DUMMY_SLEEP_AND_RANDOM,
            "1.2.3",
        );

        assert!(result.is_err());
    }

    #[test]
    fn when_login_fails_with_http_error_then_loop_terminates() {
        let mut client_stub = MockClient::new();
        client_stub.expect_clone().returning(|| {
            let mut error_response = MockResponse::new();
            error_response
                .expect_status()
                .return_const(StatusCode::FORBIDDEN);
            let mut client_clone = MockClient::new();
            client_clone
                .expect_post()
                .return_once(|_, _, _| Ok(error_response));
            client_clone
        });
        const CLI_COMMAND: &str =
            "syno-photo-frame http://fake.dsm.addr/aa/sharing/FakeSharingId --disable-update-check";

        let result = run(
            &Cli::parse_from(CLI_COMMAND.split(' ')),
            (&client_stub, &Jar::default()),
            &mut MockSdl::new().with_default_expectations(),
            DUMMY_SLEEP_AND_RANDOM,
            "1.2.3",
        );

        assert!(result.is_err());
    }

    const DUMMY_SLEEP_AND_RANDOM: (fn(Duration), Random) = (|_| (), (|_| 42, |_| ()));

    impl MockSdl {
        pub(crate) fn with_default_expectations(mut self) -> Self {
            self.expect_size().return_const((198, 102));
            self.expect_update_texture().return_const(Ok(()));
            self.expect_copy_texture_to_canvas().return_const(Ok(()));
            self.expect_fill_canvas().return_const(Ok(()));
            self.expect_present_canvas().return_const(());
            self.expect_handle_quit_event().return_const(());
            self
        }
    }
}
