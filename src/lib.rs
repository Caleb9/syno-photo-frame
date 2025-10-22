//! # syno-photo-frame
//!
//! syno_photo_frame is a full-screen slideshow app for Synology Photos and Immich albums

pub use {api_client::LoginError, rand::RandomImpl};

use std::{
    error::Error,
    fmt::{Display, Formatter},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, Scope, ScopedJoinHandle},
    time::Duration,
};

#[cfg(not(test))]
use std::{thread::sleep as thread_sleep, time::Instant};
#[cfg(test)]
use {mock_instant::Instant, test_helpers::fake_sleep as thread_sleep};

use anyhow::{Result, bail};
use chrono::Locale;

use crate::{
    api_client::{ApiClient, immich_client::ImmichApiClient, syno_client::SynoApiClient},
    cli::{Backend, Cli},
    env::Env,
    http::{CookieStore, HttpClient},
    img::{DynamicImage, Framed},
    metadata::FromEnv,
    rand::Random,
    sdl::{Sdl, TextureIndex},
    slideshow::Slideshow,
    update::UpdateNotification,
};

pub mod cli;
pub mod env;
pub mod error;
pub mod http;
pub mod logging;
pub mod sdl;

mod api_client;
mod api_crates;
mod asset;
mod img;
mod info_box;
mod metadata;
mod rand;
mod slideshow;
mod transition;
mod update;

#[cfg(test)]
mod test_helpers;

/// Slideshow loop
pub fn run<H, R>(
    cli: &Cli,
    (http_client, cookie_store): (&H, &impl CookieStore),
    sdl: &mut impl Sdl,
    random: R,
    this_crate_version: &str,
    env: &impl Env,
) -> Result<()>
where
    H: HttpClient + Sync,
    R: Random + Send,
{
    let current_image = show_welcome_screen(cli, sdl)?;

    thread::scope::<'_, _, Result<()>>(|thread_scope| {
        let (update_check_sender, update_check_receiver) = mpsc::sync_channel(1);
        if !cli.disable_update_check {
            update::check_for_updates_thread(
                http_client,
                this_crate_version,
                thread_scope,
                update_check_sender,
            );
        }

        select_backend_and_start_slideshow(
            cli,
            (http_client, cookie_store),
            sdl,
            random,
            update_check_receiver,
            current_image,
            env,
        )
    })
}

fn show_welcome_screen(cli: &Cli, sdl: &mut impl Sdl) -> Result<DynamicImage> {
    let welcome_img = if let Some(path) = &cli.splash {
        let (w, h) = sdl.size();
        match img::open(path) {
            Ok(image) => image.resize_exact(w, h, image::imageops::FilterType::Nearest),
            Err(error) => {
                log::error!("Splashscreen {}: {error}", path.to_string_lossy());
                asset::welcome_screen(sdl.size(), cli.rotation)?
            }
        }
    } else {
        asset::welcome_screen(sdl.size(), cli.rotation)?
    };
    sdl.update_texture(welcome_img.as_bytes(), TextureIndex::Current)?;
    sdl.copy_texture_to_canvas(TextureIndex::Current)?;
    sdl.present_canvas();
    Ok(welcome_img)
}

fn select_backend_and_start_slideshow<H, R>(
    cli: &Cli,
    (http_client, cookie_store): (&H, &impl CookieStore),
    sdl: &mut impl Sdl,
    random: R,
    update_check_receiver: Receiver<bool>,
    current_image: DynamicImage,
    env: &impl Env,
) -> Result<()>
where
    H: HttpClient + Sync,
    R: Random + Send,
{
    let backend = if matches!(cli.backend, Backend::Auto) {
        api_client::detect_backend(&cli.share_link)?
    } else {
        cli.backend
    };
    match backend {
        Backend::Synology => slideshow_loop(
            cli,
            SynoApiClient::build(http_client, cookie_store, &cli.share_link)?
                .with_password(&cli.password),
            sdl,
            random,
            update_check_receiver,
            current_image,
            env,
        ),
        Backend::Immich => slideshow_loop(
            cli,
            ImmichApiClient::build(http_client, &cli.share_link)?.with_password(&cli.password),
            sdl,
            random,
            update_check_receiver,
            current_image,
            env,
        ),
        Backend::Auto => unreachable!(),
    }
}

