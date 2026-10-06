//! Web Push (RFC 8030/8291/8292) and ntfy delivery for notifications (Phase 6.2, D-62).
//! The VAPID key pair lives in `data/vapid.key` (generated on first start). Messages are
//! encrypted with `web-push-native`; the VAPID token (an ES256 JWT) is signed here.

use std::path::Path;

use anyhow::Context;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use p256::{
    SecretKey,
    ecdsa::{Signature, SigningKey, signature::Signer},
    elliptic_curve::sec1::ToEncodedPoint,
};
use rand::rngs::OsRng;
use reqwest::Url;

use crate::{AppState, notify::Notification};

pub struct Vapid {
    key: SigningKey,
    /// Uncompressed public key, base64url: the browser's `applicationServerKey`.
    pub public_key: String,
    contact: String,
}

impl Vapid {
    pub fn load(data_dir: &Path, contact: &str) -> anyhow::Result<Self> {
        let path = data_dir.join("vapid.key");
        let secret = match std::fs::read(&path) {
            Ok(b) => SecretKey::from_slice(&b).context("data/vapid.key is not a P-256 key")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let k = SecretKey::random(&mut OsRng);
                crate::secrets::write_private(&path, &k.to_bytes())
                    .with_context(|| format!("writing {}", path.display()))?;
                k
            }
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let public_key = B64.encode(secret.public_key().to_encoded_point(false).as_bytes());
        Ok(Self {
            key: SigningKey::from(&secret),
            public_key,
            contact: contact.into(),
        })
    }

    /// `Authorization: vapid t=<jwt>, k=<public key>` for a push endpoint.
    pub fn header(&self, endpoint: &Url) -> String {
        let aud = format!(
            "{}://{}",
            endpoint.scheme(),
            endpoint.host_str().unwrap_or_default()
        );
        let exp = chrono::Utc::now().timestamp() + 12 * 3600;
        let head = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = B64
            .encode(serde_json::json!({"aud": aud, "exp": exp, "sub": self.contact}).to_string());
        let input = format!("{head}.{claims}");
        let sig: Signature = self.key.sign(input.as_bytes());
        format!(
            "vapid t={input}.{}, k={}",
            B64.encode(sig.to_bytes()),
            self.public_key
        )
    }
}

#[derive(sqlx::FromRow)]
struct Subscription {
    id: String,
    endpoint: String,
    p256dh: String,
    auth: String,
}

fn client() -> anyhow::Result<reqwest::Client> {
    crate::caldav::install_crypto();
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("Streamline/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// Send to every device of the user (and ntfy, if set). Dead subscriptions are removed.
pub async fn deliver(state: &AppState, user_id: &str, n: &Notification) -> anyhow::Result<()> {
    let http = client()?;
    let subs: Vec<Subscription> = sqlx::query_as(
        "SELECT id, endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.db.read)
    .await?;
    let payload = serde_json::to_vec(n)?;
    for s in subs {
        match send_one(state, &http, &s, &payload).await {
            Ok(status) if status.is_success() => {
                sqlx::query(
                    "UPDATE push_subscriptions SET last_ok_at = ?, failures = 0 WHERE id = ?",
                )
                .bind(crate::util::now())
                .bind(&s.id)
                .execute(&state.db.write)
                .await?;
            }
            // Gone: the user unsubscribed or reinstalled.
            Ok(status) if status.as_u16() == 404 || status.as_u16() == 410 => {
                sqlx::query("DELETE FROM push_subscriptions WHERE id = ?")
                    .bind(&s.id)
                    .execute(&state.db.write)
                    .await?;
            }
            other => {
                tracing::info!("push to a device failed: {other:?}");
                sqlx::query("UPDATE push_subscriptions SET failures = failures + 1 WHERE id = ?")
                    .bind(&s.id)
                    .execute(&state.db.write)
                    .await?;
                sqlx::query("DELETE FROM push_subscriptions WHERE id = ? AND failures >= 20")
                    .bind(&s.id)
                    .execute(&state.db.write)
                    .await?;
            }
        }
    }
    let ntfy: Option<String> = sqlx::query_scalar("SELECT ntfy_url FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(&state.db.read)
        .await?;
    if let Some(url) = ntfy.filter(|u| !u.trim().is_empty()) {
        let mut req = http
            .post(url.trim())
            .header("Title", &n.title)
            .header("Tags", "streamline")
            .body(n.body.clone());
        if let Some(base) = &state.config.public_url {
            req = req.header("Click", format!("{}{}", base.trim_end_matches('/'), n.url));
        }
        if let Err(e) = req.send().await.and_then(|r| r.error_for_status()) {
            tracing::info!("ntfy delivery failed: {}", e.without_url());
        }
    }
    Ok(())
}

async fn send_one(
    state: &AppState,
    http: &reqwest::Client,
    s: &Subscription,
    payload: &[u8],
) -> anyhow::Result<reqwest::StatusCode> {
    let endpoint = Url::parse(&s.endpoint)?;
    let ua_public = p256::PublicKey::from_sec1_bytes(&B64.decode(s.p256dh.trim_end_matches('='))?)?;
    let auth_bytes = B64.decode(s.auth.trim_end_matches('='))?;
    anyhow::ensure!(auth_bytes.len() == 16, "bad auth secret");
    let auth = web_push_native::Auth::clone_from_slice(&auth_bytes);
    let req = web_push_native::WebPushBuilder::new(s.endpoint.parse()?, ua_public, auth)
        .with_valid_duration(std::time::Duration::from_secs(12 * 3600))
        .build(payload.to_vec())?;
    let mut builder = http
        .post(endpoint.clone())
        .header("Authorization", state.vapid.header(&endpoint))
        .header("Urgency", "high");
    for (k, v) in req.headers() {
        if k.as_str() != "content-length" {
            builder = builder.header(k.as_str(), v.as_bytes());
        }
    }
    let res = builder
        .body(req.into_body())
        .send()
        .await
        .map_err(|e| anyhow::Error::new(e.without_url()))?;
    Ok(res.status())
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{VerifyingKey, signature::Verifier};

    #[test]
    fn vapid_token_verifies() {
        let dir = std::env::temp_dir().join(format!("streamline-vapid-{}", crate::util::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let v = Vapid::load(&dir, "mailto:me@example.org").unwrap();
        assert_eq!(
            Vapid::load(&dir, "mailto:me@example.org")
                .unwrap()
                .public_key,
            v.public_key,
            "key is kept"
        );
        let h = v.header(&Url::parse("https://web.push.apple.com/abc/def").unwrap());
        let (t, k) = h
            .strip_prefix("vapid t=")
            .unwrap()
            .split_once(", k=")
            .unwrap();
        assert_eq!(k, v.public_key);
        let parts: Vec<&str> = t.split('.').collect();
        let claims: serde_json::Value =
            serde_json::from_slice(&B64.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://web.push.apple.com");
        assert_eq!(claims["sub"], "mailto:me@example.org");
        let pk = VerifyingKey::from_sec1_bytes(&B64.decode(k).unwrap()).unwrap();
        let sig = Signature::from_slice(&B64.decode(parts[2]).unwrap()).unwrap();
        pk.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
            .unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
