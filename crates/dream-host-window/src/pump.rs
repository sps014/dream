use crate::{host::Host, metrics::Metrics, wire::kind};
use sdl3::{
    event::{Event, EventSender, WindowEvent},
    mouse::{Cursor, SystemCursor},
    video::WindowFlags,
};
use std::sync::OnceLock;

static WAKE: OnceLock<(u32, EventSender)> = OnceLock::new();

pub(crate) fn initialize_wake(sdl: &sdl3::Sdl) -> Result<(), sdl3::Error> {
    let events = sdl.event()?;
    // The reserved event carries no pointers or typed user data and has a single producer API.
    let id = unsafe { events.register_event()? };
    let _ = WAKE.set((id, events.event_sender()));
    Ok(())
}

pub(crate) fn wake() {
    if let Some((id, sender)) = WAKE.get() {
        let _ = sender.push_event(Event::User {
            timestamp: 0,
            window_id: 0,
            type_: *id,
            code: 0,
            data1: std::ptr::null_mut(),
            data2: std::ptr::null_mut(),
        });
    }
}

pub(crate) fn drain(host: &mut Host) {
    let mut events = std::mem::take(&mut host.parked);
    events.extend(host.pump.poll_iter());
    for entry in host.windows.values_mut() {
        let metrics = Metrics::read(&entry.window);
        metrics.changes(entry.metrics, &mut entry.input.wire);
        entry.metrics = metrics;
    }
    for event in events {
        match event {
            Event::Quit { .. } | Event::AppTerminating { .. } => {
                for entry in host.windows.values_mut() {
                    entry.input.cancel_touches();
                    entry.input.wire.push(kind::CLOSE_REQUESTED, &[]);
                }
            }
            Event::AppWillEnterBackground { .. } | Event::AppDidEnterBackground { .. } => {
                host.background = true;
                for entry in host.windows.values_mut() {
                    entry.input.occlusion(true);
                    if entry.input.focused {
                        entry.input.focused = false;
                        entry.input.wire.push(kind::UNFOCUSED, &[]);
                    }
                }
            }
            Event::AppDidEnterForeground { .. } => {
                host.background = false;
                for entry in host.windows.values_mut() {
                    if !entry.input.focused && entry.window.has_input_focus() {
                        entry.input.focused = true;
                        entry.input.wire.push(kind::FOCUSED, &[]);
                    }
                }
            }
            _ => {
                if let Some(id) = event
                    .get_window_id()
                    .and_then(|id| host.native_ids.get(&id))
                    .copied()
                {
                    if let Some(entry) = host.windows.get_mut(&id) {
                        entry.input.encode(&event, entry.metrics);
                    }
                    if matches!(
                        event,
                        Event::Window {
                            win_event: WindowEvent::FocusGained | WindowEvent::MouseEnter,
                            ..
                        }
                    ) {
                        apply_cursor(host, id);
                    }
                }
            }
        }
    }
    for entry in host.windows.values_mut() {
        let flags = WindowFlags::from(entry.window.window_flags());
        entry.input.occlusion(
            host.background
                || flags.intersects(
                    WindowFlags::HIDDEN | WindowFlags::MINIMIZED | WindowFlags::OCCLUDED,
                ),
        );
    }
}

pub(crate) fn apply_cursor(host: &mut Host, id: i32) {
    const SHAPES: [SystemCursor; 10] = [
        SystemCursor::Arrow,
        SystemCursor::IBeam,
        SystemCursor::Crosshair,
        SystemCursor::Hand,
        SystemCursor::SizeWE,
        SystemCursor::SizeNS,
        SystemCursor::SizeNWSE,
        SystemCursor::SizeNESW,
        SystemCursor::SizeAll,
        SystemCursor::No,
    ];
    let Some(entry) = host.windows.get(&id) else {
        return;
    };
    if !entry.window.has_mouse_focus() && !entry.window.has_input_focus() {
        return;
    }
    host.sdl.mouse().show_cursor(entry.cursor_mode == 0);
    let index = entry.cursor_icon;
    if host.cursors[index].is_none() {
        host.cursors[index] = Cursor::from_system(SHAPES[index]).ok();
    }
    if let Some(cursor) = &host.cursors[index] {
        cursor.set();
    }
}
