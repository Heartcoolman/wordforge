use crate::store::StoreError;

pub fn validate_id(id: &str) -> Result<&str, StoreError> {
    if id.is_empty() {
        return Err(StoreError::Validation("ID 不能为空".to_string()));
    }
    if id.contains(':') {
        return Err(StoreError::Validation("ID 不能包含冒号分隔符".to_string()));
    }
    Ok(id)
}

pub fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

pub fn source_url_hash_prefix(url: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(url.as_bytes());
    hex::encode(&hash[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_id_rejects_empty() {
        assert!(validate_id("").is_err());
    }

    #[test]
    fn validate_id_rejects_colon() {
        assert!(validate_id("a:b").is_err());
    }

    #[test]
    fn validate_id_accepts_valid() {
        assert!(validate_id("abc-123").is_ok());
    }

    #[test]
    fn canonical_pair_orders_correctly() {
        let (a, b) = canonical_pair("z", "a");
        assert_eq!(a, "a");
        assert_eq!(b, "z");
    }
}
