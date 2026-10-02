//! Native host link artifacts, shared by the compiler and package manager.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostCapability {
    Core,
    Net,
    Gpu,
    WebView,
}

impl HostCapability {
    pub const ALL: [Self; 4] = [Self::Core, Self::Net, Self::Gpu, Self::WebView];

    pub const fn link_name(self) -> &'static str {
        match self {
            Self::Core => "dream_host_core",
            Self::Net => "dream_host_net",
            Self::Gpu => "dream_host_gpu",
            Self::WebView => "dream_host_webview",
        }
    }

    pub fn library_name(self) -> String {
        let name = self.link_name();
        if cfg!(windows) {
            format!("{name}.dll")
        } else if cfg!(target_os = "macos") {
            format!("lib{name}.dylib")
        } else {
            format!("lib{name}.so")
        }
    }

    pub fn import_library_name(self) -> String {
        format!("{}.dll.lib", self.link_name())
    }
}
