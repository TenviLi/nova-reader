use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

use nova_core::domain::user::Claims;
use nova_core::{Error, Result};

/// JWT token manager.
pub struct JwtService {
    secret: Vec<u8>,
    access_ttl: Duration,
    refresh_ttl: Duration,
}

impl JwtService {
    #[cfg(test)]
    pub fn new(secret: &str) -> Self {
        Self::with_access_ttl(secret, Duration::minutes(30))
    }

    pub fn with_access_ttl(secret: &str, access_ttl: Duration) -> Self {
        Self {
            secret: secret.as_bytes().to_vec(),
            access_ttl,
            refresh_ttl: Duration::days(30),
        }
    }

    pub fn access_ttl_seconds(&self) -> i64 {
        self.access_ttl.num_seconds()
    }

    /// Generate an access token for a user with their role.
    pub fn create_access_token(&self, user_id: &str, role: &str) -> Result<String> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            iat: now.timestamp(),
            exp: (now + self.access_ttl).timestamp(),
            token_type: "access".to_string(),
            role: Some(role.to_string()),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.secret),
        )
        .map_err(|e| Error::Internal(format!("JWT encode error: {e}")))
    }

    /// Generate a refresh token for a user.
    pub fn create_refresh_token(&self, user_id: &str) -> Result<(String, chrono::DateTime<Utc>)> {
        let now = Utc::now();
        let expires_at = now + self.refresh_ttl;
        let claims = Claims {
            sub: user_id.to_string(),
            iat: now.timestamp(),
            exp: expires_at.timestamp(),
            token_type: "refresh".to_string(),
            role: None,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.secret),
        )
        .map_err(|e| Error::Internal(format!("JWT encode error: {e}")))?;

        Ok((token, expires_at))
    }

    /// Validate and decode a token.
    pub fn validate_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.secret),
            &Validation::default(),
        )
        .map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => Error::Unauthorized,
            _ => Error::InvalidCredentials,
        })?;

        Ok(token_data.claims)
    }

    /// Hash a refresh token for storage (prevent DB leak → token compromise).
    pub fn hash_token(token: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
}
