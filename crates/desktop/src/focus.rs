//! Focus options on this computer (D-82): mute the speakers while working and show the
//! timer on every monitor, following the user's focus timer wherever it was started. The
//! timer comes from the replica (kept current by the live events); what to do in each
//! phase is `streamline_domain::focus::Timer::effects`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use streamline_domain::focus::{Options, Phase, Screens, Show, Timer};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

use crate::engine::Engine;

/// The speakers, as far as muting goes.
pub trait Mixer {
    fn muted(&self) -> Option<bool>;
    fn set_muted(&self, on: bool) -> bool;
}

/// PipeWire or PulseAudio, through `pactl` (the default output).
pub struct Pactl;

impl Mixer for Pactl {
    fn muted(&self) -> Option<bool> {
        let out = std::process::Command::new("pactl")
            .args(["get-sink-mute", "@DEFAULT_SINK@"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        out.status.success().then(|| text.contains("yes"))
    }
    fn set_muted(&self, on: bool) -> bool {
        std::process::Command::new("pactl")
            .args([
                "set-sink-mute",
                "@DEFAULT_SINK@",
                if on { "1" } else { "0" },
            ])
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// Muting for focus and putting things back as they were: speakers the user had muted
/// already stay muted afterwards.
#[derive(Debug, Default)]
pub struct MuteGuard {
    /// We muted them (and they weren't muted before).
    muted_by_us: bool,
}

impl MuteGuard {
    pub fn apply(&mut self, want: bool, mixer: &dyn Mixer) {
        if want && !self.muted_by_us {
            if mixer.muted() == Some(false) && mixer.set_muted(true) {
                self.muted_by_us = true;
            }
        } else if !want && self.muted_by_us {
            // Unmute only if still muted (the user may have unmuted by hand meanwhile).
            if mixer.muted() == Some(true) {
                mixer.set_muted(false);
            }
            self.muted_by_us = false;
        }
    }
}

/// The user's timer as the server sends it.
pub fn parse_timer(v: &Value) -> Option<Timer> {
    Some(Timer {
        phase: Phase::parse(v.get("phase")?.as_str()?)?,
        task_id: v.get("task_id").and_then(Value::as_str).map(String::from),
        running_since: v.get("running_since_ms").and_then(Value::as_i64),
        elapsed_ms: v.get("elapsed_ms").and_then(Value::as_i64).unwrap_or(0),
        length_min: v.get("length_min").and_then(Value::as_u64).unwrap_or(0) as u32,
        cycle_done: v.get("cycle_done").and_then(Value::as_u64).unwrap_or(0) as u32,
        options: Options {
            mute: v.get("mute").and_then(Value::as_bool).unwrap_or(false),
            screens: v
                .get("screens")
                .and_then(Value::as_str)
                .and_then(Screens::parse)
                .unwrap_or_default(),
        },
    })
}

/// What the timer windows show.
pub fn readout(t: &Timer, now: i64) -> (String, &'static str) {
    let left = (t.remaining(now) + 999) / 1000;
    let time = format!("{}:{:02}", left / 60, left % 60);
    let waiting = t.phase == Phase::Work && t.running_since.is_none() && t.elapsed_ms == 0;
    let label = match t.phase {
        _ if waiting => "Break's over",
        _ if t.running_since.is_none() => "Paused",
        Phase::Work => "Focus",
        Phase::ShortBreak => "Short break",
        Phase::LongBreak => "Long break",
        Phase::Idle => "",
    };
    (time, label)
}

/// One interval (for "hide until the next one").
fn interval_key(t: &Timer) -> String {
    let start = t.running_since.map(|s| s - t.elapsed_ms).unwrap_or(-1);
    format!("{}:{start}", t.phase.as_str())
}

/// A monitor's place and size (physical pixels).
type Monitor = (i32, i32, u32, u32);

#[derive(Default)]
pub struct Focus {
    mute: Mutex<MuteGuard>,
    /// Full screen hidden for this interval (Esc).
    hidden: Mutex<Option<String>>,
    /// What the windows are made for: (show, monitors).
    windows: Mutex<Option<(Show, Vec<Monitor>)>>,
    current: Mutex<Option<String>>,
    /// Each set of windows gets new labels: closing the old ones finishes later.
    generation: std::sync::atomic::AtomicU32,
}

impl Focus {
    /// Hide full screen until the next interval.
    pub fn hide(&self) {
        *self.hidden.lock().unwrap() = self.current.lock().unwrap().clone();
    }

    /// Give the speakers back (quitting the app).
    pub fn restore(&self) {
        self.mute.lock().unwrap().apply(false, &Pactl);
    }

    /// Follow the timer every second.
    pub fn spawn(self: &Arc<Self>, app: AppHandle, engine: Arc<Engine>) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tick.tick().await;
                let timer = engine
                    .single("focus_timer")
                    .await
                    .as_ref()
                    .and_then(parse_timer);
                let timer = timer.unwrap_or_else(Timer::idle);
                let task = match &timer.task_id {
                    Some(id) => engine
                        .record("tasks", id)
                        .await
                        .and_then(|t| t["title"].as_str().map(String::from)),
                    None => None,
                };
                me.step(&app, &timer, task).await;
            }
        });
    }

