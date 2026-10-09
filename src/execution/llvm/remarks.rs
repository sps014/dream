use std::path::Path;
use std::process::Command;

pub(super) fn prepare(path: Option<&Path>) -> Result<(), std::io::Error> {
    if let Some(path) = path {
        std::fs::write(path, "")?;
    }
    Ok(())
}

pub(super) fn configure(command: &mut Command, path: Option<&Path>) {
    if let Some(path) = path {
        command
            .arg(format!("-pass-remarks-output={}", path.display()))
            .arg("-pass-remarks-filter=gvn|licm|loop-vectorize|slp-vectorizer|inline")
            .arg("-pass-remarks-format=yaml");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remarks_are_optional_and_paths_are_single_arguments() {
        let mut command = Command::new("opt");
        configure(&mut command, None);
        assert_eq!(command.get_args().count(), 0);
        configure(
            &mut command,
            Some(Path::new("output with spaces.remarks.yaml")),
        );
        assert_eq!(
            command.get_args().next().unwrap(),
            "-pass-remarks-output=output with spaces.remarks.yaml"
        );
        assert_eq!(command.get_args().count(), 3);
    }
}
