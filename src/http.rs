pub use bytes::Bytes;
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

/// Isolates reqwest's Client for testing
#[derive(Clone)]
pub struct ReqwestClient {
    client: reqwest::blocking::Client,
}

impl ReqwestClient {
    pub fn new(client: reqwest::blocking::Client) -> Self {
        Self { client }
    }
}

pub trait Client<R>: Clone + Send + 'static {
    fn post<'a>(
        &self,
        url: &str,
        form: &[(&'a str, &'a str)],
        header: Option<(&str, &str)>,
    ) -> Result<R, String>;

    fn get<'a>(&self, url: &str, query: &[(&'a str, &'a str)]) -> Result<R, String>;
}

impl Client<ReqwestResponse> for ReqwestClient {
    fn post(
        &self,
        url: &str,
        form: &[(&str, &str)],
        header: Option<(&str, &str)>,
    ) -> Result<ReqwestResponse, String> {
        let mut request_builder = self.client.post(url).form(form);
        if let Some((key, value)) = header {
            request_builder = request_builder.header(key, value);
        }
        let response = request_builder.send().map_err_to_string()?;
        Ok(ReqwestResponse { response })
    }

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<ReqwestResponse, String> {
        let response = self
            .client
            .get(url)
            .query(query)
            .send()
            .map_err_to_string()?;
        Ok(ReqwestResponse { response })
    }
}
