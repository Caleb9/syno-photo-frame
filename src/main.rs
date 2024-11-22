use std::{error::Error, sync::Arc, time::Duration};

use log::LevelFilter;
use rand::{self, seq::SliceRandom, Rng};
use simple_logger::SimpleLogger;

use syno_photo_frame::{
    self,
    cli::{Cli, Parser},
    error::{ErrorToString, FrameError},
    http::ClientBuilder,
    logging::LoggingClientDecorator,
    sdl::{self, SdlWrapper},
    FrameResult, Random,
};

fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info) /* Default */
        .env() /* Allow overwriting default level with RUST_LOG env var */
        .init()?;

    match init_and_run() {
        Err(FrameError::Login(error)) => {
            log::error!("{error}");
            Err(
                "Login to Synology Photos failed. Make sure the share link is pointing to a \
                *publicly shared album*. If the album's password link protection is \
                enabled, use the --password option with a valid password.",
            )?
        }
        Err(FrameError::Other(error)) => {
            log::error!("{error}");
            Err(error)?
        }
        Ok(()) | Err(FrameError::Quit(_)) => Ok(()),
    }
}

/// Setup "real" dependencies and run
fn init_and_run() -> FrameResult<()> {
    let cli = Cli::parse();

    /* HTTP client */
    let cookie_store = Arc::new(reqwest::cookie::Jar::default());
    let http_client = ClientBuilder::new()
        .cookie_provider(Arc::clone(&cookie_store))
        .timeout(Duration::from_secs(cli.timeout_seconds as u64))
        .build()
        .map_err_to_string()?;

    /* SDL */
    let video = sdl::init_video()?;
    let display_size = sdl::display_size(&video)?;
    let canvas = sdl::create_canvas(&video, display_size)?;
    let texture_creator = canvas.texture_creator();
    let textures = [
        sdl::create_texture(&texture_creator, display_size)?,
        sdl::create_texture(&texture_creator, display_size)?,
    ];
    let events = video.sdl().event_pump()?;
    let mut sdl = SdlWrapper::new(canvas, textures, events);

    /* Random */
    let random: Random = (
        |range| rand::thread_rng().gen_range(range),
        |slice| slice.shuffle(&mut rand::thread_rng()),
    );

    /* This crate version */
    let installed_version = env!("CARGO_PKG_VERSION");

    syno_photo_frame::run(
        &cli,
        (
            &LoggingClientDecorator::new(http_client).with_level(log::Level::Trace),
            cookie_store.as_ref(),
        ),
        &mut sdl,
        random,
        installed_version,
    )
}
