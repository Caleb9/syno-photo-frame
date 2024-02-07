//! # syno-photo-frame
//!
//! syno_photo_frame is a full-screen slideshow app for Synology Photos albums

use std::{
    ops::Range,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
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
    show_welcome_screen(sdl, &cli.splash)?;

    let slideshow = Mutex::new(
        Slideshow::try_from(&cli.share_link)?
            .with_password(&cli.password)
            .with_ordering(cli.order)
            .with_source_size(cli.source_size),
    );
    let photo_change_interval = Duration::from_secs(cli.interval_seconds as u64);
    let is_update_available = &AtomicBool::new(false);

    thread::scope::<'_, _, Result<(), SynoPhotoFrameError>>(|thread_scope| {
        if !cli.disable_update_check {
            check_for_updates(http.0, installed_version, is_update_available, thread_scope);
        }

        slideshow_loop(
            &slideshow,
            http,
            sdl,
            (photo_change_interval, cli.transition),
            is_update_available,
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

fn check_for_updates<'a>(
    client: &'a impl Client,
    installed_version: &'a str,
    is_update_available: &'a AtomicBool,
    thread_scope: &'a Scope<'a, '_>,
) -> ScopedJoinHandle<'a, ()> {
    let client = client.clone();
    thread_scope.spawn(move || {
        match api_crates::get_latest_version(&client) {
            Ok(remote_crate) => {
                if remote_crate.vers != installed_version {
                    is_update_available.store(true, Ordering::Relaxed);
                    log::info!(
                        "New version is available ({installed_version} -> {})",
                        remote_crate.vers
                    );
                }
            }
            Err(error) => {
                log::error!("Check for updates: {error}");
            }
        };
    })
}

fn slideshow_loop<'a>(
    slideshow: &'a Mutex<Slideshow>,
    http: (&'a impl Client, &'a impl CookieStore),
    sdl: &mut impl Sdl,
    (photo_change_interval, transition): (Duration, Transition),
    is_update_available: &AtomicBool,
    (sleep, random): (fn(Duration), Random),
    thread_scope: &'a Scope<'a, '_>,
) -> Result<(), SynoPhotoFrameError> {
    /* Load the first photo as soon as it's ready. */
    let mut last_change = Instant::now() - photo_change_interval;
    let mut next_photo_thread =
        get_next_photo_thread(slideshow, http, sdl.size(), random, thread_scope);
    loop {
        sdl.handle_quit_event();

        let next_photo_is_ready = next_photo_thread.is_finished();
        let elapsed_display_duration = Instant::now() - last_change;
        if elapsed_display_duration >= photo_change_interval && next_photo_is_ready {
            load_photo_from_thread_or_error_screen(next_photo_thread, sdl)?;
            transition.play(sdl, is_update_available)?;
            last_change = Instant::now();
            sdl.swap_textures();
            next_photo_thread =
                get_next_photo_thread(slideshow, http, sdl.size(), random, thread_scope);
        } else {
            /* Avoid maxing out CPU */
            const LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);
            sleep(LOOP_SLEEP_DURATION);
        }
    }
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
