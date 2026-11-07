use anyhow::{Result, bail};

pub(crate) fn c_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(crate) fn java_package(value: &str) -> Result<()> {
    let keywords = [
        "class",
        "package",
        "public",
        "private",
        "protected",
        "static",
        "native",
        "void",
        "int",
        "long",
        "float",
        "double",
        "boolean",
        "char",
        "byte",
        "short",
        "return",
        "if",
        "else",
        "switch",
        "case",
        "default",
        "while",
        "for",
        "do",
        "break",
        "continue",
        "new",
        "this",
        "super",
        "extends",
        "implements",
        "interface",
        "enum",
        "abstract",
        "final",
        "try",
        "catch",
        "finally",
        "throw",
        "throws",
        "synchronized",
        "volatile",
        "transient",
        "instanceof",
        "import",
        "assert",
        "const",
        "goto",
        "strictfp",
        "true",
        "false",
        "null",
        "_",
    ];
    if value
        .split('.')
        .any(|v| !c_identifier(v) || keywords.contains(&v))
    {
        bail!("invalid Java package {value}");
    }
    Ok(())
}
