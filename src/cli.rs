pub use clap::Parser;

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

    /// Start slideshow at randomly selected photo
    #[arg(long)]
    pub random_start: bool,
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}
