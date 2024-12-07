use anyhow::{anyhow, bail, Result};

use crate::http::{HttpClient, HttpResponse};

pub fn get_latest_version(client: &impl HttpClient) -> Result<dto::Crate> {
    let response = client.get("https://index.crates.io/sy/no/syno-photo-frame", &[])?;
    let status = response.status();
    if status.is_success() {
        response
            .text()?
            .lines()
            .map(serde_json::from_str::<dto::Crate>)
            .filter_map(|r| r.ok())
            .rfind(|c| !c.yanked)
            .ok_or(anyhow!("Unable to read creates.io response".to_string()))
    } else {
        bail!("{:?}", status.canonical_reason().unwrap_or(status.as_str()))
    }
}

pub mod dto {
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize)]
    pub struct Crate {
        pub vers: String,
        pub yanked: bool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        http::{MockHttpResponse, StatusCode},
        test_helpers::MockHttpClient,
    };

    #[test]
    fn get_latest_version_returns_last_not_yanked_crate() {
        const TEXT: &str = r#"{"vers": "0.1.0", "yanked": false}
            {"vers": "0.2.0", "yanked": false}
            {"vers": "0.3.0", "yanked": true}
            {"vers": "0.4.0", "yanked": false}
            {"vers": "0.5.0", "yanked": false}
            {"vers": "0.6.0", "yanked": true}"#;
        let mut response_mock = MockHttpResponse::new();
        response_mock.expect_status().return_const(StatusCode::OK);
        response_mock
            .expect_text()
            .return_once(|| Ok(TEXT.to_string()));
        let mut client_mock = MockHttpClient::new();
        client_mock
            .expect_get()
            .return_once(|_, _| Ok(response_mock));

        let result = get_latest_version(&client_mock);

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            dto::Crate {
                vers: "0.5.0".to_string(),
                yanked: false
            }
        );
    }
}