fn slideshow_loop<A, R>(
    cli: &Cli,
    api_client: A,
    sdl: &mut impl Sdl,
    random: R,
    update_check_receiver: Receiver<bool>,
    mut current_image: DynamicImage,
    env: &impl Env,
) -> Result<()>
where
    A: ApiClient + Send,
    R: Random + Send,
{
    /* Load the first photo as soon as it's ready. */
    let mut last_change = Instant::now() - cli.photo_change_interval;
    let screen_size = sdl.size();
    let mut update_notification = UpdateNotification::new(screen_size, cli.rotation)?;
    let (photo_sender, photo_receiver) = mpsc::sync_channel(1);
    const LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);

    thread::scope::<'_, _, Result<()>>(|thread_scope| {
        photo_fetcher_thread(
            cli,
            api_client,
            screen_size,
            random,
            thread_scope,
            photo_sender,
            env,
        )?;

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
                let mut next_photo = match next_photo_result {
                    Ok(photo) => photo,
                    Err(error) if error.is::<LoginError>() => {
                        /* Login error terminates the main thread loop */
                        break Err(error);
                    }
                    Err(error) => {
                        /* Any non-login error gets logged and an error screen is displayed. */
                        log::error!("{error}");
                        DynamicImagePhoto::error(screen_size, cli.rotation)?
                    }
                };
                if update_notification.is_visible {
                    update_notification.overlay(&mut next_photo.image);
                }
                sdl.update_texture(next_photo.image.as_bytes(), TextureIndex::Next)?;
                cli.transition.play(sdl)?;
                overlay_info_box(sdl, &next_photo, cli)?;

                last_change = Instant::now();

                sdl.swap_textures();
                current_image = next_photo.image;
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

fn photo_fetcher_thread<'a, A, R>(
    cli: &'a Cli,
    api_client: A,
    screen_size: (u32, u32),
    random: R,
    thread_scope: &'a Scope<'a, '_>,
    photo_sender: SyncSender<Result<DynamicImagePhoto>>,
    env: &impl Env,
) -> Result<ScopedJoinHandle<'a, ()>>
where
    A: ApiClient + Send + 'a,
    R: Random + Send + 'a,
{
    if !api_client.is_logged_in() {
        api_client.login()?;
    }
    let mut slideshow = Slideshow::new(api_client, random, Locale::from_env(env))
        .with_ordering(cli.order)
        .with_random_start(cli.random_start)
        .with_source_size(cli.source_size);
    Ok(thread_scope.spawn(move || {
        loop {
            let photo_result = slideshow.get_next_photo().and_then(|photo| {
                load_image_from_memory(&photo.bytes).map(|image| {
                    DynamicImagePhoto::new(
                        image.fit_to_screen_and_add_background(
                            screen_size,
                            cli.rotation,
                            cli.background,
                        ),
                        photo.info,
                    )
                })
            });
            /* Blocks until photo is received by the main thread */
            let send_result = photo_sender.send(photo_result);
            if send_result.is_err() {
                break;
            }
        }
    }))
}

fn load_image_from_memory(bytes: &[u8]) -> Result<DynamicImage> {
    img::load_from_memory(bytes)
        /* Synology Photos API may respond with an http OK code and a JSON containing an
         * error instead of image bytes in the response body. Log such responses for
         * debugging. */
        .or_else(|e| {
            let is_json = serde_json::from_slice::<serde::de::IgnoredAny>(bytes).is_ok();
            if !is_json {
                return Err(e);
            }
            let json = String::from_utf8_lossy(bytes);
            bail!("Failed to decode image bytes. Received the following data: {json}");
        })
}

struct DynamicImagePhoto {
    image: DynamicImage,
    info: String,
}

impl DynamicImagePhoto {
    fn new(image: DynamicImage, info: String) -> Self {
        Self { image, info }
    }

    fn error(screen_size: (u32, u32), rotation: cli::Rotation) -> Result<Self> {
        Ok(Self {
            image: asset::error_screen(screen_size, rotation)?,
            info: "".to_string(),
        })
    }
}

fn overlay_info_box(sdl: &mut impl Sdl, photo: &DynamicImagePhoto, cli: &Cli) -> Result<()> {
    if cli.display_photo_info {
        sdl.render_info_box(&photo.info, cli.rotation)?;
        sdl.present_canvas();
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct QuitEvent;

impl Display for QuitEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Quit")
    }
}

