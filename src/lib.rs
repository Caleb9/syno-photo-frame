use std::{
    fmt::Display,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cli::Cli;
use http::Client;
use sdl::Sdl;
use slideshow::Slideshow;
use transition::Transition;

use image::DynamicImage;
use reqwest::cookie::CookieStore;
use sdl2::event::Event;

mod api;
pub mod cli;
pub mod http;
mod img;
pub mod sdl;
mod slideshow;
mod transition;

pub fn run<C: Client, S: Sdl>(
    cli: &Cli,
    http: (&C, &Arc<dyn CookieStore>),
    sdl: &mut S,
) -> Result<(), String> {
    let slideshow = Arc::new(Mutex::new(Slideshow::try_from(&cli.share_link)?));

    let photo_change_interval = Duration::from_secs(cli.interval_seconds as u64);

    /* Initialize slideshow by getting the first photo and starting with fade-in */
    let first_photo_thread = get_next_photo_thread(&slideshow, http, sdl.size());
    while !first_photo_thread.is_finished() {
        if is_exit_requested(sdl) {
            return Ok(());
        }
        /* Avoid maxing out CPU */
        const LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);
        thread::sleep(LOOP_SLEEP_DURATION);
    }
    sdl.update_texture(first_photo_thread.join().unwrap()?.as_bytes())?;
    Transition::In.play(sdl)?;

    /* Continue indefinitely */
    slideshow_loop(http, sdl, &slideshow, photo_change_interval)
}

fn get_next_photo_thread<C: Client>(
    slideshow: &Arc<Mutex<Slideshow>>,
    (client, cookie_store): (&C, &Arc<dyn CookieStore>),
    dimensions: (u32, u32),
) -> JoinHandle<Result<DynamicImage, String>> {
    let (client, slideshow, cookie_store) =
        (client.clone(), slideshow.clone(), cookie_store.clone());
    thread::spawn(move || {
        let bytes = slideshow
            .lock()
            .map_err_to_string()?
            .get_next_photo((&client, &cookie_store))?;
        let original = image::load_from_memory(&bytes).map_err_to_string()?;
        let final_image = img::fit_to_screen_and_add_background(&original, dimensions);
        Ok(final_image)
    })
}

fn is_exit_requested<S: Sdl>(sdl: &mut S) -> bool {
    for event in sdl.events() {
        match event {
            Event::Quit { .. } => return true,
            _ => { /* ignore */ }
        }
    }
    false
}

fn slideshow_loop<C: Client, S: Sdl>(
    http: (&C, &Arc<dyn CookieStore>),
    sdl: &mut S,
    slideshow: &Arc<Mutex<Slideshow>>,
    photo_change_interval: Duration,
) -> Result<(), String> {
    let mut last_change = Instant::now();
    let mut next_photo_thread = get_next_photo_thread(slideshow, http, sdl.size());
    loop {
        if is_exit_requested(sdl) {
            break;
        }

        let next_photo_is_ready = next_photo_thread.is_finished();
        let elapsed_display_duration = Instant::now() - last_change;
        if elapsed_display_duration >= photo_change_interval && next_photo_is_ready {
            Transition::Out.play(sdl)?;
            sdl.update_texture(next_photo_thread.join().unwrap()?.as_bytes())?;
            next_photo_thread = get_next_photo_thread(&slideshow, http, sdl.size());
            Transition::In.play(sdl)?;
            last_change = Instant::now();
        } else {
            /* Avoid maxing out CPU */
            const LOOP_SLEEP_DURATION: Duration = Duration::from_secs(1);
            thread::sleep(LOOP_SLEEP_DURATION);
        }
    }
    Ok(())
}

pub trait ErrorToString<T> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: Display> ErrorToString<T> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
