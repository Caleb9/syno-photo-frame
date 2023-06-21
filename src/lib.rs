use std::{
    fmt::Display,
    ops::Range,
    sync::{Arc, Mutex},
    thread::{self, Scope, ScopedJoinHandle},
    time::{Duration, Instant},
};

use cli::Cli;
use http::{Client, CookieStore};
use img::{DynamicImage, Framed};
use sdl::{Event, Sdl};
use slideshow::Slideshow;
use transition::Transition;

pub mod cli;
pub mod http;
pub mod logging;
pub mod sdl;

mod api;
mod asset;
mod img;
mod slideshow;
mod transition;

#[cfg(test)]
mod test_helpers;

pub type Random = (fn(Range<u32>) -> u32, fn(&mut [u32]));

pub fn run(
    cli: &Cli,
    http: (&impl Client, &Arc<dyn CookieStore>),
    sdl: &mut impl Sdl,
    sleep: fn(Duration),
    random: Random,
) -> Result<(), String> {
    let slideshow = Arc::new(Mutex::new(
        Slideshow::try_from(&cli.share_link)?.with_ordering(cli.order),
    ));

    let photo_change_interval = Duration::from_secs(cli.interval_seconds as u64);

    show_welcome_screen(sdl)?;

    /* Initialize slideshow by getting the first photo and starting with fade-in */
    thread::scope::<'_, _, Result<(), String>>(|thread_scope| {
        let first_photo_thread =
            get_next_photo_thread(&slideshow, http, sdl.size(), random, thread_scope);
        while !first_photo_thread.is_finished() {
            if is_exit_requested(sdl) {
                return Ok(());
            }
            /* Avoid maxing out CPU */
            const LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);
            sleep(LOOP_SLEEP_DURATION);
        }
        load_photo_from_thread_or_error_screen(first_photo_thread, sdl)
    })?;
    Transition::In.play(sdl)?;

    /* Continue indefinitely */
    slideshow_loop(http, sdl, &slideshow, photo_change_interval, sleep, random)
}

fn show_welcome_screen(sdl: &mut impl Sdl) -> Result<(), String> {
    sdl.update_texture(asset::welcome_image(sdl.size())?.as_bytes())?;
    sdl.copy_texture_to_canvas()?;
    sdl.present_canvas();
    Ok(())
}

fn get_next_photo_thread<'a>(
    slideshow: &Arc<Mutex<Slideshow>>,
    (client, cookie_store): (&'a impl Client, &Arc<dyn CookieStore>),
    dimensions: (u32, u32),
    random: Random,
    thread_scope: &'a Scope<'a, '_>,
) -> ScopedJoinHandle<'a, Result<DynamicImage, String>> {
    let (client, slideshow, cookie_store) =
        (client.clone(), slideshow.clone(), cookie_store.clone());

    thread_scope.spawn(move || {
        let bytes = slideshow
            .lock()
            .map_err_to_string()?
            .get_next_photo((&client, &cookie_store), random)?;
        let photo = img::load_from_memory(&bytes)?.fit_to_screen_and_add_background(dimensions);
        Ok(photo)
    })
}

fn is_exit_requested(sdl: &mut impl Sdl) -> bool {
    sdl.events().any(|e| match e {
        event @ (Event::Quit { .. } | Event::AppTerminating { .. }) => {
            log::debug!("SDL event received: {event:?}");
            true
        }
        _ => false,
    })
}

fn load_photo_from_thread_or_error_screen(
    get_photo_thread: ScopedJoinHandle<'_, Result<DynamicImage, String>>,
    sdl: &mut impl Sdl,
) -> Result<(), String> {
    match get_photo_thread.join().unwrap() {
        Ok(photo) => sdl.update_texture(photo.as_bytes()),
        Err(error) => {
            log::error!("{error}");
            sdl.update_texture(asset::error_image(sdl.size())?.as_bytes())
        }
    }
}

fn slideshow_loop(
    http: (&impl Client, &Arc<dyn CookieStore>),
    sdl: &mut impl Sdl,
    slideshow: &Arc<Mutex<Slideshow>>,
    photo_change_interval: Duration,
    sleep: fn(Duration),
    random: Random,
) -> Result<(), String> {
    thread::scope(|thread_scope| {
        let mut last_change = Instant::now();
        let mut next_photo_thread =
            get_next_photo_thread(slideshow, http, sdl.size(), random, thread_scope);
        loop {
            if is_exit_requested(sdl) {
                break;
            }

            let next_photo_is_ready = next_photo_thread.is_finished();
            let elapsed_display_duration = Instant::now() - last_change;
            if elapsed_display_duration >= photo_change_interval && next_photo_is_ready {
                Transition::Out.play(sdl)?;
                load_photo_from_thread_or_error_screen(next_photo_thread, sdl)?;
                next_photo_thread =
                    get_next_photo_thread(slideshow, http, sdl.size(), random, thread_scope);
                Transition::In.play(sdl)?;
                last_change = Instant::now();
            } else {
                /* Avoid maxing out CPU */
                const LOOP_SLEEP_DURATION: Duration = Duration::from_secs(1);
                sleep(LOOP_SLEEP_DURATION);
            }
        }
        Ok(())
    })
}

pub trait ErrorToString<T> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: Display> ErrorToString<T> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