impl Error for QuitEvent {}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use mock_instant::MockClock;
    use syno_api::dto::{ApiResponse, Error, List};

    use super::*;
    use crate::{
        api_client::syno_client::Login,
        cli::Parser,
        env::MockEnv,
        http::{Jar, MockHttpResponse, StatusCode},
        sdl::MockSdl,
        test_helpers::{MockHttpClient, rand::FakeRandom},
    };

    #[test]
    fn when_login_fails_with_api_error_then_loop_terminates() {
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";
        const EXPECTED_API_URL: &str = "http://fake.dsm.addr/aa/sharing/webapi/entry.cgi";

        let mut client_stub = MockHttpClient::new();
        client_stub
            .expect_post()
            .withf(|url, form, _| {
                url == EXPECTED_API_URL && test_helpers::is_login_form(form, "FakeSharingId")
            })
            .returning(|_, _, _| {
                let mut error_response = test_helpers::new_ok_response();
                error_response
                    .expect_json::<ApiResponse<Login>>()
                    .return_once(|| {
                        Ok(ApiResponse {
                            success: false,
                            error: Some(Error { code: 42 }),
                            data: None,
                        })
                    });
                Ok(error_response)
            });
        /* Avoid overflow when setting initial last_change */
        const DISPLAY_INTERVAL: u64 = 30;
        MockClock::set_time(Duration::from_secs(DISPLAY_INTERVAL));
        let mut sdl_stub = MockSdl::new().with_default_expectations();
        /* Hack: Break the loop eventually in case of assertion failure */
        sdl_stub
            .expect_handle_quit_event()
            .times(..5000)
            .returning(|| Ok(()));
        let cli_command = format!(
            "syno-photo-frame {SHARE_LINK} \
             --interval {DISPLAY_INTERVAL} \
             --disable-update-check \
             --splash assets/test_loading.jpeg"
        );

        let result = run(
            &Cli::parse_from(cli_command.split(' ')),
            (&client_stub, &Jar::default()),
            &mut sdl_stub,
            FakeRandom::default(),
            "1.2.3",
            &MockEnv::default(),
        );

        assert!(result.is_err_and(|e| e.is::<LoginError>()));
        client_stub.checkpoint();
    }

    #[test]
    fn when_login_fails_with_http_error_then_loop_terminates() {
        let mut client_stub = MockHttpClient::new();
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
        /* Hack: Break the loop eventually in case of assertion failure */
        sdl_stub
            .expect_handle_quit_event()
            .times(..5000)
            .returning(|| Ok(()));
        let cli_command = format!(
            "syno-photo-frame http://fake.dsm.addr/aa/sharing/FakeSharingId \
            --interval {DISPLAY_INTERVAL} \
            --disable-update-check \
            --splash assets/test_loading.jpeg"
        );

        let result = run(
            &Cli::parse_from(cli_command.split(' ')),
            (&client_stub, &Jar::default()),
            &mut sdl_stub,
            FakeRandom::default(),
            "1.2.3",
            &MockEnv::default(),
        );

        assert!(result.is_err_and(|e| e.is::<LoginError>()));
        client_stub.checkpoint();
    }

    #[test]
    fn when_getting_photo_fails_with_http_error_loop_continues() {
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";

        let mut client_stub = MockHttpClient::new();
        client_stub
            .expect_post()
            .withf(|_, form, _| test_helpers::is_login_form(form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_success_response_with_json(Login {})));
        client_stub
            .expect_post()
            .withf(|_, form, _| test_helpers::is_list_form(form))
            .returning(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![
                        test_helpers::new_photo_dto(1, "missing_photo1"),
                        test_helpers::new_photo_dto(2, "photo2"),
                    ],
                }))
            });
        /* Simulate failing GET photo bytes request */
        client_stub
            .expect_get()
            .withf(|_, form| {
                test_helpers::is_get_photo_form(form, "FakeSharingId", "1", "missing_photo1", "xl")
            })
            .returning(|_, _| {
                let mut error_response = MockHttpResponse::new();
                error_response
                    .expect_status()
                    .return_const(StatusCode::NOT_FOUND);
                Ok(error_response)
            });
        client_stub
            .expect_get()
            .withf(|_, form| {
                test_helpers::is_get_photo_form(form, "FakeSharingId", "2", "photo2", "xl")
            })
            .returning(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });

        /* Avoid overflow when setting initial last_change */
        const DISPLAY_INTERVAL: u64 = 30;
        MockClock::set_time(Duration::from_secs(DISPLAY_INTERVAL));
        let mut sdl_stub = MockSdl::new();
        {
            sdl_stub.expect_size().return_const((198, 102));
            sdl_stub.expect_clear_canvas().return_const(());
            sdl_stub
                .expect_copy_texture_to_canvas()
                .returning(|_| Ok(()));
            sdl_stub.expect_fill_canvas().returning(|_| Ok(()));
            sdl_stub.expect_present_canvas().return_const(());
            sdl_stub.expect_update_texture().returning(|_, _| Ok(()));
        }
        sdl_stub.expect_swap_textures().returning(|| {
            MockClock::advance(Duration::from_secs(1));
        });
        sdl_stub.expect_handle_quit_event().returning(|| {
            /* Until swap_textures is called (with an error image) and advances the time, return
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
            --disable-update-check \
            --transition none \
            --splash assets/test_loading.jpeg"
        );

        // let _ = SimpleLogger::new().init(); /* cargo test -- --show-output */
        let result = run(
            &Cli::parse_from(cli_command.split(' ')),
            (&client_stub, &Jar::default()),
            &mut sdl_stub,
            FakeRandom::default(),
            "1.2.3",
            &MockEnv::default().with_default_expectations(),
        );

        /* If failed request bubbled up its error and broke the main slideshow loop, we would
         * observe it here as the error type would be different from Quit */
        assert!(result.is_err_and(|e| e.is::<QuitEvent>()));
        client_stub.checkpoint();
    }

    #[test]
    fn when_getting_photo_fails_with_api_error_loop_continues() {
        const SHARE_LINK: &str = "http://fake.dsm.addr/aa/sharing/FakeSharingId";

        let mut client_stub = MockHttpClient::new();
        client_stub
            .expect_post()
            .withf(|_, form, _| test_helpers::is_login_form(form, "FakeSharingId"))
            .return_once(|_, _, _| Ok(test_helpers::new_success_response_with_json(Login {})));
        client_stub
            .expect_post()
            .withf(|_, form, _| test_helpers::is_list_form(form))
            .returning(|_, _, _| {
                Ok(test_helpers::new_success_response_with_json(List {
                    list: vec![
                        test_helpers::new_photo_dto(1, "bad_photo1"),
                        test_helpers::new_photo_dto(2, "photo2"),
                    ],
                }))
            });
        /* Simulate failing GET photo bytes request */
        client_stub
            .expect_get()
            .withf(|_, form| {
                test_helpers::is_get_photo_form(form, "FakeSharingId", "1", "bad_photo1", "xl")
            })
            .returning(|_, _| {
                let mut error_response = MockHttpResponse::new();
                error_response.expect_status().return_const(StatusCode::OK);
                error_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from("{ \"bad\": \"data\" }")));
                Ok(error_response)
            });
        client_stub
            .expect_get()
            .withf(|_, form| {
                test_helpers::is_get_photo_form(form, "FakeSharingId", "2", "photo2", "xl")
            })
            .returning(|_, _| {
                let mut get_photo_response = test_helpers::new_ok_response();
                get_photo_response
                    .expect_bytes()
                    .return_once(|| Ok(Bytes::from_static(&[])));
                Ok(get_photo_response)
            });

        /* Avoid overflow when setting initial last_change */
        const DISPLAY_INTERVAL: u64 = 30;
        MockClock::set_time(Duration::from_secs(DISPLAY_INTERVAL));
        let mut sdl_stub = MockSdl::new();
        {
            sdl_stub.expect_size().return_const((198, 102));
            sdl_stub.expect_clear_canvas().return_const(());
            sdl_stub
                .expect_copy_texture_to_canvas()
                .returning(|_| Ok(()));
            sdl_stub.expect_fill_canvas().returning(|_| Ok(()));
            sdl_stub.expect_present_canvas().return_const(());
            sdl_stub.expect_update_texture().returning(|_, _| Ok(()));
        }
        sdl_stub.expect_swap_textures().returning(|| {
            MockClock::advance(Duration::from_secs(1));
        });
        sdl_stub.expect_handle_quit_event().returning(|| {
            /* Until swap_textures is called (with an error image) and advances the time, return
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
            --disable-update-check \
            --transition none \
            --splash assets/test_loading.jpeg"
        );

        // let _ = SimpleLogger::new().init(); /* cargo test -- --show-output */
        let result = run(
            &Cli::parse_from(cli_command.split(' ')),
            (&client_stub, &Jar::default()),
            &mut sdl_stub,
            FakeRandom::default(),
            "1.2.3",
            &MockEnv::default().with_default_expectations(),
        );

        /* If failed request bubbled up its error and broke the main slideshow loop, we would
         * observe it here as the error type would be different from Quit */
        assert!(result.is_err_and(|e| e.is::<QuitEvent>()));
        client_stub.checkpoint();
    }

    impl MockSdl {
        pub fn with_default_expectations(mut self) -> Self {
            self.expect_size().return_const((198, 102));
            self.expect_update_texture().returning(|_, _| Ok(()));
            self.expect_copy_texture_to_canvas().returning(|_| Ok(()));
            self.expect_fill_canvas().returning(|_| Ok(()));
            self.expect_present_canvas().return_const(());
            self
        }
    }

    impl MockEnv {
        pub fn with_default_expectations(mut self) -> Self {
            self.expect_var().returning(|_| Ok("en_GB".to_string()));
            self
        }
    }
}
