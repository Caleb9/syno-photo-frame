use std::{
    fmt::Display,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cli::Cli;
use http::Client;
use slideshow::Slideshow;
use transition::Transition;

use image::DynamicImage;
use reqwest::cookie::CookieStore;
use sdl2::{
    event::Event,
    render::{Canvas, Texture},
    video::Window,
    Sdl, VideoSubsystem,
};

mod api;
pub mod cli;
pub mod http;
mod img;
mod rendering;
mod slideshow;
mod transition;

const LOOP_SLEEP_DURATION: Duration = Duration::from_secs(1);

pub fn run<C: Client>(cli: &Cli, http: (&C, &Arc<dyn CookieStore>)) -> Result<(), String> {
    let video_subsystem = rendering::init_video()?;
    let (w, h, bpp) = rendering::dimensions(&video_subsystem)?;
    // let (w, h) = (w / 4, h / 4);
    let dimensions = (w, h);
    let mut canvas = rendering::create_canvas(&video_subsystem, dimensions)?;
    let texture_creator = canvas.texture_creator();
    let mut texture = rendering::create_texture(&texture_creator, dimensions)?;

    let slideshow = Arc::new(Mutex::new(Slideshow::try_from(&cli.share_link)?));

    let photo_change_interval = Duration::from_secs(cli.interval_seconds as u64);

    if start_slideshow(
        &slideshow,
        http,
        (w, h, bpp),
        &video_subsystem,
        &mut texture,
        &mut canvas,
    )? {
        return Ok(());
    }
    let mut last_change = Instant::now();
    let mut next_photo_thread = get_next_photo_thread(&slideshow, http, dimensions);

    loop {
        if is_exit_requested(&video_subsystem.sdl())? {
            break;
        }

        let next_photo_is_ready = next_photo_thread.is_finished();
        let elapsed_display_duration = Instant::now() - last_change;
        if elapsed_display_duration >= photo_change_interval && next_photo_is_ready {
            Transition::Out.play(&mut canvas, &texture, &video_subsystem.sdl())?;
            texture
                .update(
                    None,
                    next_photo_thread.join().unwrap()?.as_bytes(),
                    w as usize * bpp,
                )
                .map_err_to_string()?;
            next_photo_thread = get_next_photo_thread(&slideshow, http, dimensions);
            Transition::In.play(&mut canvas, &texture, &video_subsystem.sdl())?;
            last_change = Instant::now();
        } else {
            /* Sleep for a second to avoid maxing out CPU */
            thread::sleep(LOOP_SLEEP_DURATION);
        }
    }

    Ok(())
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

fn start_slideshow<C: Client>(
    slideshow: &Arc<Mutex<Slideshow>>,
    http: (&C, &Arc<dyn CookieStore>),
    (w, h, bpp): (u32, u32, usize),
    video_subsystem: &VideoSubsystem,
    texture: &mut Texture,
    canvas: &mut Canvas<Window>,
) -> Result<bool, String> {
    let next_photo_thread = get_next_photo_thread(&slideshow, http, (w, h));
    let sdl = video_subsystem.sdl();
    while !next_photo_thread.is_finished() {
        if is_exit_requested(&sdl)? {
            return Ok(true);
        }
        /* Sleep for a second to avoid maxing out CPU */
        thread::sleep(LOOP_SLEEP_DURATION);
    }
    texture
        .update(
            None,
            next_photo_thread.join().unwrap()?.as_bytes(),
            w as usize * bpp,
        )
        .map_err_to_string()?;
    Transition::In.play(canvas, texture, &sdl)?;
    Ok(false)
}

fn is_exit_requested(sdl: &Sdl) -> Result<bool, String> {
    for event in sdl.event_pump()?.poll_iter() {
        match event {
            Event::Quit { .. } => return Ok(true),
            _ => { /* ignore */ }
        }
    }
    Ok(false)
}

pub trait ErrorToString<T> {
    fn map_err_to_string(self) -> Result<T, String>;
}

impl<T, E: Display> ErrorToString<T> for Result<T, E> {
    fn map_err_to_string(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
