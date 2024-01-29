//! # syno-photo-frame
//!
//! syno_photo_frame is a full-screen slideshow app for Synology Photos albums

use std::{
    ops::Range,
    path::PathBuf,
    sync::Mutex,
    thread::{self, Scope, ScopedJoinHandle},
    time::{Duration, Instant},
};

use cli::{Cli, Transition};
use error::SynoPhotoFrameError;
use http::{Client, CookieStore};
use img::{DynamicImage, Framed};
use sdl::{Sdl, TextureIndex};
use slideshow::Slideshow;

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

#[cfg(test)]
mod test_helpers;

/// Functions for randomized slideshow ordering
pub type Random = (fn(Range<u32>) -> u32, fn(&mut [u32]));

/// Slideshow loop
pub fn run(
    cli: &Cli,
    http: (&impl Client, &impl CookieStore),
    sdl: &mut impl Sdl,
    (sleep, random): (fn(Duration), Random),
    installed_version: &str,
) -> Result<(), SynoPhotoFrameError> {
    let slideshow = Mutex::new(
        Slideshow::try_from(&cli.share_link)?
            .with_password(&cli.password)
            .with_ordering(cli.order)
            .with_source_size(cli.source_size),
    );

    show_welcome_screen(sdl, &cli.splash)?;
    let photo_change_interval = Duration::from_secs(cli.interval_seconds as u64);

    thread::scope::<'_, _, Result<(), SynoPhotoFrameError>>(|thread_scope| {
        /* Initialize slideshow by getting the first photo and starting with fade-in */
        let first_photo_thread =
            get_next_photo_thread(&slideshow, http, sdl.size(), random, thread_scope);
        let check_for_updates_thread = is_update_available(
            http.0,
            installed_version,
            thread_scope,
            cli.disable_update_check,
        );

        while !(first_photo_thread.is_finished() && check_for_updates_thread.is_finished()) {
            sdl.handle_quit_event();
            /* Avoid maxing out CPU */
            const LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);
            sleep(LOOP_SLEEP_DURATION);
        }
        load_photo_from_thread_or_error_screen(first_photo_thread, sdl)?;
        let show_update_notification = check_for_updates_thread.join().unwrap();
        cli.transition.play(sdl, show_update_notification)?;
        sdl.swap_textures();

        /* Continue indefinitely */
        slideshow_loop(
            &slideshow,
            http,
            sdl,
            (
                photo_change_interval,
                cli.transition,
                show_update_notification,
            ),
            (sleep, random),
            thread_scope,
        )
    })
}

fn show_welcome_screen(
    sdl: &mut impl Sdl,
    custom_splash: &Option<PathBuf>,
) -> Result<(), SynoPhotoFrameError> {
    let welcome_img = match custom_splash {
        None => asset::welcome_image(sdl.size())?,
        Some(path) => {
            let (w, h) = sdl.size();
            match img::open(path) {
                Ok(image) => image.resize_exact(w, h, image::imageops::FilterType::Nearest),
                Err(error) => {
                    log::error!("Splashscreen {}: {error}", path.to_string_lossy());
                    asset::welcome_image(sdl.size())?
                }
            }
        }
    };
    sdl.update_texture(welcome_img.as_bytes(), TextureIndex::Current)?;
    sdl.copy_texture_to_canvas(TextureIndex::Current)?;
    sdl.present_canvas();
    Ok(())
}

fn get_next_photo_thread<'a>(
    slideshow: &'a Mutex<Slideshow>,
    (client, cookie_store): (&'a impl Client, &'a impl CookieStore),
    dimensions: (u32, u32),
    random: Random,
    thread_scope: &'a Scope<'a, '_>,
) -> ScopedJoinHandle<'a, Result<DynamicImage, SynoPhotoFrameError>> {
    let client = client.clone();

    thread_scope.spawn(move || {
        let bytes = slideshow
            .lock()
            .map_err(|error| SynoPhotoFrameError::Other(error.to_string()))?
            .get_next_photo((&client, cookie_store), random)?;
        let photo = img::load_from_memory(&bytes)?.fit_to_screen_and_add_background(dimensions);
        Ok(photo)
    })
}

fn load_photo_from_thread_or_error_screen(
    get_photo_thread: ScopedJoinHandle<'_, Result<DynamicImage, SynoPhotoFrameError>>,
    sdl: &mut impl Sdl,
) -> Result<(), SynoPhotoFrameError> {
    let photo_or_error = match get_photo_thread.join().unwrap() {
        Ok(photo) => photo,
        Err(SynoPhotoFrameError::Other(error)) => {
            log::error!("{error}");
            asset::error_image(sdl.size())?
        }
        login_error => return login_error.map(|_| ()),
    };
    sdl.update_texture(photo_or_error.as_bytes(), TextureIndex::Next)?;
    Ok(())
}

fn slideshow_loop<'a>(
    slideshow: &'a Mutex<Slideshow>,
    http: (&'a impl Client, &'a impl CookieStore),
    sdl: &mut impl Sdl,
    (photo_change_interval, transition, show_update_notification): (Duration, Transition, bool),
    (sleep, random): (fn(Duration), Random),
    thread_scope: &'a Scope<'a, '_>,
) -> Result<(), SynoPhotoFrameError> {
    let mut last_change = Instant::now();
    let mut next_photo_thread =
        get_next_photo_thread(slideshow, http, sdl.size(), random, thread_scope);
    loop {
        sdl.handle_quit_event();

        let next_photo_is_ready = next_photo_thread.is_finished();
        let elapsed_display_duration = Instant::now() - last_change;
        if elapsed_display_duration >= photo_change_interval && next_photo_is_ready {
            load_photo_from_thread_or_error_screen(next_photo_thread, sdl)?;
            transition.play(sdl, show_update_notification)?;
            last_change = Instant::now();
            sdl.swap_textures();
            next_photo_thread =
                get_next_photo_thread(slideshow, http, sdl.size(), random, thread_scope);
        } else {
            /* Avoid maxing out CPU */
            const LOOP_SLEEP_DURATION: Duration = Duration::from_secs(1);
            sleep(LOOP_SLEEP_DURATION);
        }
    }
}

fn is_update_available<'a>(
    client: &'a impl Client,
    installed_version: &'a str,
    thread_scope: &'a Scope<'a, '_>,
    skip_check: bool,
) -> ScopedJoinHandle<'a, bool> {
    let client = client.clone();
    thread_scope.spawn(move || {
        if !skip_check {
            match api_crates::get_latest_version(&client) {
                Ok(remote_crate) => {
                    if remote_crate.vers != installed_version {
                        log::info!(
                            "New version is available ({installed_version} -> {})",
                            remote_crate.vers
                        );
                        return true;
                    }
                }
                Err(error) => {
                    log::error!("Check for updates: {error}");
                }
            };
        }
        false
    })
}