    async fn step(&self, app: &AppHandle, t: &Timer, task: Option<String>) {
        let fx = t.effects();
        let key = interval_key(t);
        *self.current.lock().unwrap() = Some(key.clone());
        let mute = fx.mute;
        // pactl is quick, but it's a process: keep it off the async threads.
        let guard = {
            let mut g = self.mute.lock().unwrap();
            std::mem::take(&mut *g)
        };
        let guard = tauri::async_runtime::spawn_blocking(move || {
            let mut g = guard;
            g.apply(mute, &Pactl);
            g
        })
        .await
        .unwrap_or_default();
        *self.mute.lock().unwrap() = guard;

        let mut show = fx.show;
        if show == Show::Fullscreen && self.hidden.lock().unwrap().as_deref() == Some(key.as_str())
        {
            show = Show::Overlay;
        }
        self.windows(app, show);
        if show != Show::Nothing {
            let (time, label) = readout(t, chrono::Utc::now().timestamp_millis());
            let _ = app.emit(
                "focus-screen",
                json!({
                    "mode": if show == Show::Fullscreen { "fullscreen" } else { "overlay" },
                    "phase": t.phase.as_str(),
                    "break": t.phase.is_break(),
                    "time": time,
                    "label": label,
                    "task": task,
                }),
            );
        }
    }

