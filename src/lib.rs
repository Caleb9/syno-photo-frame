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
pub mod sdl;

mod api;
mod img;
mod slideshow;
mod transition;

#[cfg(test)]
mod test_helpers;

pub type Random = (fn(Range<u32>) -> u32, fn(&mut [u32]));

pub fn run<C: Client, S: Sdl>(
    cli: &Cli,
    http: (&C, &Arc<dyn CookieStore>),
    sdl: &mut S,
    sleep: fn(Duration),
    random: Random,
) -> Result<(), String> {
    let slideshow = Arc::new(Mutex::new(
        Slideshow::try_from(&cli.share_link)?.with_ordering(cli.order),
    ));

    let photo_change_interval = Duration::from_secs(cli.interval_seconds as u64);

    thread::scope::<'_, _, Result<(), String>>(|thread_scope| {
        /* Initialize slideshow by getting the first photo and starting with fade-in */
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
        sdl.update_texture(first_photo_thread.join().unwrap()?.as_bytes())?;
        Ok(())
    })?;
    Transition::In.play(sdl)?;

    /* Continue indefinitely */
    slideshow_loop(http, sdl, &slideshow, photo_change_interval, sleep, random)
}

fn get_next_photo_thread<'a, C: Client + 'a>(
    slideshow: &Arc<Mutex<Slideshow>>,
    (client, cookie_store): (&C, &Arc<dyn CookieStore>),
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

fn is_exit_requested<S: Sdl>(sdl: &mut S) -> bool {
    for event in sdl.events() {
        if let Event::Quit { .. } = event {
            return true;
        }
    }
    false
}

fn slideshow_loop<C: Client, S: Sdl>(
    http: (&C, &Arc<dyn CookieStore>),
    sdl: &mut S,
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
                sdl.update_texture(next_photo_thread.join().unwrap()?.as_bytes())?;
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

pub(crate) trait ErrorToString<T> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: Display> ErrorToString<T> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
