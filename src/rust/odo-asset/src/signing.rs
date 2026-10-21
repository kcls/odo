//! Short-lived signed URLs for file retrieval.
//!
//! A signed URL carries `expires`, `uid` and `sig` query parameters, where
//! `sig` is an HMAC-SHA256 over the file's relative path, the signing user
//! and the expiry. Retrieval accepts one in place of a JWT, so `<img>` and
//! `<video>` elements keep working after the session's access token
//! rotates, and the session token itself never lands in URLs.
//!
//! The read permission is checked when the URL is signed, not when it is
//! used: revoking a permission does not invalidate URLs already issued, so
//! keep the TTL short.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Expiries are rounded up to a multiple of this, so re-signing the same
/// path within a window yields the same URL and browsers can cache it.
const EXPIRY_BUCKET_SECS: i64 = 300;

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    Expired,
    BadSignature,
}

pub struct UrlSigner {
    key: Vec<u8>,
    ttl_secs: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Signature {
    pub expires: i64,
    pub sig: String,
}

impl UrlSigner {
    pub fn new(key: Vec<u8>, ttl_secs: i64) -> Self {
        Self { key, ttl_secs }
    }

    /// Sign `rel_path` for `user_id`. The URL is valid for at least the
    /// configured TTL and at most one bucket longer.
    pub fn sign(&self, rel_path: &str, user_id: i64, now: i64) -> Signature {
        let expires = (now + self.ttl_secs).div_euclid(EXPIRY_BUCKET_SECS) * EXPIRY_BUCKET_SECS
            + EXPIRY_BUCKET_SECS;
        let sig = hex::encode(self.mac(rel_path, user_id, expires).finalize().into_bytes());
        Signature { expires, sig }
    }

    pub fn verify(
        &self,
        rel_path: &str,
        user_id: i64,
        expires: i64,
        sig: &str,
        now: i64,
    ) -> Result<(), VerifyError> {
        // Check the MAC first so an expired-vs-forged distinction is only
        // ever reported for URLs we actually issued.
        let sig = hex::decode(sig).map_err(|_| VerifyError::BadSignature)?;
        self.mac(rel_path, user_id, expires)
            .verify_slice(&sig)
            .map_err(|_| VerifyError::BadSignature)?;
        if now >= expires {
            return Err(VerifyError::Expired);
        }
        Ok(())
    }

    fn mac(&self, rel_path: &str, user_id: i64, expires: i64) -> HmacSha256 {
        let mut mac =
            HmacSha256::new_from_slice(&self.key).expect("HMAC accepts keys of any length");
        // Versioned and newline-delimited. The path is the only free-form
        // field and the trailing two are integers, so the encoding stays
        // unambiguous even if a path contains a newline.
        mac.update(format!("v1\n{rel_path}\n{user_id}\n{expires}").as_bytes());
        mac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn signer() -> UrlSigner {
        UrlSigner::new(b"test-key".to_vec(), 3600)
    }

    #[test]
    fn round_trip() {
        let s = signer();
        let sig = s.sign("current/photos/a.jpg", 7, NOW);
        assert_eq!(
            s.verify("current/photos/a.jpg", 7, sig.expires, &sig.sig, NOW),
            Ok(())
        );
    }

    #[test]
    fn expiry_is_bucketed_and_covers_ttl() {
        let s = signer();
        let a = s.sign("p", 1, NOW);
        let b = s.sign("p", 1, NOW + 1);
        assert_eq!(a, b, "signatures within a bucket should be identical");
        assert_eq!(a.expires % EXPIRY_BUCKET_SECS, 0);
        assert!(a.expires >= NOW + 3600);
        assert!(a.expires <= NOW + 3600 + EXPIRY_BUCKET_SECS);
    }

    #[test]
    fn expired_rejected() {
        let s = signer();
        let sig = s.sign("p", 1, NOW);
        assert_eq!(
            s.verify("p", 1, sig.expires, &sig.sig, sig.expires),
            Err(VerifyError::Expired)
        );
    }

    #[test]
    fn tampering_rejected() {
        let s = signer();
        let sig = s.sign("current/photos/a.jpg", 7, NOW);
        let bad =
            |path: &str, uid: i64, expires: i64, sig: &str| s.verify(path, uid, expires, sig, NOW);
        assert_eq!(
            bad("current/photos/b.jpg", 7, sig.expires, &sig.sig),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            bad("current/photos/a.jpg", 8, sig.expires, &sig.sig),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            bad("current/photos/a.jpg", 7, sig.expires + 300, &sig.sig),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            bad("current/photos/a.jpg", 7, sig.expires, "zz"),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            bad("current/photos/a.jpg", 7, sig.expires, ""),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn different_key_rejected() {
        let sig = signer().sign("p", 1, NOW);
        let other = UrlSigner::new(b"other-key".to_vec(), 3600);
        assert_eq!(
            other.verify("p", 1, sig.expires, &sig.sig, NOW),
            Err(VerifyError::BadSignature)
        );
    }
}
