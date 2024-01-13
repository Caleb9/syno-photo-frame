use crate::http::{Client, Response};
use crate::ErrorToString;

pub(crate) fn get_latest_version(client: &impl Client) -> Result<dto::Crate, String> {
    let response = client.get("https://index.crates.io/sy/no/syno-photo-frame", &[])?;
    let status = response.status();
    if status.is_success() {
        // TODO: deal with yanked crates
        match response.text()?.lines().last() {
            Some(json) => serde_json::from_str::<dto::Crate>(json).map_err_to_string(),
            None => Err("Unable to read creates.io response".to_owned()),
        }
    } else {
        Err(status
            .canonical_reason()
            .unwrap_or(status.as_str())
            .to_owned())
    }
}

pub(crate) mod dto {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct Crate {
        pub vers: String,
        pub yanked: bool,
    }
}
