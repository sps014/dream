//! Native capability export inventory. Runtime C and foreign C imports are not host services.

use super::HostCapability;

pub(super) fn fields(capability: HostCapability) -> &'static [&'static str] {
    match capability {
        HostCapability::Core => &[],
        HostCapability::Unicode => &[
            "unicodeNormalize",
            "unicodeToLower",
            "unicodeToUpper",
            "unicodeGraphemes",
        ],
        HostCapability::Crypto => &[
            "cryptoAesGcmEncrypt",
            "cryptoAesGcmDecrypt",
            "cryptoSha256",
            "cryptoSha512",
            "cryptoHmacSha256",
            "cryptoSecureRandomBytes",
            "cryptoSecureRandomFill",
        ],
        HostCapability::Process => &[
            "processRun",
            "processSpawn",
            "processWriteStdin",
            "processReadStream",
            "processReadStreamLine",
            "processWait",
            "processKill",
        ],
        HostCapability::Timezone => &["dateZoneOffsetMinutes", "dateLocalZoneName"],
    }
}
