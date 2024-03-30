//! HTTP request-response handling

pub(crate) use bytes::Bytes;
pub use reqwest::{blocking::ClientBuilder, cookie::CookieStore};
pub(crate) use reqwest::{StatusCode, Url};

use reqwest::blocking::{Client as ReqwestClient, Response as ReqwestResponse};
use serde::de::DeserializeOwned;

use crate::error::ErrorToString;

/// Isolates [reqwest::blocking::Client] for testing
pub trait Client {
    type Response: Response;

    fn post(
        &self,
        url: &str,
        form: &[(&str, &str)],
        header: Option<(&str, &str)>,
    ) -> Result<Self::Response, String>;

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Self::Response, String>;
}

/// Isolates [reqwest::blocking::Response] for testing
#[cfg_attr(test, mockall::automock)]
pub trait Response {
    fn status(&self) -> StatusCode;

    /* 'static is needed by automock */
    fn json<T: DeserializeOwned + 'static>(self) -> Result<T, String>;

    fn bytes(self) -> Result<Bytes, String>;

    fn text(self) -> Result<String, String>;
}

impl Client for ReqwestClient {
    type Response = ReqwestResponse;

    fn post(
        &self,
        url: &str,
        form: &[(&str, &str)],
        header: Option<(&str, &str)>,
    ) -> Result<ReqwestResponse, String> {
        let mut request_builder = ReqwestClient::builder().danger_accept_invalid_certs(true).build().unwrap().post(url).form(form);
        if let Some((key, value)) = header {
            request_builder = request_builder.header(key, value);
        }
        request_builder.send().map_err_to_string()
    }

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<ReqwestResponse, String> {
        ReqwestClient::builder().danger_accept_invalid_certs(true).build().unwrap().get(url)
        .query(query)
        .send()
        .map_err_to_string()
        // ReqwestClient::get(self, url)
        //     .query(query)
        //     .send()
        //     .map_err_to_string()
    }
}

impl Response for ReqwestResponse {
    fn status(&self) -> StatusCode {
        ReqwestResponse::status(self)
    }

    fn json<T: DeserializeOwned>(self) -> Result<T, String> {
        ReqwestResponse::json(self).map_err_to_string()
    }

    fn bytes(self) -> Result<Bytes, String> {
        ReqwestResponse::bytes(self).map_err_to_string()
    }

    fn text(self) -> Result<String, String> {
        ReqwestResponse::text(self).map_err_to_string()
    }
}
