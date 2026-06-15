use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use bcrypt::{hash, verify};

#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("Password hashing failed: {0}")]
    HashingFailed(String),
    
    #[error("Password verification failed")]
    VerificationFailed,
    
    #[error("Invalid key: {0}")]
    InvalidKey(String),
}

/// Service for encryption and password hashing
pub struct EncryptionService {
    encryption_key: Vec<u8>,
    bcrypt_cost: u32,
}

impl EncryptionService {
    /// Create a new encryption service with a 32-byte key
    pub fn new(encryption_key: Vec<u8>) -> Result<Self, EncryptionError> {
        if encryption_key.len() != 32 {
            return Err(EncryptionError::InvalidKey(
                "Encryption key must be exactly 32 bytes for AES-256".to_string(), ));
        }

        Ok(Self {
            encryption_key,
            bcrypt_cost: 12, // Minimum work factor of 12 per requirements
        })
    }

    /// Create from base64-encoded key
    pub fn from_base64_key(base64_key: &str) -> Result<Self, EncryptionError> {
        let key = general_purpose::STANDARD
            .decode(base64_key)
            .map_err(|e| EncryptionError::InvalidKey(format!("Invalid base64: {}", e)))?;
        
        Self::new(key)
    }

    /// Generate a new random 32-byte encryption key
    pub fn generate_key() -> Vec<u8> {
        use aes_gcm::aead::rand_core::RngCore;
        let mut key = vec![0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    /// Encrypt payment token using AES-256-GCM
    pub fn encrypt_token(&self, token: &str) -> Result<String, EncryptionError> {
        // Create cipher instance
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| EncryptionError::EncryptionFailed(format!("Invalid key: {}", e)))?;

        // Generate random nonce (96 bits for GCM)
        use aes_gcm::aead::rand_core::RngCore;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the token
        let ciphertext = cipher
            .encrypt(nonce, token.as_bytes())
            .map_err(|e| EncryptionError::EncryptionFailed(format!("Encryption error: {}", e)))?;

        // Prepend nonce to ciphertext and encode as base64
        let mut result = nonce_bytes.to_vec(); result.extend_from_slice(&ciphertext);
        
        Ok(general_purpose::STANDARD.encode(result))
    }

    /// Decrypt payment token using AES-256-GCM
    pub fn decrypt_token(&self, encrypted_token: &str) -> Result<String, EncryptionError> {
        // Decode from base64
        let encrypted_data = general_purpose::STANDARD
            .decode(encrypted_token)
            .map_err(|e| EncryptionError::DecryptionFailed(format!("Invalid base64: {}", e)))?;

        // Extract nonce (first 12 bytes) and ciphertext
        if encrypted_data.len() < 12 {
            return Err(EncryptionError::DecryptionFailed(
                "Encrypted data too short".to_string(), ));
        }

        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        // Create cipher instance
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| EncryptionError::DecryptionFailed(format!("Invalid key: {}", e)))?;

        // Decrypt
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| EncryptionError::DecryptionFailed(format!("Decryption error: {}", e)))?;

