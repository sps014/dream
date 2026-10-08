/// Guest compilation policy, independent of debugger information and backend tuning.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompileProfile {
    #[default]
    Debug,
    Release,
}

impl CompileProfile {
    pub const fn from_release(release: bool) -> Self {
        if release { Self::Release } else { Self::Debug }
    }

    pub const fn is_debug(self) -> bool {
        matches!(self, Self::Debug)
    }
}
