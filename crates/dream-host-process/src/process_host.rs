//! Validated length-prefixed UTF-8 argument decoding.

pub(crate) fn split_args(mut wire: &[u8]) -> std::io::Result<Vec<String>> {
    fn invalid() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid process argument encoding",
        )
    }
    fn length(wire: &mut &[u8]) -> std::io::Result<usize> {
        let (head, tail) = wire.split_at_checked(4).ok_or_else(invalid)?;
        let value = u32::from_le_bytes(head.try_into().map_err(|_| invalid())?) as usize;
        *wire = tail;
        Ok(value)
    }
    let count = length(&mut wire)?;
    if count > wire.len() / 4 {
        return Err(invalid());
    }
    let mut args = Vec::with_capacity(count);
    for _ in 0..count {
        let size = length(&mut wire)?;
        let (argument, tail) = wire.split_at_checked(size).ok_or_else(invalid)?;
        if argument.contains(&0) {
            return Err(invalid());
        }
        args.push(
            std::str::from_utf8(argument)
                .map_err(|_| invalid())?
                .to_owned(),
        );
        wire = tail;
    }
    if !wire.is_empty() {
        return Err(invalid());
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::split_args;

    fn encode(args: &[&str]) -> Vec<u8> {
        let mut wire = (args.len() as u32).to_le_bytes().to_vec();
        for argument in args {
            wire.extend_from_slice(&(argument.len() as u32).to_le_bytes());
            wire.extend_from_slice(argument.as_bytes());
        }
        wire
    }

    #[test]
    fn argument_boundaries_are_preserved() {
        let args = [
            "",
            "two words",
            "first\nsecond",
            "こんにちは",
            "\u{feff}leading",
            "\"quoted\"\\tail",
        ];
        assert_eq!(split_args(&encode(&args)).unwrap(), args);
        assert!(split_args(&encode(&[])).unwrap().is_empty());
        assert_eq!(split_args(&encode(&[""])).unwrap(), [""]);
    }

    #[test]
    fn malformed_arguments_are_rejected() {
        let valid = encode(&["hello"]);
        for size in 0..valid.len() {
            assert!(split_args(&valid[..size]).is_err());
        }
        let mut trailing = valid;
        trailing.push(0);
        assert!(split_args(&trailing).is_err());
        assert!(split_args(&[255; 4]).is_err());
        assert!(split_args(&encode(&["embedded\0nul"])).is_err());
        assert!(split_args(&[1, 0, 0, 0, 1, 0, 0, 0, 255]).is_err());
    }
}
