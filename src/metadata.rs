use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    str::FromStr,
};

use chrono::{DateTime, Locale, Utc};

use crate::env::Env;

/// Adds functions to fetch metadata from the DTOs representing photos
pub trait Metadata {
    fn date(&self) -> DateTime<Utc>;
    fn location(&self) -> Location;
    fn try_format_as_localized_string(&self, locale: Locale) -> String {
        let (date, location) = (self.date(), self.location());
        let mut output = String::new();
        output.push_str(date.format_localized("%x", locale).to_string().as_str());
        if let (None, None) = (&location.area, &location.country) {
            return output;
        }
        output.push_str(&format!(" {location}"));
        output
    }
}

#[derive(Debug, Default)]
pub struct Location {
    pub area: Option<String>,
    pub country: Option<String>,
}

impl Location {
    pub fn new(area: Option<String>, country: Option<String>) -> Self {
        Self { area, country }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match (&self.area, &self.country) {
            (Some(area), Some(country)) => write!(f, "{}, {}", area, country),
            (Some(area), None) => write!(f, "{}", area),
            (None, Some(country)) => write!(f, "{}", country),
            _ => Ok(()),
        }
    }
}

pub trait FromEnv<T> {
    fn from_env(env: &impl Env) -> T;
}

impl FromEnv<Locale> for Locale {
    fn from_env(env: &impl Env) -> Locale {
        /* Read variables in decreasing priority order. See https://wiki.debian.org/Locale. */
        let var = env.var("LC_ALL").or(env.var("LC_TIME")).or(env.var("LANG"));
        if let Ok(var) = var {
            let locale_id = var.split_once('.').map_or(var.as_str(), |(left, _)| left);
            Locale::from_str(locale_id)
                .inspect_err(|_| {
                    log::warn!("Unknown locale '{var}'. Falling back to POSIX date format.")
                })
                .map_or(Locale::POSIX, |locale| locale)
        } else {
            log::warn!(
                "Date format not set in locale (LC_ALL, LC_TIME, and LANG are all empty). \
                 Falling back to POSIX date format."
            );
            Locale::POSIX
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;
    use syno_api::foto::browse::item::dto::{Additional, Address};

    #[test]
    fn locale_from_env_lc_all_has_highest_priority() {
        let mut env_stub = MockEnv::default();
        env_stub
            .expect_var()
            .withf(|key| key == "LC_ALL")
            .return_once(|_| Ok("pl_PL.UTF-8".to_string()));
        env_stub
            .expect_var()
            .withf(|key| key == "LC_TIME")
            .return_once(|_| Ok("en_GB.UTF-8".to_string()));
        env_stub
            .expect_var()
            .withf(|key| key == "LANG")
            .return_once(|_| Ok("da_DK.UTF-8".to_string()));

        let result = Locale::from_env(&env_stub);

        assert_eq!(result, Locale::pl_PL);
    }

    #[test]
    fn locale_from_env_lc_time_has_second_priority() {
        let mut env_stub = MockEnv::default();
        env_stub
            .expect_var()
            .withf(|key| key == "LC_ALL")
            .return_once(|_| Err(anyhow::anyhow!("")));
        env_stub
            .expect_var()
            .withf(|key| key == "LC_TIME")
            .return_once(|_| Ok("en_GB.UTF-8".to_string()));
        env_stub
            .expect_var()
            .withf(|key| key == "LANG")
            .return_once(|_| Ok("da_DK.UTF-8".to_string()));

        let result = Locale::from_env(&env_stub);

        assert_eq!(result, Locale::en_GB);
    }

    #[test]
    fn locale_from_env_lang_has_lowest_priority() {
        let mut env_stub = MockEnv::default();
        env_stub
            .expect_var()
            .withf(|key| key == "LC_ALL")
            .return_once(|_| Err(anyhow::anyhow!("")));
        env_stub
            .expect_var()
            .withf(|key| key == "LC_TIME")
            .return_once(|_| Err(anyhow::anyhow!("")));
        env_stub
            .expect_var()
            .withf(|key| key == "LANG")
            .return_once(|_| Ok("da_DK.UTF-8".to_string()));

        let result = Locale::from_env(&env_stub);

        assert_eq!(result, Locale::da_DK);
    }

    #[test]
    fn locale_from_env_when_locale_unset_then_falls_back_to_posix() {
        let mut env_stub = MockEnv::default();
        env_stub
            .expect_var()
            .withf(|key| key == "LC_ALL")
            .return_once(|_| Err(anyhow::anyhow!("")));
        env_stub
            .expect_var()
            .withf(|key| key == "LC_TIME")
            .return_once(|_| Err(anyhow::anyhow!("")));
        env_stub
            .expect_var()
            .withf(|key| key == "LANG")
            .return_once(|_| Err(anyhow::anyhow!("")));

        let result = Locale::from_env(&env_stub);

        assert_eq!(result, Locale::POSIX);
    }

    #[test]
    fn locale_from_env_when_invalid_locale_then_falls_back_to_posix() {
        let mut env_stub = MockEnv::default();
        env_stub
            .expect_var()
            .withf(|key| key == "LC_ALL")
            .return_once(|_| Ok("not-a-locale".to_string()));
        env_stub
            .expect_var()
            .withf(|key| key == "LC_TIME")
            .return_once(|_| Ok("not-a-locale".to_string()));
        env_stub
            .expect_var()
            .withf(|key| key == "LANG")
            .return_once(|_| Ok("not-a-locale".to_string()));

        let result = Locale::from_env(&env_stub);

        assert_eq!(result, Locale::POSIX);
    }

    #[test]
    fn metadata_try_format_as_localized_string_formats_date_according_to_locale() {
        let photo = syno_api::foto::browse::item::dto::Item {
            time: 1760788393,
            ..Default::default()
        };

        let result_pl = photo.try_format_as_localized_string(Locale::pl_PL);
        let result_gb = photo.try_format_as_localized_string(Locale::en_GB);

        assert_eq!(result_pl, "18.10.2025");
        assert_eq!(result_gb, "18/10/25");
    }

    #[test]
    fn metadata_try_format_as_localized_string_contains_country() {
        let photo = syno_api::foto::browse::item::dto::Item {
            additional: Some(Additional {
                address: Some(Address {
                    country: "Denmark".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = photo.try_format_as_localized_string(Locale::en_GB);

        assert_eq!(result, "01/01/70 Denmark");
    }

    #[test]
    fn metadata_try_format_as_localized_string_contains_area() {
        let photo = syno_api::foto::browse::item::dto::Item {
            additional: Some(Additional {
                address: Some(Address {
                    city: "Copenhagen".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = photo.try_format_as_localized_string(Locale::en_GB);

        assert_eq!(result, "01/01/70 Copenhagen");
    }

    #[test]
    fn metadata_try_format_as_localized_string_contains_area_and_country() {
        let photo = syno_api::foto::browse::item::dto::Item {
            additional: Some(Additional {
                address: Some(Address {
                    city: "Copenhagen".to_string(),
                    country: "Denmark".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = photo.try_format_as_localized_string(Locale::en_GB);

        assert_eq!(result, "01/01/70 Copenhagen, Denmark");
    }
}
