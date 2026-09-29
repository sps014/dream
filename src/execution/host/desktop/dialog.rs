//! Native file and message dialogs (`rfd`), started and polled without blocking the Dream task.
//!
//! macOS panels need the main-thread run loop, which only turns while something pumps winit, so
//! each poll pumps once (`webview::pump_idle`) before checking the dialog future. rfd also falls
//! back to a blocking modal when the app is not running at creation time, so requests are queued
//! and built from inside the pump (`open_queued`).

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use indexmap::IndexMap;
use rfd::{AsyncFileDialog, AsyncMessageDialog, FileHandle, MessageDialogResult};

use super::wire::{DialogKind, DialogRequest, Outcome};
use crate::execution::host::webview;

type Pending = Pin<Box<dyn Future<Output = Outcome>>>;

thread_local! {
    static NEXT: Cell<i32> = const { Cell::new(1) };
    static QUEUED: RefCell<Vec<(i32, DialogKind, DialogRequest)>> = const { RefCell::new(Vec::new()) };
    static PENDING: RefCell<IndexMap<i32, Pending>> = RefCell::new(IndexMap::new());
}

fn file_dialog(req: &DialogRequest) -> AsyncFileDialog {
    match webview::with_window(req.parent, |w| unparented_file_dialog(req).set_parent(w)) {
        Some(parented) => parented,
        None => unparented_file_dialog(req),
    }
}

fn unparented_file_dialog(req: &DialogRequest) -> AsyncFileDialog {
    let mut d = AsyncFileDialog::new();
    if !req.title.is_empty() {
        d = d.set_title(req.title.as_str());
    }
    if !req.directory.is_empty() {
        d = d.set_directory(PathBuf::from(&req.directory));
    }
    if !req.file_name.is_empty() {
        d = d.set_file_name(req.file_name.as_str());
    }
    for (name, exts) in &req.filters {
        d = d.add_filter(name.as_str(), exts);
    }
    d
}

fn message_dialog(req: &DialogRequest) -> AsyncMessageDialog {
    match webview::with_window(req.parent, |w| unparented_message_dialog(req).set_parent(w)) {
        Some(parented) => parented,
        None => unparented_message_dialog(req),
    }
}

fn unparented_message_dialog(req: &DialogRequest) -> AsyncMessageDialog {
    let level = match req.level {
        1 => rfd::MessageLevel::Warning,
        2 => rfd::MessageLevel::Error,
        _ => rfd::MessageLevel::Info,
    };
    let buttons = match req.buttons {
        1 => rfd::MessageButtons::OkCancel,
        2 => rfd::MessageButtons::YesNo,
        3 => rfd::MessageButtons::YesNoCancel,
        _ => rfd::MessageButtons::Ok,
    };
    AsyncMessageDialog::new()
        .set_title(req.title.as_str())
        .set_description(req.text.as_str())
        .set_level(level)
        .set_buttons(buttons)
}

fn one(handle: Option<FileHandle>) -> Outcome {
    match handle {
        Some(h) => Outcome::Paths(vec![h.path().to_string_lossy().into_owned()]),
        None => Outcome::Cancelled,
    }
}

fn many(handles: Option<Vec<FileHandle>>) -> Outcome {
    match handles {
        Some(hs) if !hs.is_empty() => Outcome::Paths(
            hs.iter()
                .map(|h| h.path().to_string_lossy().into_owned())
                .collect(),
        ),
        _ => Outcome::Cancelled,
    }
}

fn button(result: MessageDialogResult) -> Outcome {
    Outcome::Button(match result {
        MessageDialogResult::Ok => "ok",
        MessageDialogResult::Yes => "yes",
        MessageDialogResult::No => "no",
        MessageDialogResult::Cancel | MessageDialogResult::Custom(_) => "cancel",
    })
}

fn build(kind: DialogKind, req: &DialogRequest) -> Pending {
    match kind {
        DialogKind::PickFile => {
            let f = file_dialog(req).pick_file();
            Box::pin(async move { one(f.await) })
        }
        DialogKind::PickFiles => {
            let f = file_dialog(req).pick_files();
            Box::pin(async move { many(f.await) })
        }
        DialogKind::PickFolder => {
            let f = file_dialog(req).pick_folder();
            Box::pin(async move { one(f.await) })
        }
        DialogKind::PickFolders => {
            let f = file_dialog(req).pick_folders();
            Box::pin(async move { many(f.await) })
        }
        DialogKind::SaveFile => {
            let f = file_dialog(req).save_file();
            Box::pin(async move { one(f.await) })
        }
        DialogKind::Message => {
            let f = message_dialog(req).show();
            Box::pin(async move { button(f.await) })
        }
    }
}

/// Queues the dialog and returns a handle for [`poll`]; it opens on the next pump.
pub(crate) fn start(kind: DialogKind, req: &DialogRequest) -> i32 {
    let handle = NEXT.with(|n| {
        let h = n.get();
        n.set(h.wrapping_add(1).max(1));
        h
    });
    QUEUED.with(|q| q.borrow_mut().push((handle, kind, req.clone())));
    webview::pump_idle();
    handle
}

#[cfg(target_os = "macos")]
fn app_running() -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    MainThreadMarker::new().is_none_or(|mtm| NSApplication::sharedApplication(mtm).isRunning())
}

#[cfg(not(target_os = "macos"))]
fn app_running() -> bool {
    true
}

/// Called from inside the winit pump. The first pumps of a fresh event loop run before AppKit
/// finishes launching, so requests stay queued until the app reports running.
pub(crate) fn open_queued() {
    if !app_running() {
        return;
    }
    let queued = QUEUED.with(|q| std::mem::take(&mut *q.borrow_mut()));
    for (handle, kind, req) in queued {
        let future = build(kind, &req);
        PENDING.with(|p| p.borrow_mut().insert(handle, future));
    }
}

fn is_queued(handle: i32) -> bool {
    QUEUED.with(|q| q.borrow().iter().any(|(h, _, _)| *h == handle))
}

/// Drops a dialog whose result nobody will read (its page closed).
pub(crate) fn forget(handle: i32) {
    QUEUED.with(|q| q.borrow_mut().retain(|(h, _, _)| *h != handle));
    PENDING.with(|p| {
        p.borrow_mut().swap_remove(&handle);
    });
}

/// `None` while the dialog is still open. Unknown handles read as cancelled.
pub(crate) fn poll(handle: i32) -> Option<Outcome> {
    webview::pump_idle();
    if is_queued(handle) {
        return None;
    }
    let mut future = match PENDING.with(|p| p.borrow_mut().swap_remove(&handle)) {
        Some(f) => f,
        None => return Some(Outcome::Cancelled),
    };
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(outcome) => Some(outcome),
        Poll::Pending => {
            PENDING.with(|p| p.borrow_mut().insert(handle, future));
            None
        }
    }
}
