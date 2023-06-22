use std::path::PathBuf;

pub use clap::Parser;
use clap::ValueEnum;

use crate::http::Url;

/// Synology Photos album fullscreen slideshow
#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Cli {
    /// Link to a publicly shared album on Synology Photos
    ///
    /// Note that the album's privacy settings must be set to Public and link password protection must be disabled
    pub share_link: Url,

    /// Photo change interval in seconds
    ///
    /// Must be greater or equal to 5. Note that it is only guaranteed that the display time will not be shorter than
    /// specified value, but it may exceed this value if next photo fetching and processing takes longer time
    #[arg(short = 'i', long = "interval", default_value_t = 30, value_parser = clap::value_parser!(u16).range(5..))]
    pub interval_seconds: u16,

    /// Slideshow ordering
    #[arg(long, value_enum, default_value_t = Order::ByDate)]
    pub order: Order,

    /// Path to a JPEG file to display during startup, replacing the default splash-screen
    #[arg(long)]
    pub splash: Option<PathBuf>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Order {
    /// ordered by photo shooting date
    ByDate,
    /// ordered by photo shooting date but starting at randomly selected photo
    RandomStart,
    /// in random order
    Random,
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}
