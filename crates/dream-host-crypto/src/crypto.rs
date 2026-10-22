//! Operating-system entropy for `system.crypto`.

pub(crate) fn secure_random(len: i32) -> Vec<u8> {
    secure_random_from(len, getrandom::getrandom)
}

fn secure_random_from(
    len: i32,
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Vec<u8> {
    if !(0..=16 * 1024 * 1024).contains(&len) {
        return Vec::new();
    }
    let mut bytes = vec![0; len as usize];
    if fill(&mut bytes).is_err() {
        return Vec::new();
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entropy_failure_discards_partial_output() {
        let bytes = secure_random_from(16, |bytes| {
            bytes[0] = 42;
            Err(getrandom::Error::UNSUPPORTED)
        });
        assert!(bytes.is_empty());
        assert_eq!(
            secure_random_from(4, |bytes| {
                bytes.fill(7);
                Ok(())
            }),
            vec![7; 4]
        );
    }
}
