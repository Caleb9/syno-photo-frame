//! CLI options

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
    #[arg(short = 'o', long, value_enum, default_value_t = Order::ByDate)]
    pub order: Order,

    /// Transition effect
    #[arg(short = 't', long, value_enum, default_value_t = Transition::Crossfade)]
    pub transition: Transition,

    /// Path to a JPEG file to display during startup, replacing the default splash-screen
    #[arg(long)]
    pub splash: Option<PathBuf>,

    /// HTTP request timeout in seconds
    ///
    /// Must be greater or equal to 5. When Synology Photos does not respond within the timeout, an error is
    /// displayed
    #[arg(long = "timeout", default_value_t = 30, value_parser = clap::value_parser!(u16).range(5..))]
    pub timeout_seconds: u16,

    /// Size of the photo as fetched from the Synology Photos. Can reduce network and CPU utilization at the
    /// cost of image quality
    #[arg(long, value_enum, default_value_t = SourceSize::L)]
    pub source_size: SourceSize,
}

/// Slideshow ordering
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Order {
    /// ordered by photo shooting date
    ByDate,
    /// ordered by photo shooting date but starting at randomly selected photo
    RandomStart,
    /// in random order
    Random,
}

/// Transition to next photo effect
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Transition {
    /// Crossfade (or cross dissolve)
    Crossfade,
    /// Fade out to black and in to next photo
    FadeToBlack,
    /// Disable transition effect
    None,
}

/// Size of source photo to fetch
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum SourceSize {
    /// small (360x240)
    S,
    /// medium (481x320)
    M,
    /// large (1922x1280)
    L,
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}
