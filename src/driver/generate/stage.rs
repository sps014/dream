/// Which generators a compile runs. A generator executable is itself compiled with a narrower
/// stage so generator builds cannot recurse: user generator programs still get stdlib derives
/// (`@json` inside a generator works), and stdlib generator programs get none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum GeneratorStage {
    #[default]
    All,
    StdlibOnly,
    None,
}

impl GeneratorStage {
    /// The stage used to compile the executable of a generator declared at this stage.
    pub fn for_generator_build(is_std: bool) -> Self {
        if is_std {
            GeneratorStage::None
        } else {
            GeneratorStage::StdlibOnly
        }
    }
}
