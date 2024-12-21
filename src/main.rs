use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Result};
use log::LevelFilter;
use simple_logger::SimpleLogger;

use syno_photo_frame::{
    self,
    cli::{Cli, Parser},
    http::ClientBuilder,
    logging::LoggingClientDecorator,
    sdl::{self, SdlWrapper},
    LoginError, QuitEvent, RandomImpl,
};

fn main() -> Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info) /* Default */
        .env() /* Allow overwriting default level with RUST_LOG env var */
        .init()?;

    match init_and_run() {
        Err(error) => {
            if error.is::<QuitEvent>() {
                return Ok(());
            }
            log::error!("{error}");
            match error.downcast_ref::<LoginError>() {
                Some(LoginError(_)) => {
                    bail!(
                        "Login failed. Make sure the share link is pointing to a \
                        *publicly shared album*. If the album's password link protection is \
                        enabled, use the --password option with a valid password.",
                    )
                }
                _ => bail!(error),
            }
        }
        _ => Ok(()),
    }
}

/// Setup "real" dependencies and run
fn init_and_run() -> Result<()> {
    let cli = Cli::parse();

    /* HTTP client */
    let cookie_store = Arc::new(reqwest::cookie::Jar::default());
    let http_client = ClientBuilder::new()
        .cookie_provider(Arc::clone(&cookie_store))
        .timeout(Duration::from_secs(cli.timeout_seconds as u64))
        .build()?;

    /* SDL */
    let video = sdl::init_video()?;
    let display_size = sdl::display_size(&video)?;
    let canvas = sdl::create_canvas(&video, display_size)?;
    let texture_creator = canvas.texture_creator();
    let textures = [
        sdl::create_texture(&texture_creator, display_size)?,
        sdl::create_texture(&texture_creator, display_size)?,
    ];
    let events = video.sdl().event_pump().map_err(|s| anyhow!(s))?;
    let mut sdl = SdlWrapper::new(canvas, textures, events);

    /* This crate version */
    let installed_version = env!("CARGO_PKG_VERSION");

    syno_photo_frame::run(
        &cli,
        (
            &LoggingClientDecorator::new(http_client).with_level(log::Level::Trace),
            cookie_store.as_ref(),
        ),
        &mut sdl,
        RandomImpl,
        installed_version,
    )
}
