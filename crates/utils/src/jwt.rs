use chrono::{DateTime, Utc};
use jsonwebtoken::dangerous::insecure_decode;
use serde::{Deserialize, de::DeserializeOwned};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TokenClaimsError {
    #[error("failed to decode JWT: {0}")]
    Decode(#[from] jsonwebtoken::errors::Error),
    #[error("missing `exp` claim in token")]
    MissingExpiration,
    #[error("invalid `exp` value `{0}`")]
    InvalidExpiration(i64),
    #[error("missing `sub` claim in token")]
    MissingSubject,
    #[error("invalid `sub` value: {0}")]
    InvalidSubject(String),
}

#[derive(Debug, Deserialize)]
struct ExpClaim {
    exp: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SubClaim {
    sub: Option<String>,
}

/// Extract the expiration timestamp from a JWT without verifying its signature.
pub fn extract_expiration(token: &str) -> Result<DateTime<Utc>, TokenClaimsError> {
    let data = insecure_decode::<ExpClaim>(token)?;
    let exp = data.claims.exp.ok_or(TokenClaimsError::MissingExpiration)?;
    DateTime::from_timestamp(exp, 0).ok_or(TokenClaimsError::InvalidExpiration(exp))
}

/// Extract the subject (user ID) from a JWT without verifying its signature.
pub fn extract_subject(token: &str) -> Result<Uuid, TokenClaimsError> {
    let data = insecure_decode::<SubClaim>(token)?;
    let sub = data.claims.sub.ok_or(TokenClaimsError::MissingSubject)?;
    Uuid::parse_str(&sub).map_err(|_| TokenClaimsError::InvalidSubject(sub))
}

/// Extract custom claims from a JWT without verifying its signature.
///
/// This function deserializes the JWT payload into any type that implements
/// `DeserializeOwned`. Use `Option<T>` for fields that may be missing.
///
/// # Example
/// ```ignore
/// #[derive(Deserialize)]
/// struct MyClaims {
///     custom_field: Option<String>,
///     #[serde(rename = "https://api.example.com/auth")]
///     nested_auth: Option<AuthInfo>,
/// }
/// let claims: MyClaims = extract_custom_claims(token)?;
/// ```
pub fn extract_custom_claims<T: DeserializeOwned>(token: &str) -> Result<T, TokenClaimsError> {
    let data = insecure_decode::<T>(token)?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    // Test JWT with payload: {"sub": "550e8400-e29b-41d4-a716-446655440000", "exp": 1893456000}
    const VALID_JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiI1NTBlODQwMC1lMjliLTQxZDQtYTcxNi00NDY2NTU0NDAwMDAiLCJleHAiOjE4OTM0NTYwMDB9.signature";

    // Test JWT with nested claims:
    // {
    //   "sub": "user123",
    //   "exp": 1893456000,
    //   "https://api.openai.com/auth": {
    //     "subscription": "pro",
    //     "tier": 2
    //   },
    //   "custom_field": "hello"
    // }
    const NESTED_CLAIMS_JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIiwiZXhwIjoxODkzNDU2MDAwLCJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsic3Vic2NyaXB0aW9uIjoicHJvIiwidGllciI6Mn0sImN1c3RvbV9maWVsZCI6ImhlbGxvIn0.signature";

    #[test]
    fn extract_custom_claims_simple() {
        #[derive(Debug, Deserialize)]
        struct SimpleClaims {
            exp: i64,
            sub: String,
        }

        let claims: SimpleClaims = extract_custom_claims(VALID_JWT).unwrap();
        assert_eq!(claims.exp, 1893456000);
        assert_eq!(claims.sub, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn extract_custom_claims_nested() {
        #[derive(Debug, Deserialize)]
        struct AuthInfo {
            subscription: String,
            tier: u32,
        }

        #[derive(Debug, Deserialize)]
        struct NestedClaims {
            custom_field: String,
            #[serde(rename = "https://api.openai.com/auth")]
            openai_auth: AuthInfo,
        }

        let claims: NestedClaims = extract_custom_claims(NESTED_CLAIMS_JWT).unwrap();
        assert_eq!(claims.custom_field, "hello");
        assert_eq!(claims.openai_auth.subscription, "pro");
        assert_eq!(claims.openai_auth.tier, 2);
    }

    #[test]
    fn extract_custom_claims_optional_missing() {
        #[derive(Debug, Deserialize)]
        struct ClaimsWithOptional {
            exp: i64,
            missing_field: Option<String>,
        }

        let claims: ClaimsWithOptional = extract_custom_claims(VALID_JWT).unwrap();
        assert_eq!(claims.exp, 1893456000);
        assert!(claims.missing_field.is_none());
    }

    #[test]
    fn extract_custom_claims_invalid_jwt() {
        #[derive(Debug, Deserialize)]
        struct AnyClaims {
            #[serde(default)]
            _marker: (),
        }

        let result = extract_custom_claims::<AnyClaims>("not.a.jwt");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TokenClaimsError::Decode(_)));
    }

    #[test]
    fn extract_custom_claims_malformed_token() {
        #[derive(Debug, Deserialize)]
        struct AnyClaims {
            #[serde(default)]
            _marker: (),
        }

        let result = extract_custom_claims::<AnyClaims>("completely-invalid");
        assert!(result.is_err());
    }
}