    /// One window per monitor, made again when the mode or the monitors change.
    fn windows(&self, app: &AppHandle, show: Show) {
        let monitors: Vec<(i32, i32, u32, u32, f64)> = app
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .map(|m| {
                (
                    m.position().x,
                    m.position().y,
                    m.size().width,
                    m.size().height,
                    m.scale_factor(),
                )
            })
            .collect();
        let want = (
            show,
            monitors
                .iter()
                .map(|m| (m.0, m.1, m.2, m.3))
                .collect::<Vec<_>>(),
        );
        let mut current = self.windows.lock().unwrap();
        if current.as_ref() == Some(&want) || (show == Show::Nothing && current.is_none()) {
            return;
        }
        for (label, w) in app.webview_windows() {
            if label.starts_with("focus-") {
                let _ = w.destroy();
            }
        }
        *current = if show == Show::Nothing {
            None
        } else {
            Some(want)
        };
        if show == Show::Nothing {
            return;
        }
        let generation = self
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        for (i, (x, y, w, h, scale)) in monitors.into_iter().enumerate() {
            let full = show == Show::Fullscreen;
            let (ow, oh) = ((260.0 * scale) as u32, (84.0 * scale) as u32);
            let margin = (24.0 * scale) as i32;
            let (pos, size) = if full {
                (PhysicalPosition::new(x, y), PhysicalSize::new(w, h))
            } else {
                (
                    PhysicalPosition::new(x + w as i32 - ow as i32 - margin, y + margin),
                    PhysicalSize::new(ow, oh),
                )
            };
            let built = WebviewWindowBuilder::new(
                app,
                format!("focus-{generation}-{i}"),
                WebviewUrl::App("desktop-focus.html".into()),
            )
            .title("Streamline focus")
            .decorations(false)
            .transparent(!full)
            .always_on_top(true)
            .skip_taskbar(true)
            // The size is given here: a window that can't be resized keeps its first size.
            .inner_size(
                f64::from(size.width) / scale,
                f64::from(size.height) / scale,
            )
            .resizable(false)
            .shadow(false)
            .focused(full)
            .visible(false)
            .build();
            if let Ok(win) = built {
                // Shown first: on Linux the native window only exists from then on; size,
                // place and click-through need it (tao panics on click-through without one).
                let _ = win.show();
                let _ = win.set_position(pos);
                let _ = win.set_size(size);
                if full {
                    let _ = win.set_fullscreen(true);
                } else {
                    // The small timer never gets in the way of the mouse.
                    let _ = win.set_ignore_cursor_events(true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        muted: RefCell<bool>,
        log: RefCell<Vec<bool>>,
    }
    impl Mixer for Fake {
        fn muted(&self) -> Option<bool> {
            Some(*self.muted.borrow())
        }
        fn set_muted(&self, on: bool) -> bool {
            *self.muted.borrow_mut() = on;
            self.log.borrow_mut().push(on);
            true
        }
    }

    #[test]
    fn mutes_and_puts_back() {
        let mixer = Fake {
            muted: RefCell::new(false),
            log: RefCell::new(vec![]),
        };
        let mut g = MuteGuard::default();
        g.apply(true, &mixer);
        g.apply(true, &mixer);
        assert!(*mixer.muted.borrow());
        g.apply(false, &mixer);
        assert!(!*mixer.muted.borrow());
        assert_eq!(
            *mixer.log.borrow(),
            vec![true, false],
            "one mute, one unmute"
        );

        // Already muted by the user: left alone, and still muted afterwards.
        let mixer = Fake {
            muted: RefCell::new(true),
            log: RefCell::new(vec![]),
        };
        let mut g = MuteGuard::default();
        g.apply(true, &mixer);
        g.apply(false, &mixer);
        assert!(*mixer.muted.borrow());
        assert!(mixer.log.borrow().is_empty());

        // Unmuted by hand during focus: not muted again or toggled at the end.
        let mixer = Fake {
            muted: RefCell::new(false),
            log: RefCell::new(vec![]),
        };
        let mut g = MuteGuard::default();
        g.apply(true, &mixer);
        *mixer.muted.borrow_mut() = false;
        g.apply(false, &mixer);
        assert_eq!(*mixer.log.borrow(), vec![true]);
    }

    #[test]
    fn reads_the_timer() {
        let v = json!({"phase": "short_break", "task_id": null, "running_since_ms": 1_000, "elapsed_ms": 0,
            "length_min": 5, "cycle_done": 1, "mute": true, "screens": "fullscreen_breaks", "rev": 3});
        let t = parse_timer(&v).unwrap();
        assert_eq!(
            t.options,
            Options {
                mute: true,
                screens: Screens::FullscreenBreaks
            }
        );
        assert_eq!(t.effects().show, Show::Fullscreen);
        assert_eq!(readout(&t, 1_000 + 61_000), ("3:59".into(), "Short break"));
        let waiting = parse_timer(
            &json!({"phase": "work", "running_since_ms": null, "elapsed_ms": 0, "length_min": 25}),
        )
        .unwrap();
        assert_eq!(readout(&waiting, 0), ("25:00".into(), "Break's over"));
        let paused = parse_timer(&json!({"phase": "work", "running_since_ms": null, "elapsed_ms": 60_000, "length_min": 25})).unwrap();
        assert_eq!(readout(&paused, 0).1, "Paused");
        assert!(parse_timer(&json!({"phase": "nap"})).is_none());
        assert_ne!(interval_key(&t), interval_key(&waiting));
    }
}
