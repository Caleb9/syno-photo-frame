//! CLI options

pub use clap::Parser;

use std::{path::PathBuf, time::Duration};

use anyhow::{bail, Result};
use clap::{builder::TypedValueParser as _, ValueEnum};

use crate::http::Url;

/// Synology Photos album fullscreen slideshow
///
/// Project website: <https://github.com/caleb9/syno-photo-frame>
#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Cli {
    /// Link to a publicly shared album on Synology Photos
    ///
    /// Note that the album's privacy settings must be set to Public
    pub share_link: Url,

    /// Link protection password if set in the album sharing settings
    #[arg(short = 'p', long = "password")]
    pub password: Option<String>,

    /// Photo change interval in seconds
    ///
    /// Must be greater or equal to 5. Note that it is only guaranteed that the display time will
    /// not be shorter than specified value, but it may exceed this value if next photo fetching and
    /// processing takes longer time
    #[arg(
        short = 'i',
        long = "interval",
        default_value = "30",
        value_parser = try_parse_duration)]
    pub photo_change_interval: Duration,

    /// Slideshow ordering
    #[arg(short = 'o', long, value_enum, default_value_t = Order::ByDate)]
    pub order: Order,

    /// Start at randomly selected photo, then continue according to --order
    #[arg(long, default_value_t = false)]
    pub random_start: bool,

    /// Transition effect
    #[arg(short = 't', long, value_enum, default_value_t = Transition::Crossfade)]
    pub transition: Transition,

    /// Background fill effect
    #[arg(long, value_enum, default_value_t = Background::Blur)]
    pub background: Background,

    /// Rotate display to match screen orientation
    #[arg(
        long = "rotate",
        default_value = "0",
        value_parser =
            clap::builder::PossibleValuesParser::new(ROTATIONS).map(Rotation::from)
    )]
    pub rotation: Rotation,

    /// Path to a JPEG file to display during startup, replacing the default splash-screen
    #[arg(long)]
    pub splash: Option<PathBuf>,

    /// HTTP request timeout in seconds
    ///
    /// Must be greater or equal to 5. When Synology Photos does not respond within the timeout, an
    /// error is displayed. Try to increase the value for slow connections
    #[arg(
        long = "timeout",
        default_value_t = 30,
        value_parser = clap::value_parser!(u16).range(5..))]
    pub timeout_seconds: u16,

    /// Requested size of the photo as fetched from the Synology Photos. Can reduce network and CPU
    /// utilization at the cost of image quality. Note that photos are still scaled to full-screen
    /// size
    #[arg(long, value_enum, default_value_t = SourceSize::L)]
    pub source_size: SourceSize,

    /// Disable checking for updates during startup
    #[arg(long, default_value_t = false)]
    pub disable_update_check: bool,
}

fn try_parse_duration(arg: &str) -> Result<Duration> {
    let seconds = arg.parse()?;
    if seconds < 5 {
        bail!("must not be less than 5")
    } else {
        Ok(Duration::from_secs(seconds))
    }
}

/// Slideshow ordering
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Order {
    /// by photo shooting date
    ByDate,
    /// by photo file name
    ByName,
    /// randomly
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

/// Background fill effect
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Background {
    /// Blur the photo
    Blur,
    /// Disable background (black)
    None,
}
const ROTATIONS: [&str; 4] = ["0", "90", "180", "270"];

/// Screen rotation in degrees
#[derive(Debug, Copy, Clone)]
pub enum Rotation {
    /// 0°
    D0,
    /// 90°
    D90,
    /// 180°
    D180,
    /// 270°
    D270,
}

impl From<String> for Rotation {
    fn from(value: String) -> Self {
        match value.as_str() {
            "0" => Rotation::D0,
            "90" => Rotation::D90,
            "180" => Rotation::D180,
            "270" => Rotation::D270,
            _ => panic!(),
        }
    }
}

/// Requested size of source photo to fetch from Synology Photos
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
