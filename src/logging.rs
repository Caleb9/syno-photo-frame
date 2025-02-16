//! Logging

use core::fmt::Debug;

use log::Level;

/// Adds logging to [Client]
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
