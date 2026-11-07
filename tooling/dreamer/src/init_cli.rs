use clap::{Args, ValueEnum};
use dreamer::commands::init::{self, Scaffold};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum Template {
    /// A command-line program.
    #[default]
    Console,
    /// A GPU canvas app for the current desktop OS.
    Desktop,
    /// A shared GPU canvas project for Android and iOS.
    Mobile,
    /// A shared GPU canvas project for desktop, Android, and iOS.
    App,
    /// A browser program with an HTML launcher.
    Web,
    /// A Node program with a JavaScript launcher.
    Node,
    /// A reusable library.
    Lib,
    /// A project containing @test functions.
    Test,
    /// A workspace containing a console program and tests.
    Workspace,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum LibOutput {
    Staticlib,
    Cdylib,
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Project template. Omit to create a console program in the current directory.
    #[arg(value_enum, default_value = "console")]
    template: Template,
    /// Package name and new directory. Omit to initialize the current directory.
    name: Option<String>,
    /// Create the project here instead of ./<name>.
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Library output kind; only valid with the lib template (default: staticlib).
    #[arg(long, value_enum)]
    output: Option<LibOutput>,
}

impl InitArgs {
    pub fn run(self, cwd: &Path) -> anyhow::Result<()> {
        let (scaffold, runtime) = self.configuration()?;
        if let Some(name) = &self.name {
            dreamer::manifest::validate_package_name(name)?;
        }
        let dir = init::prepare_project_dir(cwd, self.name.as_deref(), self.dir.as_deref())?;
        init::run(&dir, self.name, runtime.map(str::to_owned), scaffold)
    }

    fn configuration(&self) -> anyhow::Result<(Scaffold, Option<&'static str>)> {
        if self.output.is_some() && self.template != Template::Lib {
            anyhow::bail!("--output is only valid with `dreamer init lib`");
        }
        Ok(match self.template {
            Template::Console => (Scaffold::Bin, None),
            Template::Desktop => (Scaffold::Canvas, Some("native")),
            Template::Mobile => (Scaffold::Canvas, Some("android,ios")),
            Template::App => (Scaffold::Canvas, Some("native,android,ios")),
            Template::Web => (Scaffold::Bin, Some("web")),
            Template::Node => (Scaffold::Bin, Some("node")),
            Template::Lib => (
                Scaffold::Lib(match self.output.unwrap_or(LibOutput::Staticlib) {
                    LibOutput::Staticlib => dream_abi::library::LibraryKind::Staticlib,
                    LibOutput::Cdylib => dream_abi::library::LibraryKind::Cdylib,
                }),
                None,
            ),
            Template::Test => (Scaffold::Test, None),
            Template::Workspace => (Scaffold::Workspace, None),
        })
    }
}
