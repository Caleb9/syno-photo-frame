//! Logging

use core::fmt::Debug;

use anyhow::Result;
use log::Level;

use crate::http::{HttpClient, HttpResponse, Query};

/// Adds logging to [HttpClient]
#[derive(Clone, Debug)]
pub struct LoggingClientDecorator<C> {
    client: C,
    level: Level,
}

impl<C> LoggingClientDecorator<C> {
    pub const fn new(client: C) -> Self {
        LoggingClientDecorator {
            client,
            level: Level::Debug,
        }
    }

    pub const fn with_level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }
}

impl<C, R> HttpClient for LoggingClientDecorator<C>
where
    C: HttpClient<Response = R>,
    R: Debug + HttpResponse,
{
    type Response = R;

    fn post(
        &self,
        url: &str,
        form: &[(&str, &str)],
        query: Query,
        header: Option<(&str, &str)>,
    ) -> Result<Self::Response> {
        /* Obfuscate password from the form parameters */
        let obfuscated_form = form
            .iter()
            .map(|(k, v)| (*k, if *k == "password" { "[REDACTED]" } else { *v }))
            .collect::<Vec<(&str, &str)>>();
        log::log!(
            self.level,
            "POST {url}, form: {obfuscated_form:?}, header: {header:?}"
        );
        let response = self.client.post(url, form, query, header);
        log::log!(self.level, "{response:?}");
        response
    }

    fn post_json(
        &self,
        url: &str,
        query: &[(&str, &str)],
        json: &serde_json::Value,
    ) -> Result<Self::Response> {
        log::log!(self.level, "POST {url}, json: {json:?}");
        let response = self.client.post_json(url, query, json);
        log::log!(self.level, "{response:?}");
        response
    }

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Self::Response> {
        log::log!(self.level, "GET {url}, query: {query:?}");
        let response = self.client.get(url, query);
        log::log!(self.level, "{response:?}");
        response
    }
}
