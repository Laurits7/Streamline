//! Outbound HTTP for user-supplied addresses (CalDAV, Web Push, ntfy) (security review,
//! D-67). Users may point Streamline at servers on the LAN (a home Radicale is the normal
//! case), so private addresses are allowed; link-local addresses (cloud metadata at
//! 169.254.169.254, fe80::/10), unspecified and multicast addresses are refused, both for
//! IP literals and for every address a name resolves to. Redirects never leave the host,
//! and response bodies are size-limited.

use std::net::{IpAddr, SocketAddr};

use anyhow::{Context, bail};
use reqwest::{
    Url,
    dns::{Addrs, Name, Resolve, Resolving},
    redirect,
};

/// Addresses Streamline never connects to on a user's behalf.
pub fn blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_link_local() || v4.is_unspecified() || v4.is_multicast() || v4.is_broadcast()
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return blocked(IpAddr::V4(v4));
            }
            v6.is_unspecified() || v6.is_multicast() || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// An http(s) URL whose host isn't a refused IP literal.
pub fn check_url(url: &Url) -> anyhow::Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        bail!("the address must start with http:// or https://");
    }
    let host = url.host_str().context("the address has no host")?;
    match host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
    {
        Ok(ip) if blocked(ip) => bail!("that address can't be used"),
        _ => Ok(()),
    }
}

/// Resolves names like the system does, minus refused addresses.
struct SafeResolver;

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|a| !blocked(a.ip()))
                .collect();
            if addrs.is_empty() {
                return Err("that address can't be used".into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// A client for user-supplied addresses. `follow_same_host`: allow up to 5 redirects that
/// stay on the same host (CalDAV servers do this); otherwise none.
pub fn client(timeout_secs: u64, follow_same_host: bool) -> anyhow::Result<reqwest::Client> {
    crate::caldav::install_crypto();
    let policy = if follow_same_host {
        redirect::Policy::custom(|attempt| {
            let same = attempt
                .previous()
                .first()
                .map(|u| (u.scheme(), u.host_str(), u.port()))
                == Some((
                    attempt.url().scheme(),
                    attempt.url().host_str(),
                    attempt.url().port(),
                ));
            if attempt.previous().len() < 5 && same {
                attempt.follow()
            } else {
                attempt.stop()
            }
        })
    } else {
        redirect::Policy::none()
    };
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .connect_timeout(std::time::Duration::from_secs(10))
        .redirect(policy)
        .dns_resolver(std::sync::Arc::new(SafeResolver))
        .user_agent(concat!("Streamline/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// The response body as text, refusing more than `max` bytes.
pub async fn text_limited(mut res: reqwest::Response, max: usize) -> anyhow::Result<String> {
    if res.content_length().is_some_and(|n| n as usize > max) {
        bail!("the server sent too much data");
    }
    let mut buf = Vec::new();
    while let Some(chunk) = res.chunk().await.context("reading the response")? {
        if buf.len() + chunk.len() > max {
            bail!("the server sent too much data");
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_addresses() {
        for ok in [
            "192.168.1.10",
            "10.0.0.5",
            "127.0.0.1",
            "8.8.8.8",
            "::1",
            "fd00::1",
        ] {
            assert!(!blocked(ok.parse().unwrap()), "{ok}");
        }
        for bad in [
            "169.254.169.254",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "fe80::1",
            "::",
            "::ffff:169.254.169.254",
        ] {
            assert!(blocked(bad.parse().unwrap()), "{bad}");
        }
        assert!(
            check_url(&Url::parse("http://169.254.169.254/latest/meta-data").unwrap()).is_err()
        );
        assert!(check_url(&Url::parse("http://[fe80::1]/").unwrap()).is_err());
        assert!(check_url(&Url::parse("file:///etc/passwd").unwrap()).is_err());
        assert!(check_url(&Url::parse("http://radicale:5232/").unwrap()).is_ok());
    }
}
