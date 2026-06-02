//! v1.1-P0.6：资源包 Ed25519 签名链路。
//!
//! 与 `services::updater` 的 minisign 验签**刻意解耦**：
//!   - minisign 用于二进制自更新（有自己的元数据信封格式）
//!   - 资源包对接文档 §7.1 明确要求 raw Ed25519（`signatureAlgorithm: "ed25519"`，
//!     64 字节 base64），客户端 SDK 直接 verify(payloadBytes, signature, publicKey)
//!
//! 密钥管理：
//!   - 首次启动若 `<dir>/ed25519_resource_pack.{key,pub}` 不存在则生成
//!   - 私钥权限 0600（Unix），公钥 0644
//!   - 公钥通过 `GET /api/resource-packs/public-key` 端点匿名下发（便于客户端
//!     verify SDK 自检；正式分发仍由 iOS v1.1 发版同时硬编码）
//!
//! 签名格式：`base64::STANDARD.encode(signature_bytes)`，长度 88 字符（含 padding）。

use base64::Engine as _;
use ed25519_dalek::{
    Signer, SigningKey, Verifier, VerifyingKey, SECRET_KEY_LENGTH, SIGNATURE_LENGTH,
};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ResourcePackSigningError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid private key length: expected {SECRET_KEY_LENGTH}, got {0}")]
    InvalidPrivateKeyLength(usize),
    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
    #[error("base64 decode: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("signature verification failed")]
    VerificationFailed,
}

/// 持有签名/验签密钥对。私钥仅在 admin 上传时使用，公钥每次都用。
pub struct ResourcePackSigner {
    signing_key: SigningKey,
    public_key_b64: String,
}

impl ResourcePackSigner {
    /// 从已存在的私钥文件加载，或不存在则生成新对并写盘。
    ///
    /// `dir` 内会创建：
    ///   - `ed25519_resource_pack.key`：32 字节原始私钥，权限 0600
    ///   - `ed25519_resource_pack.pub`：32 字节原始公钥，权限 0644
    pub fn load_or_generate(dir: impl AsRef<Path>) -> Result<Self, ResourcePackSigningError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let key_path = dir.join("ed25519_resource_pack.key");
        let pub_path = dir.join("ed25519_resource_pack.pub");

        let signing_key = if key_path.exists() {
            load_signing_key(&key_path)?
        } else {
            let key = generate_signing_key();
            write_secret(&key_path, &key.to_bytes())?;
            write_public(&pub_path, &key.verifying_key().to_bytes())?;
            key
        };

        // pub 文件如果丢失，从私钥重新派生写回（容忍 admin 误删公钥）
        if !pub_path.exists() {
            write_public(&pub_path, &signing_key.verifying_key().to_bytes())?;
        }

        let public_key_b64 = base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().to_bytes());

        Ok(Self {
            signing_key,
            public_key_b64,
        })
    }

    /// 对原始字节签名，返回 base64 字符串（88 字符含 padding）。
    pub fn sign_base64(&self, payload: &[u8]) -> String {
        let sig = self.signing_key.sign(payload);
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes())
    }

    /// 公钥 base64（44 字符含 padding）。匿名端点 `GET /api/resource-packs/public-key` 返回此值。
    pub fn public_key_base64(&self) -> &str {
        &self.public_key_b64
    }
}

/// 仅用公钥验签，不需要持有私钥。客户端集成测试 / 后端 roundtrip 校验用。
pub fn verify_base64(
    payload: &[u8],
    signature_b64: &str,
    public_key_b64: &str,
) -> Result<(), ResourcePackSigningError> {
    let pk_bytes = base64::engine::general_purpose::STANDARD.decode(public_key_b64)?;
    let pk_array: [u8; 32] = pk_bytes.as_slice().try_into().map_err(|_| {
        ResourcePackSigningError::InvalidPublicKey(format!("len={}", pk_bytes.len()))
    })?;
    let verifying_key = VerifyingKey::from_bytes(&pk_array)
        .map_err(|e| ResourcePackSigningError::InvalidPublicKey(e.to_string()))?;

    let sig_bytes = base64::engine::general_purpose::STANDARD.decode(signature_b64)?;
    let sig_array: [u8; SIGNATURE_LENGTH] = sig_bytes.as_slice().try_into().map_err(|_| {
        ResourcePackSigningError::InvalidSignature(format!("len={}", sig_bytes.len()))
    })?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

    verifying_key
        .verify(payload, &signature)
        .map_err(|_| ResourcePackSigningError::VerificationFailed)
}