        String::from_utf8(plaintext)
            .map_err(|e| EncryptionError::DecryptionFailed(format!("Invalid UTF-8: {}", e)))
    }

    /// Hash password using bcrypt with work factor >= 12
    pub fn hash_password(&self, password: &str) -> Result<String, EncryptionError> {
        hash(password, self.bcrypt_cost)
            .map_err(|e| EncryptionError::HashingFailed(format!("Bcrypt error: {}", e)))
    }

    /// Verify password against bcrypt hash
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool, EncryptionError> {
        verify(password, hash).map_err(|_| EncryptionError::VerificationFailed)
    }

    /// Get the bcrypt cost factor (should be >= 12)
    pub fn get_bcrypt_cost(&self) -> u32 {
        self.bcrypt_cost
    }

    /// Set custom bcrypt cost (must be >= 12 per requirements)
    pub fn set_bcrypt_cost(&mut self, cost: u32) -> Result<(), EncryptionError> {
        if cost < 12 {
            return Err(EncryptionError::HashingFailed(
                "Bcrypt cost must be at least 12 per security requirements".to_string(), ));
        }
        self.bcrypt_cost = cost;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_service() -> EncryptionService {
        let key = EncryptionService::generate_key();
        EncryptionService::new(key).unwrap()
    }

    #[test]
    fn test_generate_key() {
        let key = EncryptionService::generate_key();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_invalid_key_length() {
        let short_key = vec![0u8; 16]; // Too short
        assert!(EncryptionService::new(short_key).is_err());

        let long_key = vec![0u8; 64]; // Too long
        assert!(EncryptionService::new(long_key).is_err());
    }

    #[test]
    fn test_encrypt_decrypt_token() {
        let service = create_test_service();
        let token = "AUTH_paystack_12345";

        // Encrypt
        let encrypted = service.encrypt_token(token).unwrap();
        assert_ne!(encrypted, token);
        assert!(!encrypted.contains("AUTH_paystack")); // Ensure token is encrypted

        // Decrypt
        let decrypted = service.decrypt_token(&encrypted).unwrap();
        assert_eq!(decrypted, token);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let service = create_test_service();
        let token = "AUTH_paystack_12345";

        // Encrypt same token twice — different ciphertexts due to random nonce
        let encrypted1 = service.encrypt_token(token).unwrap();
        let encrypted2 = service.encrypt_token(token).unwrap();
        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same plaintext
        assert_eq!(service.decrypt_token(&encrypted1).unwrap(), token);
        assert_eq!(service.decrypt_token(&encrypted2).unwrap(), token);
    }

    #[test]
    fn test_hash_password() {
        let service = create_test_service();
        let password = "SecurePassword123!";

        let hash = service.hash_password(password).unwrap();
        assert_ne!(hash, password);
        assert!(hash.starts_with("$2")); // Bcrypt hash format
    }

    #[test]
    fn test_verify_password() {
        let service = create_test_service();
        let password = "SecurePassword123!";

        let hash = service.hash_password(password).unwrap();

        // Correct password
        assert!(service.verify_password(password, &hash).unwrap());

        // Incorrect password
        assert!(!service.verify_password("WrongPassword", &hash).unwrap());
    }

    #[test]
    fn test_bcrypt_cost_minimum() {
        let mut service = create_test_service();

        // Default cost should be >= 12
        assert!(service.get_bcrypt_cost() >= 12);

        // Should reject cost < 12
        assert!(service.set_bcrypt_cost(10).is_err());

        // Should accept cost >= 12
        assert!(service.set_bcrypt_cost(12).is_ok());
        assert!(service.set_bcrypt_cost(14).is_ok());
    }

    #[test]
    fn test_decrypt_invalid_data() {
        let service = create_test_service();

        // Invalid base64
        assert!(service.decrypt_token("not-base64!@#").is_err());

        // Too short
        assert!(service.decrypt_token("YWJj").is_err()); // "abc" in base64

        // Valid base64 but wrong key
        let other_service = create_test_service();
        let encrypted = service.encrypt_token("test").unwrap();
        assert!(other_service.decrypt_token(&encrypted).is_err());
    }
}


#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Property 23: Password hashing security (bcrypt work factor >= 12)

    proptest! {
        #[test]
        fn property_24_token_encryption_roundtrip(
            token in "[A-Za-z0-9_-]{10,100}",
        ) {
            let service = EncryptionService::new(EncryptionService::generate_key()).unwrap(); // Encrypt
            let encrypted = service.encrypt_token(&token).unwrap(); // Property: Encrypted token should not contain plaintext
            prop_assert!(!encrypted.contains(&token),
                "Encrypted token should not contain plaintext");
            
            // Property: Encrypted token should be different from plaintext
            prop_assert_ne!(&encrypted, &token,
                "Encrypted token should differ from plaintext");

            // Property: Decryption should recover original token
            let decrypted = service.decrypt_token(&encrypted).unwrap(); prop_assert_eq!(&decrypted, &token,
                "Decrypted token should match original");
        }
    }

    proptest! {
        #[test]
        fn property_23_password_hashing_security(
            password in "[A-Za-z0-9!@#$%^&*]{8,50}",
        ) {
            let service = EncryptionService::new(EncryptionService::generate_key()).unwrap(); // Property: Bcrypt cost should be >= 12
            prop_assert!(service.get_bcrypt_cost() >= 12,
                "Bcrypt cost must be at least 12");

            // Hash password
            let hash = service.hash_password(&password).unwrap(); // Property: Hash should not contain plaintext password
            prop_assert!(!hash.contains(&password),
                "Hash should not contain plaintext password");

            // Property: Hash should start with bcrypt identifier
            prop_assert!(hash.starts_with("$2"),
                "Hash should use bcrypt format");

            // Property: Verification should succeed with correct password
            prop_assert!(service.verify_password(&password, &hash).unwrap(), "Should verify correct password");

            // Property: Verification should fail with incorrect password
            let wrong_password = format!("{}wrong", password);
            prop_assert!(!service.verify_password(&wrong_password, &hash).unwrap(), "Should reject incorrect password");
        }
    }

    proptest! {
        #[test]
        fn property_24_encryption_produces_unique_ciphertexts(
            token in "[A-Za-z0-9_-]{10,50}",
        ) {
            let service = EncryptionService::new(EncryptionService::generate_key()).unwrap(); // Encrypt same token multiple times
            let encrypted1 = service.encrypt_token(&token).unwrap(); let encrypted2 = service.encrypt_token(&token).unwrap(); // Property: Same plaintext should produce different ciphertexts (due to random nonce)
            prop_assert_ne!(&encrypted1, &encrypted2,
                "Same plaintext should produce different ciphertexts");

            // Property: Both should decrypt to same plaintext
            prop_assert_eq!(&service.decrypt_token(&encrypted1).unwrap(), &token);
            prop_assert_eq!(&service.decrypt_token(&encrypted2).unwrap(), &token);
        }
    }

    #[test]
    fn test_property_23_bcrypt_cost_enforcement() {
        let mut service = EncryptionService::new(EncryptionService::generate_key()).unwrap(); // Property: Should reject cost < 12
        assert!(service.set_bcrypt_cost(10).is_err());
        assert!(service.set_bcrypt_cost(11).is_err());

        // Property: Should accept cost >= 12
        assert!(service.set_bcrypt_cost(12).is_ok());
        assert!(service.set_bcrypt_cost(13).is_ok());
        assert!(service.set_bcrypt_cost(14).is_ok());
    }
}
