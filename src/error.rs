//! Maps Result<T, String> often returned from SDL implementation to anyhow::Result<T>

use anyhow::anyhow;

pub trait AnyhowErrorMapper<T> {
    fn map_err_to_anyhow(self) -> anyhow::Result<T>;
}

impl<T> AnyhowErrorMapper<T> for anyhow::Result<T, String> {
    fn map_err_to_anyhow(self) -> anyhow::Result<T> {
        self.map_err(|e| anyhow!(e))
    }
}
