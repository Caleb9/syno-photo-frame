use std::{
    fmt::Display,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cli::Cli;
use slideshow::Slideshow;

use http::{Client, Response};
use image::DynamicImage;
use reqwest::cookie::CookieStore;
use sdl2::event::Event;

mod api;
pub mod cli;
pub mod http;
mod img;
mod rendering;
mod slideshow;

pub fn run<C, R>(cli: &Cli, http: (&C, &Arc<dyn CookieStore>)) -> Result<(), String>
where
    C: Client<R>,
    R: Response,
{
    let video_subsystem = rendering::init_video()?;
    let (w, h, bpp) = rendering::dimensions(&video_subsystem)?;
    let dimensions = (w, h);
    let mut canvas = rendering::create_canvas(&video_subsystem, dimensions)?;
    let texture_creator = canvas.texture_creator();
    let mut texture = rendering::create_texture(&texture_creator, dimensions)?;

    let slideshow = Arc::new(Mutex::new(Slideshow::new(&cli.share_link)?));

    let photo_change_interval = Duration::from_secs(cli.interval_seconds as u64);
    let mut next_photo_thread = get_next_photo_thread(&slideshow, http, dimensions);
    let mut last_change = Instant::now() - photo_change_interval;
    'mainloop: loop {
        for event in video_subsystem.sdl().event_pump()?.poll_iter() {
            match event {
                Event::Quit { .. } => break 'mainloop,
                _ => {}
            }
        }

        let display_duration = Instant::now() - last_change;
        let next_photo_is_ready = next_photo_thread.is_finished();
        // if display_duration >= photo_change_interval && !next_photo_is_ready {
        //     println!("Still waiting for next photo");
        // }
        if display_duration >= photo_change_interval && next_photo_is_ready {
            texture.with_lock(
                None,
                rendering::image_to_texture(next_photo_thread.join().unwrap()?, bpp),
            )?;
            canvas.copy(&texture, None, None)?;
            canvas.present();
            last_change = Instant::now();
            next_photo_thread = get_next_photo_thread(&slideshow, http, dimensions);
        } else {
            const LOOP_SLEEP_DURATION: Duration = Duration::from_secs(1);
            thread::sleep(LOOP_SLEEP_DURATION);
        }
    }

    Ok(())
}

fn get_next_photo_thread<C, R>(
    slideshow: &Arc<Mutex<Slideshow>>,
    (client, cookie_store): (&C, &Arc<dyn CookieStore>),
    dimensions: (u32, u32),
) -> JoinHandle<Result<DynamicImage, String>>
where
    C: Client<R>,
    R: Response,
{
    let (client, slideshow, cookie_store) =
        (client.clone(), slideshow.clone(), cookie_store.clone());
    thread::spawn(move || {
        let bytes = slideshow
            .lock()
            .map_err_to_string()?
            .get_next_photo((&client, &cookie_store))?;
        let original = image::load_from_memory(&bytes).map_err_to_string()?;
        let final_image = img::prepare_photo_for_display(&original, dimensions);
        Ok(final_image)
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
