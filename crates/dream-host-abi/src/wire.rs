//! Little-endian records exchanged with the guest's `WireWriter` / `WireReader`: fixed-width
//! integers, and strings or byte runs as a `u32` length followed by the bytes.

/// Format version leading every operation header and request record; a mismatch means the
/// guest and host were built from different sources.
pub const VERSION: u8 = 1;

/// The fixed prefix of every guest operation: `[version][action][id][timeout_ms]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub action: u8,
    pub id: i32,
    /// Zero means no deadline.
    pub timeout_ms: u32,
}

impl Header {
    /// Encoded size; the action's arguments start at this offset.
    pub const LEN: usize = 10;
}

/// Decodes a guest record; every read fails once the record is short or malformed.
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// Why a guest record could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], Malformed> {
        let end = self.at.checked_add(count).ok_or(Malformed)?;
        let slice = self.bytes.get(self.at..end).ok_or(Malformed)?;
        self.at = end;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8, Malformed> {
        Ok(self.take(1)?[0])
    }

    pub fn i32(&mut self) -> Result<i32, Malformed> {
        let bytes = self.take(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn i64(&mut self) -> Result<i64, Malformed> {
        let low = self.i32()? as u32 as u64;
        let high = self.i32()? as u32 as u64;
        Ok((low | (high << 32)) as i64)
    }

    pub fn bytes(&mut self) -> Result<&'a [u8], Malformed> {
        let length = usize::try_from(self.i32()?).map_err(|_| Malformed)?;
        self.take(length)
    }

    pub fn text(&mut self) -> Result<&'a str, Malformed> {
        std::str::from_utf8(self.bytes()?).map_err(|_| Malformed)
    }

    pub fn header(&mut self) -> Result<Header, Malformed> {
        if self.u8()? != VERSION {
            return Err(Malformed);
        }
        let action = self.u8()?;
        let id = self.i32()?;
        let timeout_ms = u32::try_from(self.i32()?).map_err(|_| Malformed)?;
        Ok(Header {
            action,
            id,
            timeout_ms,
        })
    }

    /// Fails unless every byte has been read, so trailing garbage is rejected.
    pub fn finish(&self) -> Result<(), Malformed> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(Malformed)
        }
    }
}

/// Encodes a record for the guest.
#[derive(Default)]
pub struct Writer(Vec<u8>);

impl Writer {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.0.push(value);
        self
    }

    pub fn i32(&mut self, value: i32) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn i64(&mut self, value: i64) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Writes `bytes` with its length; runs past `i32::MAX` bytes are truncated, which no
    /// guest array can hold anyway.
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        let length = bytes.len().min(i32::MAX as usize);
        self.i32(length as i32);
        self.0.extend_from_slice(&bytes[..length]);
        self
    }

    pub fn text(&mut self, value: &str) -> &mut Self {
        self.bytes(value.as_bytes())
    }

    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_fields() {
        let bytes = Writer::new()
            .u8(7)
            .i32(-2)
            .text("héllo")
            .bytes(&[1, 2])
            .finish();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.u8(), Ok(7));
        assert_eq!(reader.i32(), Ok(-2));
        assert_eq!(reader.text(), Ok("héllo"));
        assert_eq!(reader.bytes(), Ok(&[1u8, 2][..]));
        assert_eq!(reader.finish(), Ok(()));
    }

    #[test]
    fn rejects_short_and_trailing_records() {
        assert_eq!(Reader::new(&[5, 0, 0, 0, b'a']).text(), Err(Malformed));
        assert_eq!(Reader::new(&[255, 255, 255, 255]).bytes(), Err(Malformed));
        let mut reader = Reader::new(&[1, 2]);
        reader.u8().unwrap();
        assert_eq!(reader.finish(), Err(Malformed));
        assert_eq!(Reader::new(&[0, 0, 0, 0xc3]).i64(), Err(Malformed));
    }

    #[test]
    fn reads_versioned_headers() {
        let bytes = Writer::new().u8(VERSION).u8(3).i32(9).i32(250).finish();
        let header = Reader::new(&bytes).header();
        assert_eq!(
            header,
            Ok(Header {
                action: 3,
                id: 9,
                timeout_ms: 250
            })
        );
        let stale = Writer::new().u8(VERSION + 1).u8(3).i32(9).i32(0).finish();
        assert_eq!(Reader::new(&stale).header(), Err(Malformed));
        let negative = Writer::new().u8(VERSION).u8(3).i32(9).i32(-1).finish();
        assert_eq!(Reader::new(&negative).header(), Err(Malformed));
    }
}
