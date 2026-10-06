use std::{net::IpAddr, path::PathBuf};

use anyhow::{Context, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieSecure {
    /// Secure only when the request arrived over HTTPS (via a trusted proxy).
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    pub initial_admin_user: Option<String>,
    pub initial_admin_password: Option<String>,
    /// Trust `X-Forwarded-Proto` / `X-Forwarded-Host` from a reverse proxy.
    pub trust_proxy: bool,
    pub cookie_secure: CookieSecure,
    pub session_days: i64,
    /// Key for encrypting stored credentials; if unset, `data/secret.key` is generated.
    pub secret_key: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let cookie_secure = match var("COOKIE_SECURE").as_deref().unwrap_or("auto") {
            "auto" => CookieSecure::Auto,
            "true" | "1" => CookieSecure::Always,
            "false" | "0" => CookieSecure::Never,
            other => bail!("COOKIE_SECURE must be auto, true or false (got {other:?})"),
        };
        Ok(Self {
            bind: var("BIND")
                .unwrap_or_else(|| "0.0.0.0".into())
                .parse()
                .context("BIND")?,
            port: var("PORT")
                .unwrap_or_else(|| "3000".into())
                .parse()
                .context("PORT")?,
            data_dir: var("DATA_DIR").unwrap_or_else(|| "./data".into()).into(),
            initial_admin_user: var("INITIAL_ADMIN_USER"),
            initial_admin_password: var("INITIAL_ADMIN_PASSWORD"),
            trust_proxy: matches!(var("TRUST_PROXY").as_deref(), Some("true" | "1")),
            cookie_secure,
            session_days: var("SESSION_DAYS")
                .map(|v| v.parse())
                .transpose()
                .context("SESSION_DAYS")?
                .unwrap_or(90),
            secret_key: var("SECRET_KEY"),
        })
    }

    /// Config for tests: a throwaway data directory.
    pub fn for_tests(data_dir: PathBuf) -> Self {
        Self {
            bind: "127.0.0.1".parse().unwrap(),
            port: 0,
            data_dir,
            initial_admin_user: None,
            initial_admin_password: None,
            trust_proxy: false,
            cookie_secure: CookieSecure::Auto,
            session_days: 90,
            secret_key: None,
        }
    }
}
