use std::{error::Error, sync::Arc};

use clap::Parser;
use reqwest::blocking::ClientBuilder;

use syno_photo_frame::{
    self,
    cli::Cli,
    http::{self, ReqwestResponse},
};

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let cookie_store = Arc::new(reqwest::cookie::Jar::default());
    let client = ClientBuilder::new()
        .cookie_provider(cookie_store.clone())
        .build()?;

    syno_photo_frame::run::<_, _, ReqwestResponse>(
        &cli,
        (
            cookie_store,
            &|url, params, header| http::post(&client, url, params, header),
            &|url, query| http::get(&client, url, query),
        ),
    )?;

    Ok(())
}
