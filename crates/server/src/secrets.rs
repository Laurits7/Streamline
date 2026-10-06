//! Encryption for stored credentials (CalDAV passwords, later integration tokens).
//! The key comes from `SECRET_KEY` (any string; hashed to 256 bits) or, if unset, from
//! `data/secret.key`, which is generated on first start. ChaCha20-Poly1305 with a random
//! nonce per value; stored as nonce ‖ ciphertext.

use std::path::Path;

use anyhow::{Context, anyhow};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, AeadCore, OsRng},
};
use sha2::{Digest, Sha256};

pub struct Secrets {
    cipher: ChaCha20Poly1305,
}

impl Secrets {
    pub fn load(data_dir: &Path, secret_key: Option<&str>) -> anyhow::Result<Self> {
        let key: [u8; 32] = match secret_key {
            Some(s) => Sha256::digest(s.as_bytes()).into(),
            None => {
                let path = data_dir.join("secret.key");
                match std::fs::read(&path) {
                    Ok(b) if b.len() == 32 => b.try_into().unwrap(),
                    Ok(_) => anyhow::bail!("{} is not a 32-byte key", path.display()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        let k: [u8; 32] = ChaCha20Poly1305::generate_key(&mut OsRng).into();
                        write_private(&path, &k)
                            .with_context(|| format!("writing {}", path.display()))?;
                        k
                    }
                    Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
                }
            }
        };
        Ok(Self {
            cipher: ChaCha20Poly1305::new(Key::from_slice(&key)),
        })
    }

    pub fn encrypt(&self, plain: &str) -> Vec<u8> {
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let mut out = nonce.to_vec();
        out.extend(
            self.cipher
                .encrypt(&nonce, plain.as_bytes())
                .expect("encryption cannot fail"),
        );
        out
    }

    pub fn decrypt(&self, sealed: &[u8]) -> anyhow::Result<String> {
        let err = || {
            anyhow!(
                "the stored password can't be decrypted (was SECRET_KEY or data/secret.key changed?); enter it again"
            )
        };
        if sealed.len() < 12 {
            return Err(err());
        }
        let (nonce, body) = sealed.split_at(12);
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), body)
            .map_err(|_| err())?;
        String::from_utf8(plain).map_err(|_| err())
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    opts.open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_key_file() {
        let dir =
            std::env::temp_dir().join(format!("streamline-secrets-{}", crate::util::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = Secrets::load(&dir, None).unwrap();
        let sealed = a.encrypt("hunter2");
        assert!(!sealed.windows(7).any(|w| w == b"hunter2"));
        assert_ne!(a.encrypt("hunter2"), sealed, "fresh nonce each time");
        // The generated key is reused on the next start.
        assert_eq!(
            Secrets::load(&dir, None).unwrap().decrypt(&sealed).unwrap(),
            "hunter2"
        );
        // A different key can't read it.
        assert!(
            Secrets::load(&dir, Some("other"))
                .unwrap()
                .decrypt(&sealed)
                .is_err()
        );
        let b = Secrets::load(&dir, Some("k")).unwrap();
        assert_eq!(b.decrypt(&b.encrypt("x")).unwrap(), "x");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
