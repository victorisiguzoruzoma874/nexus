use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub expiry_hours: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let jwt_secret = env::var("JWT_SECRET").context("JWT_SECRET must be set")?;
        validate_jwt_secret(&jwt_secret)?;

        Ok(Self {
            server: ServerConfig {
                host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
                // Prefer the platform-injected `PORT` (Render/Railway/Heroku),
                // fall back to `SERVER_PORT`, then default to 8080 locally.
                port: env::var("PORT")
                    .or_else(|_| env::var("SERVER_PORT"))
                    .unwrap_or_else(|_| "8080".to_string())
                    .parse()
                    .context("PORT/SERVER_PORT must be a valid port number")?,
            },
            database: DatabaseConfig {
                url: env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
                max_connections: env::var("DATABASE_MAX_CONNECTIONS")
                    .unwrap_or_else(|_| "10".to_string())
                    .parse()
                    .context("DATABASE_MAX_CONNECTIONS must be a number")?,
            },
            jwt: JwtConfig {
                secret: jwt_secret,
                expiry_hours: env::var("JWT_EXPIRY_HOURS")
                    .unwrap_or_else(|_| "24".to_string())
                    .parse()
                    .context("JWT_EXPIRY_HOURS must be a number")?,
            },
        })
    }
}

/// Minimum recommended length for `JWT_SECRET` (matches `.env.example`).
const MIN_JWT_SECRET_LEN: usize = 32;

/// Reject a missing/empty signing secret (fail fast at startup) and warn when it
/// is shorter than the recommended length. An empty secret would make every
/// issued token forgeable, so it must never reach the running server.
fn validate_jwt_secret(secret: &str) -> Result<()> {
    if secret.trim().is_empty() {
        anyhow::bail!("JWT_SECRET must not be empty");
    }
    if secret.len() < MIN_JWT_SECRET_LEN {
        tracing::warn!(
            "JWT_SECRET is shorter than the recommended {MIN_JWT_SECRET_LEN} characters"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_secret() {
        assert!(validate_jwt_secret("").is_err());
    }

    #[test]
    fn rejects_whitespace_only_secret() {
        assert!(validate_jwt_secret("   ").is_err());
    }

    #[test]
    fn accepts_a_strong_secret() {
        assert!(validate_jwt_secret("a-sufficiently-long-secret-0123456789").is_ok());
    }

    #[test]
    fn accepts_but_warns_on_short_secret() {
        // Non-empty but short: allowed (returns Ok); a warning is logged.
        assert!(validate_jwt_secret("short").is_ok());
    }
}
