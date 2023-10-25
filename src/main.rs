use std::{error::Error, process, sync::Arc, thread, time::Duration};

use log::LevelFilter;
use rand::{self, seq::SliceRandom, Rng};
use simple_logger::SimpleLogger;

use syno_photo_frame::{
    self,
    cli::{Cli, Parser},
    http::{ClientBuilder, CookieStore, ReqwestClient},
    logging::LoggingClientDecorator,
    sdl::{self, SdlWrapper},
    ErrorToString, Random,
};

fn main() -> Result<(), Box<dyn Error>> {
    ctrlc::set_handler(|| {
        log::debug!("ctrlc signal received, exiting");
        process::exit(0);
    })?;

    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .env()
        .init()?;

    match init_and_run() {
        Err(error) => {
            log::error!("{error}");
            Err(error)?
        }
        _ => Ok(()),
    }
}

fn init_and_run() -> Result<(), String> {
    let cli = Cli::parse();

    /* HTTP client */
    let cookie_store = Arc::new(reqwest::cookie::Jar::default());
    let client = ClientBuilder::new()
        .cookie_provider(cookie_store.clone())
        .timeout(Duration::from_secs(cli.timeout_seconds as u64))
        .build()
        .map_err_to_string()?;

    /* SDL */
    let video = sdl::init_video()?;
    let display_size = sdl::display_size(&video)?;
    let canvas = sdl::create_canvas(&video, display_size)?;
    let texture_creator = canvas.texture_creator();
    let texture = sdl::create_texture(&texture_creator, display_size)?;
    let events = video.sdl().event_pump()?;
    let mut sdl = SdlWrapper::new(canvas, texture, events);

    /* Random */
    let random: Random = (
        |range| rand::thread_rng().gen_range(range),
        |slice| slice.shuffle(&mut rand::thread_rng()),
    );

    syno_photo_frame::run(
        &cli,
        (
            &LoggingClientDecorator::new(ReqwestClient::from(client)).with_level(log::Level::Trace),
            &(cookie_store as Arc<dyn CookieStore>),
        ),
        &mut sdl,
        thread::sleep,
        random,
    )?;

    Ok(())
}
