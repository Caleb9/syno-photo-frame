use core::fmt::Debug;

use log::Level;

use crate::http::{Client, Response};

/// Adds logging to [crate::http::Client]
#[derive(Clone, Debug)]
pub struct LoggingClientDecorator<C> {
    client: C,
    level: Level,
}

impl<C> LoggingClientDecorator<C> {
    pub fn new(client: C) -> Self {
        LoggingClientDecorator {
            client,
            level: Level::Debug,
        }
    }

    pub fn with_level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }
}

impl<C, R> Client for LoggingClientDecorator<C>
where
    C: Client<Response = R>,
    R: Debug + Response,
{
    type Response = R;

    fn post(
        &self,
        url: &str,
        form: &[(&str, &str)],
        header: Option<(&str, &str)>,
    ) -> Result<Self::Response, String> {
        log::log!(self.level, "POST {url}, form: {form:?}, header: {header:?}");
        let response = self.client.post(url, form, header);
        log::log!(self.level, "{response:?}");
        response
    }

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Self::Response, String> {
        log::log!(self.level, "GET {url}, query: {query:?}");
        let response = self.client.get(url, query);
        log::log!(self.level, "{response:?}");
        response
    }
}
