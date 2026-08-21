use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

const PKCE_VERIFIER_LEN: usize = 43;
const STATE_BYTES_LEN: usize = 32;
const PKCE_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OauthCodecError {
    InvalidLength,
    InvalidByte,
}

impl std::fmt::Display for OauthCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OauthCodecError::InvalidLength => write!(f, "invalid base64url length"),
            OauthCodecError::InvalidByte => write!(f, "invalid base64url byte"),
        }
    }
}

impl std::error::Error for OauthCodecError {}

impl From<base64::DecodeError> for OauthCodecError {
    fn from(error: base64::DecodeError) -> Self {
        match error {
            base64::DecodeError::InvalidLength(_) => OauthCodecError::InvalidLength,
            _ => OauthCodecError::InvalidByte,
        }
    }
}

pub(crate) type RandomError = getrandom::Error;

pub(crate) fn base64url_encode(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

pub(crate) fn base64url_decode(s: &str) -> Result<Vec<u8>, OauthCodecError> {
    URL_SAFE_NO_PAD.decode(s).map_err(OauthCodecError::from)
}

pub(crate) struct PkceCodes {
    pub verifier: String,
    pub challenge: String,
}

pub(crate) fn generate_pkce() -> Result<PkceCodes, RandomError> {
    let mut indexes = [0u8; PKCE_VERIFIER_LEN];
    getrandom::fill(&mut indexes)?;
    let verifier: String = indexes
        .iter()
        .map(|&index| PKCE_ALPHABET[(index % 64) as usize] as char)
        .collect();
    let digest = Sha256::digest(verifier.as_bytes());
    Ok(PkceCodes {
        challenge: base64url_encode(&digest),
        verifier,
    })
}

pub(crate) fn generate_state() -> Result<String, RandomError> {
    let mut bytes = [0u8; STATE_BYTES_LEN];
    getrandom::fill(&mut bytes)?;
    Ok(base64url_encode(&bytes))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn base64url_roundtrips_and_avoids_padding() {
        let input = b"subjects?_";
        let encoded = super::base64url_encode(input);
        assert_eq!(encoded, "c3ViamVjdHM_Xw"); // no '+', '/', '='
        assert_eq!(super::base64url_decode(&encoded).unwrap(), input);
        // second remainder path (len % 4 == 3)
        assert_eq!(super::base64url_encode(b"subjects?__"), "c3ViamVjdHM_X18");
    }

    #[test]
    fn pkce_verifier_matches_challenge() {
        let pkce = super::generate_pkce().unwrap();
        assert_eq!(pkce.verifier.len(), 43);
        let digest = Sha256::digest(pkce.verifier.as_bytes());
        assert_eq!(super::base64url_encode(&digest), pkce.challenge);
    }

    #[test]
    fn state_is_unique_and_urlsafe() {
        let a = super::generate_state().unwrap();
        let b = super::generate_state().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()
            || c == '-'
            || c == '_'
            || c == '.'
            || c == '~'));
    }
}
