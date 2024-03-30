use std::error::Error;

use log::LevelFilter;
use rand::{self, seq::SliceRandom, Rng};
use simple_logger::SimpleLogger;

use syno_photo_frame::{
    self,
    cli::{Cli, Parser},
    error::FrameError,
    sdl::{self, SdlWrapper},
    FrameResult, Random,
};

fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new()
        .with_level(LevelFilter::Debug)
        .env()
        .init()?;

    match init_and_run() {
        Err(FrameError::Other(error)) => {
            log::error!("{error}");
            Err(error)?
        }
        Ok(()) | Err(FrameError::Quit(_)) => Ok(()),
    }
}

fn init_and_run() -> FrameResult<()> {
    let cli = Cli::parse();

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

    syno_photo_frame::run(
        &cli,
        &mut sdl,
        random,
    )
}
