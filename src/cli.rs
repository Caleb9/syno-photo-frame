pub use clap::Parser;

use crate::http::Url;

#[derive(Debug, Parser)]
pub struct Cli {
    /// Link to publicly shared album
    pub share_link: Url,

    /// How long should each photo be displayed
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
