use std::panic::Location;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};
use derive_more::Display;
use log::debug;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::located_error::LocatedError;
use crate::protocol::clock::{Current, DurationSinceUnixEpoch, Time, TimeNow};
use crate::protocol::common::AUTH_KEY_LENGTH;

#[must_use]
/// # Panics
///
/// It would panic if the `lifetime: Duration` + Duration is more than `Duration::MAX`.
pub fn generate(lifetime: Duration) -> ExpiringKey {
    let random_id: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(AUTH_KEY_LENGTH)
        .map(char::from)
        .collect();

    debug!("Generated key: {}, valid for: {:?} seconds", random_id, lifetime);

    ExpiringKey {
        key: random_id.parse::<Key>().unwrap(),
        valid_until: Current::add(&lifetime).unwrap(),
    }
}

/// # Errors
///
/// Will return `Error::KeyExpired` if `auth_key.valid_until` is past the `current_time`.
///
/// Will return `Error::KeyInvalid` if `auth_key.valid_until` is past the `None`.
pub fn verify(auth_key: &ExpiringKey) -> Result<(), Error> {
    let current_time: DurationSinceUnixEpoch = Current::now();

    if auth_key.valid_until < current_time {
        Err(Error::KeyExpired {
            location: Location::caller(),
        })
    } else {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ExpiringKey {
    pub key: Key,
    pub valid_until: DurationSinceUnixEpoch,
}

impl std::fmt::Display for ExpiringKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "key: `{}`, valid until `{}`",
            self.key,
            DateTime::<Utc>::from_utc(
                NaiveDateTime::from_timestamp(
                    i64::try_from(self.valid_until.as_secs()).expect("Overflow of i64 seconds, very future!"),
                    self.valid_until.subsec_nanos(),
                ),
                Utc
            )
        )
    }
}

impl ExpiringKey {
    #[must_use]
    pub fn id(&self) -> Key {
        self.key.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Display, Hash)]
pub struct Key(String);

#[derive(Debug, PartialEq, Eq)]
pub struct ParseKeyError;

impl FromStr for Key {
    type Err = ParseKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != AUTH_KEY_LENGTH {
            return Err(ParseKeyError);
        }

        Ok(Self(s.to_string()))
    }
}

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum Error {
    #[error("Key could not be verified: {source}")]
    KeyVerificationError {
        source: LocatedError<'static, dyn std::error::Error + Send + Sync>,
    },
    #[error("Failed to read key: {key}, {location}")]
    UnableToReadKey {
        location: &'static Location<'static>,
        key: Box<Key>,
    },
    #[error("Key has expired, {location}")]
    KeyExpired { location: &'static Location<'static> },
}

impl From<r2d2_sqlite::rusqlite::Error> for Error {
    fn from(e: r2d2_sqlite::rusqlite::Error) -> Self {
        Error::KeyVerificationError {
            source: (Arc::new(e) as Arc<dyn std::error::Error + Send + Sync>).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::time::Duration;

    use crate::protocol::clock::{Current, StoppedTime};
    use crate::tracker::auth;

    #[test]
    fn auth_key_from_string() {
        let key_string = "YZSl4lMZupRuOpSRC3krIKR5BPB14nrJ";
        let auth_key = auth::Key::from_str(key_string);

        assert!(auth_key.is_ok());
        assert_eq!(auth_key.unwrap().to_string(), key_string);
    }

    #[test]
    fn generate_valid_auth_key() {
        let auth_key = auth::generate(Duration::new(9999, 0));

        assert!(auth::verify(&auth_key).is_ok());
    }

    #[test]
    fn generate_and_check_expired_auth_key() {
        // Set the time to the current time.
        Current::local_set_to_system_time_now();

        // Make key that is valid for 19 seconds.
        let auth_key = auth::generate(Duration::from_secs(19));

        // Mock the time has passed 10 sec.
        Current::local_add(&Duration::from_secs(10)).unwrap();

        assert!(auth::verify(&auth_key).is_ok());

        // Mock the time has passed another 10 sec.
        Current::local_add(&Duration::from_secs(10)).unwrap();

        assert!(auth::verify(&auth_key).is_err());
    }
}