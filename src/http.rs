//! HTTP request-response handling

use std::fmt::Formatter;

pub(crate) use bytes::Bytes;
pub use reqwest::{blocking::ClientBuilder, cookie::CookieStore};
pub(crate) use reqwest::{StatusCode, Url};

use anyhow::{bail, Result};
use reqwest::blocking::{Client as ReqwestClient, Response as ReqwestResponse};
use serde::de::DeserializeOwned;

#[cfg(test)]
pub(crate) use reqwest::cookie::Jar;

/// Isolates [reqwest::blocking::Client] for testing
pub trait HttpClient {
    type Response: HttpResponse;

    fn post(
        &self,
        url: &str,
        form: &[(&str, &str)],
        header: Option<(&str, &str)>,
    ) -> Result<Self::Response>;

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Self::Response>;
}

/// Isolates [reqwest::blocking::Response] for testing
#[cfg_attr(test, mockall::automock)]
pub trait HttpResponse {
    fn status(&self) -> StatusCode;

    /* 'static is needed by automock */
    fn json<T: DeserializeOwned + 'static>(self) -> Result<T>;

    fn bytes(self) -> Result<Bytes>;

    fn text(self) -> Result<String>;
}

impl HttpClient for ReqwestClient {
    type Response = ReqwestResponse;

    fn post(
        &self,
        url: &str,
        form: &[(&str, &str)],
        header: Option<(&str, &str)>,
    ) -> Result<ReqwestResponse> {
        let mut request_builder = ReqwestClient::post(self, url).form(form);
        if let Some((key, value)) = header {
            request_builder = request_builder.header(key, value);
        }
        Ok(request_builder.send()?)
    }

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<ReqwestResponse> {
        Ok(ReqwestClient::get(self, url).query(query).send()?)
    }
}

impl HttpResponse for ReqwestResponse {
    fn status(&self) -> StatusCode {
        ReqwestResponse::status(self)
    }

    fn json<T: DeserializeOwned>(self) -> Result<T> {
        Ok(ReqwestResponse::json(self)?)
    }

    fn bytes(self) -> Result<Bytes> {
        Ok(ReqwestResponse::bytes(self)?)
    }

    fn text(self) -> Result<String> {
        Ok(ReqwestResponse::text(self)?)
    }
}

pub fn read_response<R, S, T>(response: R, on_success: S) -> Result<T>
where
    R: HttpResponse,
    S: FnOnce(R) -> Result<T>,
{
    let status = response.status();
    if status.is_success() {
        on_success(response)
    } else {
        bail!(InvalidHttpResponse(status))
    }
}

#[derive(Debug)]
pub struct InvalidHttpResponse(pub StatusCode);

impl std::fmt::Display for InvalidHttpResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid HTTP response code: {}", self.0)
    }
}
