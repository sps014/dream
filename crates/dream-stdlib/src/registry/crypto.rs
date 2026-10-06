use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.crypto",
    deps: &["system.core", "system.primitives"],
    generators: &[],
    files: &[
        (
            "<std>/system/crypto/crypto_error.dream",
            include_str!("../system/crypto/crypto_error.dream"),
        ),
        (
            "<std>/system/crypto/sha256.dream",
            include_str!("../system/crypto/sha256.dream"),
        ),
        (
            "<std>/system/crypto/sha512.dream",
            include_str!("../system/crypto/sha512.dream"),
        ),
        (
            "<std>/system/crypto/hmac_sha256.dream",
            include_str!("../system/crypto/hmac_sha256.dream"),
        ),
        (
            "<std>/system/crypto/secure_random.dream",
            include_str!("../system/crypto/secure_random.dream"),
        ),
        (
            "<std>/system/crypto/aes_gcm_key.dream",
            include_str!("../system/crypto/aes_gcm_key.dream"),
        ),
        (
            "<std>/system/crypto/aes_gcm.dream",
            include_str!("../system/crypto/aes_gcm.dream"),
        ),
    ],
};
