pub use bytes::Bytes;
use reqwest::blocking::Client;
pub use reqwest::StatusCode;
use serde::de::DeserializeOwned;

use crate::ErrorToString;

/// Isolates reqwest's Response for testing
#[cfg_attr(test, mockall::automock)]
pub trait Response {
    fn status(&self) -> StatusCode;

    fn json<T>(self) -> Result<T, String>
    where
        T: DeserializeOwned + 'static; /* 'static is needed by automock */

    fn bytes(self) -> Result<Bytes, String>;
}

pub fn post(
    client: &Client,
    url: &str,
    params: &[(&str, &str)],
    header: Option<(&str, &str)>,
) -> Result<ReqwestResponse, String> {
    let mut request_builder = client.post(url).form(&params);
    if let Some((key, value)) = header {
        request_builder = request_builder.header(key, value);
    }
    let response = request_builder.send().map_err_to_string()?;
    Ok(ReqwestResponse { response })
}

pub fn get(client: &Client, url: &str, query: &[(&str, &str)]) -> Result<ReqwestResponse, String> {
    let response = client.get(url).query(&query).send().map_err_to_string()?;
    Ok(ReqwestResponse { response })
}

pub struct ReqwestResponse {
    response: reqwest::blocking::Response,
}

impl Response for ReqwestResponse {
    fn status(&self) -> StatusCode {
        self.response.status()
    }

    fn json<T>(self) -> Result<T, String>
    where
        T: DeserializeOwned,
    {
        self.response.json().map_err_to_string()
    }

    fn bytes(self) -> Result<Bytes, String> {
        self.response.bytes().map_err_to_string()
    }
}
