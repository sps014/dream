use crate::schema::{Definition, Kind};
use clap::{Arg, ArgAction, ColorChoice, Command, builder::PossibleValuesParser};

pub fn build(definition: &Definition) -> Command {
    let mut command = Command::new(definition.name.clone())
        .about(definition.description.clone())
        .color(ColorChoice::Never)
        .disable_help_subcommand(true)
        .subcommand_required(definition.require_subcommand)
        .subcommand_negates_reqs(true);
    if !definition.version.is_empty() {
        command = command
            .version(definition.version.clone())
            .propagate_version(true);
    }
    let mut position = 0;
    for definition in &definition.args {
        let mut argument = Arg::new(definition.name.clone())
            .help(definition.description.clone())
            .required(definition.required);
        if definition.kind == Kind::Positional {
            position += 1;
            argument = argument.index(position);
            if definition.multiple {
                argument = argument.num_args(1..).action(ArgAction::Append);
            }
        } else {
            argument = argument.long(definition.name.clone());
            if let Some(short) = definition.short.chars().next() {
                argument = argument.short(short);
            }
        }
        if definition.kind == Kind::Flag {
            argument = argument
                .action(ArgAction::SetTrue)
                .overrides_with(definition.name.clone());
        } else {
            if definition.kind != Kind::Positional {
                argument = argument.action(ArgAction::Append).num_args(1);
            }
            if definition.kind == Kind::Int {
                let choices = definition.choices.clone();
                argument = argument
                    .allow_negative_numbers(true)
                    .value_parser(move |text: &str| {
                        text.parse::<i32>()
                            .map_err(|_| "expected a decimal 32-bit integer".to_string())?;
                        if !choices.is_empty() && !choices.iter().any(|value| value == text) {
                            return Err(format!("expected one of: {}", choices.join(", ")));
                        }
                        Ok::<_, String>(text.to_owned())
                    });
            } else if !definition.choices.is_empty() {
                argument =
                    argument.value_parser(PossibleValuesParser::new(definition.choices.clone()));
            }
            if let Some(value) = &definition.default_value {
                argument = argument.default_value(value.clone());
            }
        }
        command = command.arg(argument);
    }
    for child in &definition.subcommands {
        command = command.subcommand(build(child));
    }
    command
}
