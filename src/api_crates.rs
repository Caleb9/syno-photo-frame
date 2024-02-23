use crate::http::{Client, Response};

pub fn get_latest_version(client: &impl Client) -> Result<dto::Crate, String> {
    let response = client.get("https://index.crates.io/sy/no/syno-photo-frame", &[])?;
    let status = response.status();
    if status.is_success() {
        response
            .text()?
            .lines()
            .map(serde_json::from_str::<dto::Crate>)
            .filter_map(|r| r.ok())
            .rfind(|c| !c.yanked)
            .ok_or("Unable to read creates.io response".to_string())
    } else {
        Err(status
            .canonical_reason()
            .unwrap_or(status.as_str())
            .to_string())
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
        http::{MockResponse, StatusCode},
        test_helpers::MockClient,
    };

    #[test]
    fn get_latest_version_returns_last_not_yanked_crate() {
        const TEXT: &str = r#"{"vers": "0.1.0", "yanked": false}
            {"vers": "0.2.0", "yanked": false}
            {"vers": "0.3.0", "yanked": true}
            {"vers": "0.4.0", "yanked": false}
            {"vers": "0.5.0", "yanked": false}
            {"vers": "0.6.0", "yanked": true}"#;
        let mut response_mock = MockResponse::new();
        response_mock.expect_status().return_const(StatusCode::OK);
        response_mock
            .expect_text()
            .return_const(Ok(TEXT.to_string()));
        let mut client_mock = MockClient::new();
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
