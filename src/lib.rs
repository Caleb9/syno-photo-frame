//! # syno-photo-frame
//!
//! syno_photo_frame is a full-screen slideshow app for Synology Photos albums

use std::{
    ops::Range,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, Scope},
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
) -> Result<()> {
    let mut slideshow = new_slideshow(cli)?;
    let client = client.clone();
    thread_scope.spawn(move || loop {
        let photo_result = slideshow
            .get_next_photo((&client, cookie_store), random)
            .and_then(|bytes| img::load_from_memory(&bytes).map_err(SynoPhotoFrameError::Other))
            .map(|image| image.fit_to_screen_and_add_background(screen_size, cli.rotation));
        /* Blocks until photo is received by the main thread */
        photo_tx.send(photo_result).unwrap();
    });
    Ok(())
}

fn new_slideshow(cli: &Cli) -> Result<Slideshow> {
    Ok(Slideshow::new(&cli.share_link)?
        .with_password(&cli.password)
        .with_ordering(cli.order)
        .with_random_start(cli.random_start)
        .with_source_size(cli.source_size))
}

/// Return a photo or error screen from Result, unless it's a login error, in which case return
/// early from the slideshow loop.
fn load_photo_or_error_screen(
    next_photo_result: Result<DynamicImage>,
    screen_size: (u32, u32),
    rotation: Rotation,
) -> Result<DynamicImage> {
    match next_photo_result {
        login_error @ Err(SynoPhotoFrameError::Login(_)) => login_error,
        Err(SynoPhotoFrameError::Other(error)) => {
            log::error!("{error}");
            Ok(asset::error_screen(screen_size, rotation)?)
        }
        photo @ Ok(_) => photo,
    }
}
