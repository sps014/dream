use crate::{
    command,
    schema::{Definition, Kind, Request},
};
use clap::{ArgMatches, error::ErrorKind, parser::ValueSource};
use serde_json::{Value, json};

pub fn failure(code: &str, message: &str) -> String {
    json!({"kind":"error", "code":code, "message":message}).to_string()
}
fn matches(definition: &Definition, parsed: &ArgMatches) -> Value {
    let mut values = serde_json::Map::new();
    for argument in &definition.args {
        let provided = parsed.value_source(&argument.name) == Some(ValueSource::CommandLine);
        let values_list: Vec<String> = if argument.kind == Kind::Flag {
            vec![parsed.get_flag(&argument.name).to_string()]
        } else {
            parsed
                .get_many::<String>(&argument.name)
                .map(|v| v.cloned().collect())
                .unwrap_or_default()
        };
        values.insert(
            argument.name.clone(),
            json!({"kind":argument.kind.name(), "values": values_list, "provided":provided}),
        );
    }
    let child = parsed.subcommand().and_then(|(name, parsed)| {
        definition
            .subcommands
            .iter()
            .find(|definition| definition.name == name)
            .map(|definition| matches(definition, parsed))
    });
    json!({"kind":"values", "name":definition.name, "values":values, "subcommand":child})
}
pub fn parse(request: &str) -> String {
    if request.len() > 1024 * 1024 {
        return failure("EINVAL", "CLI request exceeds 1 MiB");
    }
    let request: Request = match serde_json::from_str(request) {
        Ok(request) => request,
        Err(error) => return failure("EINVAL", &format!("invalid CLI definition: {error}")),
    };
    if request.args.len() > 16384 {
        return failure("EINVAL", "too many CLI arguments");
    }
    if let Err(error) = request.command.validate(0, false) {
        return failure("EINVAL", &error);
    }
    let command = command::build(&request.command).no_binary_name(true);
    match command.try_get_matches_from(request.args) {
        Ok(parsed) => matches(&request.command, &parsed).to_string(),
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp => {
                json!({"kind":"help", "help_text":error.to_string()}).to_string()
            }
            ErrorKind::DisplayVersion => {
                json!({"kind":"version", "version_text":error.to_string()}).to_string()
            }
            _ => failure("EARGS", &error.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_definitions_are_errors() {
        for input in [
            "",
            "null",
            "{}",
            "{\"command\":{\"name\":\"\"},\"args\":[]}",
        ] {
            let result: Value = serde_json::from_str(&parse(input)).unwrap();
            assert_eq!(result["code"], "EINVAL");
        }
    }
}
