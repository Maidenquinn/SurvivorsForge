//! Authentication as a GitHub App: a short-lived RS256 JWT signed with the App's private key.

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;

#[derive(Debug, Serialize, serde::Deserialize, PartialEq)]
pub struct Claims {
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
}

/// `iat` lies 60 s back against clock drift; GitHub allows at most 10 minutes of lifetime.
pub fn claims(app_id: &str, now: DateTime<Utc>) -> Claims {
    Claims {
        iat: (now - Duration::seconds(60)).timestamp(),
        exp: (now + Duration::minutes(9)).timestamp(),
        iss: app_id.to_string(),
    }
}

pub fn app_jwt(app_id: &str, key: &EncodingKey, now: DateTime<Utc>) -> Result<String, jsonwebtoken::errors::Error> {
    jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims(app_id, now), key)
}

/// The PEM from `GITHUB_APP_PRIVATE_KEY`, or else from the file `GITHUB_APP_PRIVATE_KEY_FILE` names.
pub fn load_private_key(value: Option<&str>, file: Option<&str>) -> Result<EncodingKey, String> {
    let pem = match (value.filter(|v| !v.trim().is_empty()), file.filter(|f| !f.trim().is_empty())) {
        (Some(value), _) => value.replace("\\n", "\n"),
        (None, Some(file)) => std::fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?,
        (None, None) => return Err("GITHUB_APP_PRIVATE_KEY or GITHUB_APP_PRIVATE_KEY_FILE is not set".into()),
    };
    EncodingKey::from_rsa_pem(pem.as_bytes()).map_err(|e| format!("the GitHub App private key is invalid: {e}"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jsonwebtoken::{DecodingKey, Validation};
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
    use std::sync::OnceLock;

    /// A key pair generated at test time, never stored in the repo. (private PEM, public PEM)
    pub fn test_keys() -> &'static (String, String) {
        static KEYS: OnceLock<(String, String)> = OnceLock::new();
        KEYS.get_or_init(|| {
            let private = rsa::RsaPrivateKey::new(&mut rand_core(), 2048).unwrap();
            let public = private.to_public_key();
            (
                private.to_pkcs8_pem(LineEnding::LF).unwrap().to_string(),
                public.to_public_key_pem(LineEnding::LF).unwrap(),
            )
        })
    }

    fn rand_core() -> rsa::rand_core::OsRng {
        rsa::rand_core::OsRng
    }

    #[test]
    fn jwt_claims_are_valid() {
        let (private, public) = test_keys();
        let now = DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z").unwrap().with_timezone(&Utc);
        let jwt = app_jwt("12345", &EncodingKey::from_rsa_pem(private.as_bytes()).unwrap(), now).unwrap();

        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = false;
        let decoded = jsonwebtoken::decode::<Claims>(&jwt, &DecodingKey::from_rsa_pem(public.as_bytes()).unwrap(), &validation)
            .unwrap();
        assert_eq!(decoded.header.alg, Algorithm::RS256);
        assert_eq!(decoded.claims.iss, "12345");
        assert_eq!(decoded.claims.iat, now.timestamp() - 60);
        assert!(decoded.claims.exp - now.timestamp() <= 600);
        assert!(decoded.claims.exp > now.timestamp());
    }

    #[test]
    fn private_key_from_file_or_value() {
        let (private, _) = test_keys();
        assert!(load_private_key(Some(private), None).is_ok());
        assert!(load_private_key(Some(&private.replace('\n', "\\n")), None).is_ok(), "escaped newlines from .env");

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app.pem");
        std::fs::write(&file, private).unwrap();
        assert!(load_private_key(None, Some(file.to_str().unwrap())).is_ok());
        assert!(load_private_key(Some(""), Some(file.to_str().unwrap())).is_ok());

        assert!(load_private_key(None, None).err().unwrap().contains("GITHUB_APP_PRIVATE_KEY"));
        assert!(load_private_key(Some("not a key"), None).is_err());
    }
}
