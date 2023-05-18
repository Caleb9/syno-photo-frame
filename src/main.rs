use std::{error::Error, process, sync::Arc};

use clap::Parser;
use reqwest::{blocking::ClientBuilder, cookie::CookieStore};

use syno_photo_frame::{self, cli::Cli, http::ReqwestClient};

fn main() -> Result<(), Box<dyn Error>> {
    ctrlc::set_handler(|| process::exit(0))?;

    let cli = Cli::parse();
    let cookie_store = Arc::new(reqwest::cookie::Jar::default());
    let client = ClientBuilder::new()
        .cookie_provider(cookie_store.clone())
        .build()?;

    syno_photo_frame::run::<ReqwestClient>(
        &cli,
        (
            &ReqwestClient::from(client),
            &(cookie_store as Arc<dyn CookieStore>),
        ),
    )?;

    Ok(())
}
