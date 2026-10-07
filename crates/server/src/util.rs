use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{NaiveDate, SecondsFormat, Utc};
use rand::RngCore;
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};

use crate::error::{AppError, bad};

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn new_id() -> String {
    ulid::Ulid::new().to_string()
}

/// Use a client-supplied ULID (lets clients create records optimistically/offline),
/// or generate one.
pub fn id_or_new(id: Option<String>) -> Result<String, AppError> {
    match id {
        None => Ok(new_id()),
        Some(s) => ulid::Ulid::from_string(&s)
            .map(|u| u.to_string())
            .map_err(|_| bad("id must be a ULID")),
    }
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn sha256_hex(s: &str) -> String {
    let d = Sha256::digest(s.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn parse_date(s: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| bad("date must be YYYY-MM-DD"))
}

pub fn check_hhmm(s: &str) -> Result<(), AppError> {
    streamline_domain::time::parse_hhmm(s)
        .map(|_| ())
        .ok_or_else(|| bad("time must be HH:MM"))
}

/// Validate a fractional-index ordering key.
pub fn check_position(s: &str) -> Result<(), AppError> {
    let ok = !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|c| c.is_ascii_alphanumeric())
        && !s.ends_with('0');
    if ok {
        Ok(())
    } else {
        Err(bad("invalid position key"))
    }
}

pub fn check_range(name: &str, v: Option<i32>, lo: i32, hi: i32) -> Result<(), AppError> {
    match v {
        Some(x) if x < lo || x > hi => Err(bad(format!("{name} must be between {lo} and {hi}"))),
        _ => Ok(()),
    }
}

/// For PATCH bodies: absent field = `None`, explicit `null` = `Some(None)`.
pub fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Run one round of a background job in its own task, so a bug that panics is logged
/// and the job simply runs again next time, instead of the loop (or server) stopping.
pub async fn guarded<F: std::future::Future<Output = ()> + Send + 'static>(name: &str, job: F) {
    if let Err(e) = tokio::spawn(job).await
        && e.is_panic()
    {
        tracing::error!("{name} job panicked; it will run again next time");
    }
}
