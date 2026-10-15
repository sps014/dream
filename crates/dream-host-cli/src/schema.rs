use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub command: Definition,
    pub args: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub require_subcommand: bool,
    #[serde(default)]
    pub args: Vec<Argument>,
    #[serde(default)]
    pub subcommands: Vec<Definition>,
}
#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    String,
    Int,
    Flag,
    Positional,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Int => "int",
            Self::Flag => "flag",
            Self::Positional => "positional",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Argument {
    pub name: String,
    pub kind: Kind,
    #[serde(default)]
    pub short: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub choices: Vec<String>,
}
fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}
impl Definition {
    pub fn validate(&self, depth: usize, inherited_version: bool) -> Result<(), String> {
        if depth > 16 || self.args.len() > 256 || self.subcommands.len() > 64 {
            return Err("command exceeds CLI definition limits".into());
        }
        if !valid_name(&self.name) {
            return Err("command names must begin with an ASCII letter and contain letters, digits, '_' or '-'".into());
        }
        let version = inherited_version || !self.version.is_empty();
        let mut names = BTreeSet::new();
        let mut shorts = BTreeSet::new();
        let mut optional_positional = false;
        let mut multiple_positional = false;
        for argument in &self.args {
            if !valid_name(&argument.name)
                || !names.insert(&argument.name)
                || argument.name == "help"
                || argument.name == "version"
            {
                return Err(format!(
                    "invalid, duplicate, or reserved argument name: {}",
                    argument.name
                ));
            }
            if !argument.short.is_empty() {
                if argument.short.len() != 1
                    || !argument.short.as_bytes()[0].is_ascii_alphabetic()
                    || argument.short == "h"
                    || (version && argument.short == "V")
                    || !shorts.insert(&argument.short)
                {
                    return Err(format!(
                        "invalid, duplicate, or reserved short option: {}",
                        argument.short
                    ));
                }
                if argument.kind == Kind::Positional {
                    return Err("positional arguments cannot have short options".into());
                }
            }
            if argument.kind == Kind::Positional {
                if multiple_positional || (optional_positional && argument.required) {
                    return Err("required positionals must precede optional positionals; only the last can be multiple".into());
                }
                optional_positional |= !argument.required;
                multiple_positional = argument.multiple;
            } else if argument.multiple {
                return Err("only positional arguments use the multiple setting; repeated options are collected automatically".into());
            }
            if argument.kind == Kind::Flag
                && (argument.required
                    || !argument.choices.is_empty()
                    || argument.default_value.is_some())
            {
                return Err("flags cannot have required values, defaults, or choices".into());
            }
            if argument.kind == Kind::Int {
                for value in argument.choices.iter().chain(argument.default_value.iter()) {
                    if value.parse::<i32>().is_err() {
                        return Err(format!(
                            "invalid integer default or choice for {}",
                            argument.name
                        ));
                    }
                }
            }
            if let Some(value) = &argument.default_value
                && !argument.choices.is_empty()
                && !argument.choices.contains(value)
            {
                return Err(format!(
                    "default value for {} must be one of its choices",
                    argument.name
                ));
            }
        }
        let mut children = BTreeSet::new();
        for command in &self.subcommands {
            if !children.insert(&command.name) {
                return Err(format!("duplicate subcommand: {}", command.name));
            }
            command.validate(depth + 1, version)?;
        }
        if self.require_subcommand && self.subcommands.is_empty() {
            return Err("a required subcommand needs at least one definition".into());
        }
        Ok(())
    }
}
