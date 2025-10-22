//! Isolates environment variables

use std::env::var;

use anyhow::Result;

#[cfg_attr(test, mockall::automock)]
pub trait Env {
    fn var(&self, key: &str) -> Result<String>;
}

pub struct EnvImpl;

impl Env for EnvImpl {
    fn var(&self, key: &str) -> Result<String> {
        Ok(var(key)?)
    }
}
