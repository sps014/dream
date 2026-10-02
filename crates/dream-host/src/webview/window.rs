//! WebView window properties and actions (title, size, position, state, devtools, icon).
//!
//! Numeric properties share one op-code table between `webviewGetWindow` and `webviewWindowOp`
//! so the Dream getters and setters stay symmetrical. Sizes and positions are logical pixels.

use std::convert::TryFrom;

use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::{Fullscreen, Theme, Window, WindowLevel};

use super::{full_window_bounds, pump, with_entry_mut, WebViewEntry};
use crate::app_icon;

#[derive(Debug, Default, Clone)]
pub(super) struct Chrome {
    pub min_size: (u32, u32),
    pub max_size: (u32, u32),
    pub always_on_top: bool,
    /// Last `devtools_open` request: WebKit opens the inspector asynchronously and only macOS can
    /// report its visibility at all.
    pub devtools: bool,
    pub icon_path: String,
    pub page_dialogs: bool,
    /// Set once Dream registers `on_close_requested`: close clicks become events it answers.
    pub intercept_close: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Prop {
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    X,
    Y,
    Resizable,
    Fullscreen,
    Maximized,
    Minimized,
    AlwaysOnTop,
    Visible,
    DevtoolsOpen,
    PageDialogs,
    Focused,
    ScaleMilli,
    DarkMode,
    Resize,
    Center,
    Focus,
    Restore,
    InterceptClose,
    /// `on_close_requested` said yes: the next tick reports the window closed.
    Close,
}

impl Prop {
    pub(crate) fn from_code(code: i32) -> Option<Self> {
        use Prop::*;
        const TABLE: [Prop; 25] = [
            Width, Height, MinWidth, MinHeight, MaxWidth, MaxHeight, X, Y, Resizable, Fullscreen,
            Maximized, Minimized, AlwaysOnTop, Visible, DevtoolsOpen, PageDialogs, Focused,
            ScaleMilli, DarkMode, Resize, Center, Focus, Restore, InterceptClose, Close,
        ];
        usize::try_from(code).ok().and_then(|i| TABLE.get(i).copied())
    }
}

fn logical_size(window: &Window) -> LogicalSize<f64> {
    window.inner_size().to_logical(window.scale_factor())
}

fn logical_position(window: &Window) -> LogicalPosition<f64> {
    window
        .outer_position()
        .map(|p| p.to_logical(window.scale_factor()))
        .unwrap_or(LogicalPosition::new(0.0, 0.0))
}

fn limit(size: (u32, u32)) -> Option<LogicalSize<f64>> {
    (size.0 > 0 || size.1 > 0).then(|| {
        let w = if size.0 > 0 { size.0 as f64 } else { f64::MAX };
        let h = if size.1 > 0 { size.1 as f64 } else { f64::MAX };
        LogicalSize::new(w, h)
    })
}

fn min_limit(size: (u32, u32)) -> Option<LogicalSize<f64>> {
    (size.0 > 0 || size.1 > 0).then(|| LogicalSize::new(size.0 as f64, size.1 as f64))
}

fn read(e: &WebViewEntry, prop: Prop) -> i32 {
    let w = &e.window;
    let flag = |b: bool| i32::from(b);
    match prop {
        Prop::Width => logical_size(w).width.round() as i32,
        Prop::Height => logical_size(w).height.round() as i32,
        Prop::MinWidth => e.chrome.min_size.0 as i32,
        Prop::MinHeight => e.chrome.min_size.1 as i32,
        Prop::MaxWidth => e.chrome.max_size.0 as i32,
        Prop::MaxHeight => e.chrome.max_size.1 as i32,
        Prop::X => logical_position(w).x.round() as i32,
        Prop::Y => logical_position(w).y.round() as i32,
        Prop::Resizable => flag(w.is_resizable()),
        Prop::Fullscreen => flag(w.fullscreen().is_some()),
        Prop::Maximized => flag(w.is_maximized()),
        Prop::Minimized => flag(w.is_minimized().unwrap_or(false)),
        Prop::AlwaysOnTop => flag(e.chrome.always_on_top),
        Prop::Visible => flag(w.is_visible().unwrap_or(true)),
        Prop::DevtoolsOpen => flag(e.chrome.devtools || e.webview.is_devtools_open()),
        Prop::PageDialogs => flag(e.chrome.page_dialogs),
        Prop::Focused => flag(w.has_focus()),
        Prop::ScaleMilli => (w.scale_factor() * 1000.0).round() as i32,
        Prop::DarkMode => flag(w.theme() == Some(Theme::Dark)),
        Prop::InterceptClose => flag(e.chrome.intercept_close),
        Prop::Resize | Prop::Center | Prop::Focus | Prop::Restore | Prop::Close => 0,
    }
}

fn center(w: &Window) {
    let Some(monitor) = w.current_monitor() else {
        return;
    };
    let area = monitor.size();
    let origin = monitor.position();
    let outer = w.outer_size();
    let x = origin.x + (area.width as i32 - outer.width as i32) / 2;
    let y = origin.y + (area.height as i32 - outer.height as i32) / 2;
    w.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
}

fn write(e: &mut WebViewEntry, prop: Prop, a: i32, b: i32) {
    let on = a != 0;
    let dim = |v: i32| v.max(0) as u32;
    let size = logical_size(&e.window);
    let w = &e.window;
    match prop {
        Prop::Width => {
            let _ = w.request_inner_size(LogicalSize::new(dim(a).max(1) as f64, size.height));
        }
        Prop::Height => {
            let _ = w.request_inner_size(LogicalSize::new(size.width, dim(a).max(1) as f64));
        }
        Prop::Resize => {
            let _ = w.request_inner_size(LogicalSize::new(dim(a).max(1) as f64, dim(b).max(1) as f64));
        }
        Prop::MinWidth | Prop::MinHeight => {
            if prop == Prop::MinWidth {
                e.chrome.min_size.0 = dim(a);
            } else {
                e.chrome.min_size.1 = dim(a);
            }
            w.set_min_inner_size(min_limit(e.chrome.min_size));
        }
        Prop::MaxWidth | Prop::MaxHeight => {
            if prop == Prop::MaxWidth {
                e.chrome.max_size.0 = dim(a);
            } else {
                e.chrome.max_size.1 = dim(a);
            }
            w.set_max_inner_size(limit(e.chrome.max_size));
        }
        Prop::X | Prop::Y => {
            let pos = logical_position(w);
            let next = if prop == Prop::X {
                LogicalPosition::new(a as f64, pos.y)
            } else {
                LogicalPosition::new(pos.x, a as f64)
            };
            w.set_outer_position(next);
        }
        Prop::Resizable => w.set_resizable(on),
        Prop::Fullscreen => w.set_fullscreen(on.then_some(Fullscreen::Borderless(None))),
        Prop::Maximized => w.set_maximized(on),
        Prop::Minimized => w.set_minimized(on),
        Prop::AlwaysOnTop => {
            e.chrome.always_on_top = on;
            w.set_window_level(if on {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            });
        }
        Prop::Visible => w.set_visible(on),
        Prop::DevtoolsOpen => {
            e.chrome.devtools = on;
            if on {
                e.webview.open_devtools();
            } else {
                e.webview.close_devtools();
            }
        }
        Prop::PageDialogs => e.chrome.page_dialogs = on,
        Prop::Center => center(w),
        Prop::Focus => w.focus_window(),
        Prop::Restore => {
            w.set_fullscreen(None);
            w.set_minimized(false);
            w.set_maximized(false);
        }
        Prop::InterceptClose => e.chrome.intercept_close = on,
        Prop::Close => e.close_requested = true,
        Prop::Focused | Prop::ScaleMilli | Prop::DarkMode => {}
    }
    let _ = e.webview.set_bounds(full_window_bounds(&e.window));
}

pub(crate) fn get_window(id: i32, prop: i32) -> i32 {
    pump();
    let Some(prop) = Prop::from_code(prop) else {
        return 0;
    };
    with_entry_mut(id, |e| read(e, prop)).unwrap_or(0)
}

/// `0` when applied, `1` for a closed view or unknown op.
pub(crate) fn window_op(id: i32, prop: i32, a: i32, b: i32) -> i32 {
    let Some(prop) = Prop::from_code(prop) else {
        return 1;
    };
    let applied = with_entry_mut(id, |e| write(e, prop, a, b)).is_some();
    pump();
    i32::from(!applied)
}

/// String props: `0` title, `1` icon path.
pub(crate) fn get_string(id: i32, prop: i32) -> String {
    with_entry_mut(id, |e| match prop {
        0 => e.window.title(),
        1 => e.chrome.icon_path.clone(),
        _ => String::new(),
    })
    .unwrap_or_default()
}

pub(crate) fn set_string(id: i32, prop: i32, value: &str) -> i32 {
    let applied = with_entry_mut(id, |e| match prop {
        0 => {
            e.window.set_title(value);
            true
        }
        1 => match std::fs::read(value)
            .map_err(|err| format!("{value}: {err}"))
            .and_then(|png| app_icon::icon_from_png_bytes(&png).map(|icon| (png, icon)))
        {
            Ok((png, icon)) => {
                e.window.set_window_icon(Some(icon));
                app_icon::apply_dock_icon(&png);
                e.chrome.icon_path = value.to_string();
                true
            }
            Err(err) => {
                eprintln!("Dream WebView.icon: {err}");
                false
            }
        },
        _ => false,
    })
    .unwrap_or(false);
    pump();
    i32::from(!applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_codes_are_stable() {
        assert_eq!(Prop::from_code(0), Some(Prop::Width));
        assert_eq!(Prop::from_code(9), Some(Prop::Fullscreen));
        assert_eq!(Prop::from_code(14), Some(Prop::DevtoolsOpen));
        assert_eq!(Prop::from_code(15), Some(Prop::PageDialogs));
        assert_eq!(Prop::from_code(17), Some(Prop::ScaleMilli));
        assert_eq!(Prop::from_code(18), Some(Prop::DarkMode));
        assert_eq!(Prop::from_code(23), Some(Prop::InterceptClose));
        assert_eq!(Prop::from_code(22), Some(Prop::Restore));
        assert_eq!(Prop::from_code(24), Some(Prop::Close));
        assert_eq!(Prop::from_code(25), None);
        assert_eq!(Prop::from_code(-1), None);
    }

    #[test]
    fn size_limits() {
        assert!(limit((0, 0)).is_none());
        assert_eq!(limit((800, 0)).unwrap().width, 800.0);
        assert_eq!(limit((800, 0)).unwrap().height, f64::MAX);
        assert!(min_limit((0, 0)).is_none());
        assert_eq!(min_limit((0, 300)).unwrap(), LogicalSize::new(0.0, 300.0));
    }
}
