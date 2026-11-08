//! cad-review: an experimental native review app for claud3d, built with Rune.
//!
//! `cad-review --project projects/<name> [--server http://127.0.0.1:PORT]` opens a project's
//! latest build. With a server (`uv run cad review <name> --native` starts one), Send posts the
//! comments to it, and the server prints them for Claude and exits. Headless:
//! `cad-review --project projects/example-box --snap shot.png --size 1440x900 [--scene assembly]`.

mod app;
mod camera;
mod data;
mod gfx;
mod mesh;
mod net;
mod side;
mod style;
mod ui;

use std::sync::Arc;

use rune::{AppSpec, Key, KeyEvent, Tx};

use app::{App, Layout, Mode, Tab};

fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let typing = tx.focused().is_some();
    match &ev.key {
        Key::Enter if ev.mods.cmd => s.send(),
        Key::Escape => {
            if typing {
                tx.blur();
                return true;
            }
            return s.escape();
        }
        _ if typing => return false,
        Key::Tab => s.set_layout(if s.layout == Layout::Plate { Layout::Assembly } else { Layout::Plate }),
        Key::Left | Key::Right => {
            s.tour_step(if ev.key == Key::Left { -1 } else { 1 });
            s.tab = Tab::Features;
            if let Some(i) = s.tour {
                tx.scroll_into_view(format!("feature-card-{i}"));
            }
        }
        Key::Text(t) => match t.as_str() {
            " " => s.set_mode(if s.mode == Mode::Navigate { s.last_tool } else { Mode::Navigate }),
            "1" => s.set_mode(Mode::Pin),
            "2" => s.set_mode(Mode::Measure),
            "f" | "F" => s.fit(true),
            "b" | "B" => s.open_studio(),
            "?" => s.help = !s.help,
            _ => return false,
        },
        _ => return false,
    }
    true
}

fn poll(s: &mut App, t: f64, tx: &mut Tx) -> bool {
    let changed = s.step(t, tx);
    if s.animating() {
        tx.wake_after(std::time::Duration::from_millis(8));
    }
    changed
}

fn setup(s: &mut App, waker: rune::Waker) {
    s.shared.lock().unwrap_or_else(|e| e.into_inner()).waker = Some(waker);
}

fn main() {
    let spec = AppSpec::<App> {
        min_size: Some((900.0, 600.0)),
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        scenes: &[
            ("assembly", |s| {
                s.set_layout(Layout::Assembly);
                s.morph.set(1.0);
                s.cam.goal = None;
            }),
            ("exploded", |s| {
                s.set_layout(Layout::Assembly);
                s.morph.set(1.0);
                s.explode.set(0.6);
                s.cam.goal = None;
            }),
            ("report", |s| s.tab = Tab::Report),
            ("features", |s| s.tab = Tab::Features),
            ("parts", |s| s.tab = Tab::Parts),
            ("section", |s| {
                s.section = Some(1);
                s.section_t = 0.5;
            }),
            ("help", |s| s.help = true),
        ],
        ..AppSpec::new("claud3d review", (1440.0, 900.0), App::new, ui::view)
    };
    rune::App::new(spec).compositor(gfx::Gfx::new(Arc::clone(&app::SHARED))).run();
}
