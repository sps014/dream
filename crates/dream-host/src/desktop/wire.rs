//! Dialog/clipboard codes and payload encodings shared by the real host and the stub build.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogKind {
    PickFile,
    PickFiles,
    PickFolder,
    PickFolders,
    SaveFile,
    Message,
}

impl DialogKind {
    pub(crate) fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::PickFile,
            1 => Self::PickFiles,
            2 => Self::PickFolder,
            3 => Self::PickFolders,
            4 => Self::SaveFile,
            5 => Self::Message,
            _ => return None,
        })
    }
}

/// Everything a dialog needs, decoded from the Dream call or the page's JSON request.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DialogRequest {
    pub title: String,
    pub text: String,
    pub directory: String,
    pub file_name: String,
    pub filters: Vec<(String, Vec<String>)>,
    /// 0 info, 1 warning, 2 error.
    pub level: i32,
    /// 0 ok, 1 ok/cancel, 2 yes/no, 3 yes/no/cancel.
    pub buttons: i32,
    /// WebView id whose window parents the dialog; `<= 0` for none.
    pub parent: i32,
}

// The stub host only ever reports `Cancelled`; the other variants come from the rfd host.
#[cfg_attr(not(feature = "webview"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Cancelled,
    Paths(Vec<String>),
    Button(&'static str),
}

/// One `name\text1,ext2` per line; blank names or extension lists are skipped.
pub(crate) fn parse_filters(encoded: &str) -> Vec<(String, Vec<String>)> {
    encoded
        .lines()
        .filter_map(|line| {
            let (name, exts) = line.split_once('\t')?;
            let exts: Vec<String> = exts
                .split(',')
                .map(|e| e.trim().trim_start_matches('.').to_string())
                .filter(|e| !e.is_empty())
                .collect();
            (!name.is_empty() && !exts.is_empty()).then(|| (name.to_string(), exts))
        })
        .collect()
}

/// `P` pending, `C` cancelled, `O\n` + one path (or the chosen button) per line.
pub(crate) fn encode_outcome(outcome: Option<&Outcome>) -> Vec<u8> {
    match outcome {
        None => b"P".to_vec(),
        Some(Outcome::Cancelled) => b"C".to_vec(),
        Some(Outcome::Paths(paths)) => {
            let mut out = b"O\n".to_vec();
            out.extend_from_slice(paths.join("\n").as_bytes());
            out
        }
        Some(Outcome::Button(name)) => {
            let mut out = b"O\n".to_vec();
            out.extend_from_slice(name.as_bytes());
            out
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipKind {
    Text,
    Html,
    Image,
    Files,
    Data,
    Formats,
}

impl ClipKind {
    pub(crate) fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::Text,
            1 => Self::Html,
            2 => Self::Image,
            3 => Self::Files,
            4 => Self::Data,
            5 => Self::Formats,
            _ => return None,
        })
    }
}

/// Clipboard reads: `1` + payload when present, `0` when missing or unreadable.
pub(crate) fn encode_clip(value: Option<Vec<u8>>) -> Vec<u8> {
    match value {
        Some(mut bytes) => {
            bytes.insert(0, b'1');
            bytes
        }
        None => b"0".to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_parse_names_and_extensions() {
        let parsed = parse_filters("Audio\tmp3, .flac\n\tbad\nEmpty\t\nImages\tpng");
        assert_eq!(
            parsed,
            vec![
                ("Audio".to_string(), vec!["mp3".to_string(), "flac".to_string()]),
                ("Images".to_string(), vec!["png".to_string()]),
            ]
        );
    }

    #[test]
    fn outcome_encoding() {
        assert_eq!(encode_outcome(None), b"P");
        assert_eq!(encode_outcome(Some(&Outcome::Cancelled)), b"C");
        assert_eq!(
            encode_outcome(Some(&Outcome::Paths(vec!["/a".into(), "/b c".into()]))),
            b"O\n/a\n/b c"
        );
        assert_eq!(encode_outcome(Some(&Outcome::Button("yes"))), b"O\nyes");
    }

    #[test]
    fn codes_round_trip() {
        assert_eq!(DialogKind::from_code(0), Some(DialogKind::PickFile));
        assert_eq!(DialogKind::from_code(5), Some(DialogKind::Message));
        assert_eq!(DialogKind::from_code(9), None);
        assert_eq!(ClipKind::from_code(2), Some(ClipKind::Image));
        assert_eq!(ClipKind::from_code(6), None);
    }

    #[test]
    fn clip_encoding() {
        assert_eq!(encode_clip(None), b"0");
        assert_eq!(encode_clip(Some(b"hi".to_vec())), b"1hi");
    }
}
