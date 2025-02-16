//! # ftp-photo-frame
//!
//! ftp_photo_frame is a full-screen slideshow app for FTP-hosted Photos

use std::{
    error::Error,
    fmt::{Display, Formatter},
    ops::Range,
    process::Command,
    sync::mpsc::{self, SyncSender},
    thread::{self, Scope, ScopedJoinHandle},
    time::Duration,
};
use std::{thread::sleep as thread_sleep, time::Instant};
use rppal::gpio::{Gpio, InputPin};

use crate::{
    cli::{Cli, Rotation},
    error::FrameError,
    img::{DynamicImage, Framed},
    sdl::{Sdl, TextureIndex},
    slideshow::{Slideshow, SlideshowError},
};

pub mod cli;
pub mod error;
pub mod logging;
pub mod sdl;

mod asset;
mod img;
mod slideshow;
mod transition;

pub type FrameResult<T> = Result<T, FrameError>;

/// Functions for randomized slideshow ordering
pub type Random = (fn(Range<u32>) -> u32, fn(&mut [u32]));

#[derive(Clone, Debug)]
pub struct QuitEvent;

/// Slideshow loop
pub fn run(cli: &Cli, sdl: &mut impl Sdl, random: Random) -> FrameResult<()> {
    let current_image = show_welcome_screen(cli, sdl)?;

    thread::scope::<'_, _, FrameResult<()>>(|_| slideshow_loop(cli, sdl, random, current_image))
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

enum DisplayMode {
    Show,
    Standby,
}

const NO_MOTION_STANDBY_DURATION: Duration = Duration::from_secs(60);
const GPIO_MOTION: u8 = 23;

fn slideshow_loop(
    cli: &Cli,
    sdl: &mut impl Sdl,
    random: Random,
    mut current_image: DynamicImage,
) -> FrameResult<()> {
    /* Load the first photo as soon as it's ready. */
    let motion_pin: Option<InputPin> = if cli.motionsensor {
        Some(
            Gpio::new()
                .unwrap()
                .get(GPIO_MOTION)
                .unwrap()
                .into_input_pulldown(),
        )
    } else {
        None
    };
    let mut display_mode = DisplayMode::Show;
    let mut last_activation = Instant::now();
    let mut last_change = Instant::now() - cli.photo_change_interval;
    let screen_size = sdl.size();
    let (photo_sender, photo_receiver) = mpsc::sync_channel(1);
    const LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);
    const LOOP_STANDBY_DURATION: Duration = Duration::from_millis(10);

    thread::scope::<'_, _, FrameResult<()>>(|thread_scope| {
        photo_fetcher_thread(cli, screen_size, random, thread_scope, photo_sender)?;

        let loop_result = loop {
            sdl.handle_quit_event()?;

            // Has motion been detected recently?
            let mut motion = true;
            if motion_pin.is_some() {
                motion = motion_pin.as_ref().unwrap().is_high();
                if motion {
                    last_activation = Instant::now();
                }
            }

            match display_mode {
                DisplayMode::Show => {
                    // Long time no motion?
                    let elapsed_no_motion_duration = Instant::now() - last_activation;
                    if elapsed_no_motion_duration > NO_MOTION_STANDBY_DURATION {
                        // Turn Display into standby mode
                        Command::new("vcgencmd")
                            .arg("display_power")
                            .arg("0")
                            .output()
                            .expect("failed to execute process");
                        display_mode = DisplayMode::Standby;
                        continue;
                    }

                    // Sleep unless interval is reached
                    let elapsed_display_duration = Instant::now() - last_change;
                    if elapsed_display_duration < cli.photo_change_interval {
                        thread_sleep(LOOP_SLEEP_DURATION);
                        continue;
                    }

                    if let Ok(next_photo_result) = photo_receiver.try_recv() {
                        let next_image = match next_photo_result {
                            Err(SlideshowError::Other(error)) => {
                                /* Login error terminates the main thread loop */
                                break Err(FrameError::Other(error.to_string()));
                            }
                            ok_or_other_error => load_photo_or_error_screen(
                                ok_or_other_error,
                                screen_size,
                                cli.rotation,
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
                }
                DisplayMode::Standby => {
                    if motion {
                        Command::new("vcgencmd")
                            .arg("display_power")
                            .arg("1")
                            .output()
                            .expect("failed to execute process");
                        display_mode = DisplayMode::Show;
                    } else {
                        thread_sleep(LOOP_STANDBY_DURATION);
                    }
                }
            }
        };
        if loop_result.is_err() {
            /* Dropping the receiver terminates photo_fetcher_thread loop */
            drop(photo_receiver);
        }
        loop_result
    })
}

fn photo_fetcher_thread<'a>(
    cli: &'a Cli,
    screen_size: (u32, u32),
    random: Random,
    thread_scope: &'a Scope<'a, '_>,
    photo_sender: SyncSender<Result<DynamicImage, SlideshowError>>,
) -> Result<ScopedJoinHandle<'a, ()>, String> {
    let mut slideshow = new_slideshow(cli)?;
    Ok(thread_scope.spawn(move || loop {
        let photo_result = slideshow
            .get_next_photo(random)
            .and_then(|bytes| img::load_from_memory(&bytes).map_err(SlideshowError::Other))
            .map(|image| image.fit_to_screen_and_add_background(screen_size, cli.rotation));
        /* Blocks until photo is received by the main thread */
        let send_result = photo_sender.send(photo_result);
        if send_result.is_err() {
            break;
        }
    }))
}

fn new_slideshow(cli: &Cli) -> Result<Slideshow, String> {
    Ok(Slideshow::build(&cli.server, &cli.folder, &cli.user)?
        .with_password(&cli.password)
        .with_ordering(cli.order)
        .with_random_start(cli.random_start)
        .with_source_size(cli.source_size))
}

fn load_photo_or_error_screen(
    next_photo_result: Result<DynamicImage, SlideshowError>,
    screen_size: (u32, u32),
    rotation: Rotation,
) -> FrameResult<DynamicImage> {
    let next_image = match next_photo_result {
        Ok(photo) => photo,
        Err(SlideshowError::Other(error)) => {
            /* Any non-login error gets logged and an error screen is displayed. */
            log::error!("{error}");
            asset::error_screen(screen_size, rotation)?
        }
    };
    Ok(next_image)
}

impl Display for QuitEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Quit")
    }
}

impl Error for QuitEvent {}
