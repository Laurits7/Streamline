//! Talking to the user's Streamline server (D-78). The page never reaches the network
//! itself (its CSP only allows IPC): it asks the app, which adds the device token and
//! only ever calls the one configured server, with the same outbound rules as the server's
//! own client (D-67): http(s) only, no link-local, multicast or unspecified addresses, no
//! redirects, size-limited responses.

use std::net::IpAddr;
use std::time::Duration;

use reqwest::Url;

/// Largest response passed to the page (a full `/sync` of a busy household fits easily).
pub const MAX_BODY: usize = 64 * 1024 * 1024;

/// Addresses the app never connects to.
fn blocked(ip: IpAddr) -> bool {
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

/// The server address as typed ("streamline.lan:3000", "https://todo.example.org/") in
/// its normal form: scheme, host, port and an optional path prefix, no trailing slash.
/// A missing scheme means http:// (home servers on the LAN usually have no certificate).
pub fn normalize_server(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("Enter your server's address".into());
    }
    let with_scheme = if s.contains("://") {
        s.to_string()
    } else {
        format!("http://{s}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| "That isn't a valid address".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("The address must start with http:// or https://".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Leave the user name out of the address".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("The address can't have ? or # parts".into());
    }
    let host = url.host_str().ok_or("The address has no host")?;
    if let Ok(ip) = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        && blocked(ip)
    {
        return Err("That address can't be used".into());
    }
    let mut out = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        out.push_str(&format!(":{port}"));
    }
    let path = url.path().trim_end_matches('/');
    // Pasted the app's own URL (…/api/v1 or a page): keep only the prefix before it.
    let path = path.split("/api/").next().unwrap_or("");
    out.push_str(path);
    Ok(out)
}

/// A path the page may request under `/api/v1`: absolute, no `..`, no scheme or host,
/// no backslashes or control characters.
pub fn check_path(path: &str) -> Result<(), String> {
    let bad = !path.starts_with('/')
        || path.starts_with("//")
        || path.contains("..")
        || path.contains('\\')
        || path.contains("://")
        || path.chars().any(|c| c.is_control() || c == ' ' || c == '#');
    if bad {
        Err(format!("refused path {path}"))
    } else {
        Ok(())
    }
}

pub fn api_url(server: &str, path: &str) -> Result<Url, String> {
    check_path(path)?;
    Url::parse(&format!("{server}/api/v1{path}")).map_err(|e| e.to_string())
}

/// The HTTP client for the configured server. Name resolution is checked as well, so a
/// name pointing at a refused address isn't used either.
pub fn client(timeout: Option<Duration>) -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut b = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(std::sync::Arc::new(SafeResolver))
        .user_agent(concat!("Streamline-Desktop/", env!("CARGO_PKG_VERSION")));
    if let Some(t) = timeout {
        b = b.timeout(t);
    }
    b.build().expect("HTTP client")
}

struct SafeResolver;

impl reqwest::dns::Resolve for SafeResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = format!("{}:0", name.as_str());
            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(host).await?.collect();
            if addrs.is_empty() || addrs.iter().any(|a| blocked(a.ip())) {
                return Err("refused address".into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Server-sent events, fed a chunk at a time (`/api/v1/events`).
#[derive(Default)]
pub struct SseParser {
    buf: String,
    event: String,
    data: Vec<String>,
}

impl SseParser {
    /// Complete events in `chunk` (with what was left from earlier chunks), as (event, data).
    pub fn feed(&mut self, chunk: &str) -> Vec<(String, String)> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();
        while let Some(i) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=i).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !self.data.is_empty() || !self.event.is_empty() {
                    let event = if self.event.is_empty() {
                        "message".into()
                    } else {
                        std::mem::take(&mut self.event)
                    };
                    out.push((event, self.data.join("\n")));
                    self.data.clear();
                }
            } else if let Some(v) = line.strip_prefix("event:") {
                self.event = v.trim_start().to_string();
            } else if let Some(v) = line.strip_prefix("data:") {
                self.data.push(v.strip_prefix(' ').unwrap_or(v).to_string());
            }
            // Comments (":keep-alive"), id: and retry: lines are ignored.
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_addresses() {
        assert_eq!(
            normalize_server("streamline.lan:3000").unwrap(),
            "http://streamline.lan:3000"
        );
        assert_eq!(
            normalize_server(" https://todo.example.org/ ").unwrap(),
            "https://todo.example.org"
        );
        assert_eq!(
            normalize_server("https://example.org/streamline/").unwrap(),
            "https://example.org/streamline"
        );
        assert_eq!(
            normalize_server("http://192.168.1.5:3000/api/v1/day").unwrap(),
            "http://192.168.1.5:3000"
        );
        assert_eq!(
            normalize_server("http://[::1]:3000").unwrap(),
            "http://[::1]:3000"
        );
        for bad in [
            "",
            "ftp://x",
            "http://user:pw@x",
            "http://169.254.169.254",
            "http://[fe80::1]",
            "http://0.0.0.0",
            "http://x/?a=1",
            "http://",
        ] {
            assert!(normalize_server(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn paths() {
        assert!(check_path("/tasks/01J?x=1").is_ok());
        assert!(check_path("/days/2026-10-14/log").is_ok());
        for bad in [
            "tasks",
            "//evil.org/x",
            "/../admin",
            "/a\\b",
            "/x?u=http://evil",
            "/a b",
            "/a\nb",
            "/a#b",
        ] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
        assert_eq!(
            api_url("https://e.org/s", "/me").unwrap().as_str(),
            "https://e.org/s/api/v1/me"
        );
    }

    #[test]
    fn sse() {
        let mut p = SseParser::default();
        assert!(p.feed(":keep-alive\n\nevent: hel").is_empty());
        assert_eq!(
            p.feed("lo\ndata: {}\n\nevent: change\r\ndata: {\"kind\":\"task\",")
                .len(),
            1
        );
        let got = p.feed("\"data\":1}\r\n\r\ndata: a\ndata: b\n\n");
        assert_eq!(
            got,
            vec![
                ("change".into(), "{\"kind\":\"task\",\"data\":1}".into()),
                ("message".into(), "a\nb".into())
            ]
        );
    }
}
