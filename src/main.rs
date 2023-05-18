use std::{error::Error, process, sync::Arc};

use clap::Parser;
use reqwest::{blocking::ClientBuilder, cookie::CookieStore};

use syno_photo_frame::{
    self,
    cli::Cli,
    http::ReqwestClient,
    sdl::{self, SdlWrapper},
};

fn main() -> Result<(), Box<dyn Error>> {
    ctrlc::set_handler(|| process::exit(0))?;

    /* HTTP client */
    let cli = Cli::parse();
    let cookie_store = Arc::new(reqwest::cookie::Jar::default());
    let client = ClientBuilder::new()
        .cookie_provider(cookie_store.clone())
        .build()?;

    /* SDL */
    let video = sdl::init_video()?;
    let display_size = sdl::display_size(&video)?;
    let canvas = sdl::create_canvas(&video, display_size)?;
    let texture_creator = canvas.texture_creator();
    let texture = sdl::create_texture(&texture_creator, display_size)?;
    let events = video.sdl().event_pump()?;
    let mut sdl = SdlWrapper::new(canvas, texture, events);

    syno_photo_frame::run(
        &cli,
        (
            &ReqwestClient::from(client),
            &(cookie_store as Arc<dyn CookieStore>),
        ),
        &mut sdl,
    )?;

    Ok(())
}