fn generate_signing_key() -> SigningKey {
    use rand::RngCore;
    let mut seed = [0u8; SECRET_KEY_LENGTH];
    rand::rngs::OsRng.fill_bytes(&mut seed);
    SigningKey::from_bytes(&seed)
}

fn load_signing_key(path: &PathBuf) -> Result<SigningKey, ResourcePackSigningError> {
    let bytes = std::fs::read(path)?;
    let arr: [u8; SECRET_KEY_LENGTH] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| ResourcePackSigningError::InvalidPrivateKeyLength(bytes.len()))?;
    Ok(SigningKey::from_bytes(&arr))
}

fn write_secret(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn write_public(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generate_persists_and_reload_matches() {
        let dir = TempDir::new().unwrap();
        let s1 = ResourcePackSigner::load_or_generate(dir.path()).unwrap();
        let pk1 = s1.public_key_base64().to_string();
        drop(s1);

        let s2 = ResourcePackSigner::load_or_generate(dir.path()).unwrap();
        assert_eq!(s2.public_key_base64(), pk1, "重启后必须返回同一公钥");
    }

    #[test]
    fn sign_verify_roundtrip() {
        let dir = TempDir::new().unwrap();
        let signer = ResourcePackSigner::load_or_generate(dir.path()).unwrap();
        let payload = b"{\"wordbooks\":[{\"id\":\"ielts-7000\"}]}";
        let sig = signer.sign_base64(payload);
        // sig 应是 base64 88 字符（含 padding）
        assert_eq!(sig.len(), 88, "Ed25519 base64 签名长度应为 88");
        verify_base64(payload, &sig, signer.public_key_base64()).expect("roundtrip 必须验证通过");
    }

    #[test]
    fn tampered_payload_fails() {
        let dir = TempDir::new().unwrap();
        let signer = ResourcePackSigner::load_or_generate(dir.path()).unwrap();
        let payload = b"original payload";
        let sig = signer.sign_base64(payload);

        let tampered = b"tampered payload";
        let err = verify_base64(tampered, &sig, signer.public_key_base64()).unwrap_err();
        assert!(matches!(err, ResourcePackSigningError::VerificationFailed));
    }

    #[test]
    fn wrong_pubkey_fails() {
        let d1 = TempDir::new().unwrap();
        let d2 = TempDir::new().unwrap();
        let signer = ResourcePackSigner::load_or_generate(d1.path()).unwrap();
        let other = ResourcePackSigner::load_or_generate(d2.path()).unwrap();
        let payload = b"x";
        let sig = signer.sign_base64(payload);
        let err = verify_base64(payload, &sig, other.public_key_base64()).unwrap_err();
        assert!(matches!(err, ResourcePackSigningError::VerificationFailed));
    }

    #[test]
    fn malformed_signature_rejected() {
        let dir = TempDir::new().unwrap();
        let signer = ResourcePackSigner::load_or_generate(dir.path()).unwrap();
        let err = verify_base64(b"x", "not-base64!!", signer.public_key_base64()).unwrap_err();
        assert!(matches!(err, ResourcePackSigningError::Base64(_)));

        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 10]);
        let err = verify_base64(b"x", &short, signer.public_key_base64()).unwrap_err();
        assert!(matches!(err, ResourcePackSigningError::InvalidSignature(_)));
    }

    #[cfg(unix)]
    #[test]
    fn private_key_has_0600_perms() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let _ = ResourcePackSigner::load_or_generate(dir.path()).unwrap();
        let key_path = dir.path().join("ed25519_resource_pack.key");
        let mode = std::fs::metadata(&key_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "私钥文件必须 0600");
    }
}
