use std::{error::Error, process, sync::Arc, thread, time::Duration};

use log::LevelFilter;
use rand::{self, seq::SliceRandom, Rng};
use simple_logger::SimpleLogger;

use syno_photo_frame::{
    self,
    cli::{Cli, Parser},
    error::{ErrorToString, SynoPhotoFrameError},
    http::{ClientBuilder, ReqwestClient},
    logging::LoggingClientDecorator,
    sdl::{self, SdlWrapper},
    Random,
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
            match error {
                SynoPhotoFrameError::Login(_) => Err(
                    "Login to Synology Photos failed. Make sure the share link is pointing to a \
                        *publicly shared album*. If the album's password link protection is \
                        enabled, use the --password option with a valid password.",
                )?,
                other => Err(other)?,
            }
        }
        _ => Ok(()),
    }
}

fn init_and_run() -> Result<(), SynoPhotoFrameError> {
    let cli = Cli::parse();

    /* HTTP client */
    let cookie_store = Arc::new(reqwest::cookie::Jar::default());
    let client = ClientBuilder::new()
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
    let ttf = sdl::init_ttf()?;
    let update_notification_texture =
        sdl::create_update_notification_texture(&ttf, &texture_creator)?;
    let mut sdl = SdlWrapper::new(canvas, textures, update_notification_texture, events);

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
            &LoggingClientDecorator::new(ReqwestClient::from(client)).with_level(log::Level::Trace),
            cookie_store.as_ref(),
        ),
        &mut sdl,
        (thread::sleep, random),
        installed_version,
    )?;

    Ok(())
}
