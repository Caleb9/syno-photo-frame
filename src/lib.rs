use std::{
    fmt::Display,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use cli::Cli;

use http::Response;
use sdl2::{event::Event, render::Texture};
use slideshow::Slideshow;

mod api;
pub mod cli;
pub mod http;
mod img;
mod rendering;
mod slideshow;

pub fn run<P, G, R>(
    cli: &Cli,
    http: (Arc<dyn reqwest::cookie::CookieStore>, &P, &G),
) -> Result<(), String>
where
    P: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
    G: Fn(&str, &[(&str, &str)]) -> Result<R, String>,
    R: Response,
{
    let video_subsystem = rendering::init_video()?;
    let (w, h, bpp) = rendering::dimensions(&video_subsystem)?;
    let mut canvas = rendering::create_canvas(&video_subsystem, w, h)?;
    let texture_creator = canvas.texture_creator();
    let mut texture = rendering::create_texture(&texture_creator, (w, h))?;

    let mut slideshow = Slideshow::new(http, &cli.share_link)?;

    let interval = Duration::from_secs(cli.interval_seconds as u64);
    let loop_sleep = Duration::from_secs(1);

    get_next_photo(&mut slideshow, &mut texture, (w, h, bpp))?;
    let mut last = Instant::now() - interval;
    'mainloop: loop {
        for event in video_subsystem.sdl().event_pump()?.poll_iter() {
            match event {
                Event::Quit { .. } => break 'mainloop,
                _ => {}
            }
        }

        if Instant::now() - last >= interval {
            canvas.copy(&texture, None, None)?;
            canvas.present();
            last = Instant::now();
            get_next_photo(&mut slideshow, &mut texture, (w, h, bpp))?;
        } else {
            thread::sleep(loop_sleep);
        }
    }

    Ok(())
}

fn get_next_photo<P, G, R>(
    slideshow: &mut Slideshow<P, G>,
    target_texture: &mut Texture,
    (w, h, bpp): (u32, u32, usize),
) -> Result<(), String>
where
    P: Fn(&str, &[(&str, &str)], Option<(&str, &str)>) -> Result<R, String>,
    G: Fn(&str, &[(&str, &str)]) -> Result<R, String>,
    R: Response,
{
    let bytes = slideshow.get_next_photo()?;
    let original = image::load_from_memory(&bytes).map_err_to_string()?;
    let final_image = img::prepare_photo_for_display(&original, (w, h));
    target_texture.with_lock(None, rendering::image_to_texture(final_image, bpp))?;
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
