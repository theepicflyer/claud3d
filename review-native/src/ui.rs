//! The window: the 3D viewport filling it, and floating panels over it.

use glam::{Vec2, Vec3};
use rune::{Color, Cursor, Cx, Drag, El, LineCap, PointerButton, Shape, Tx, canvas, col, el, row, text};

use crate::app::{App, Gesture, Layout, MARGIN, Mode, SIDEBAR_W};
use crate::gfx::{SLOT, background};
use crate::side;
use crate::style::{self, BODY, CAPS, LABEL, Look, MONO, SMALL, STRONG, badge, frost, kbd, sep, slider, tool};

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let dark = cx.system_dark();
    let l = style::look(dark);
    let (top, bottom) = background(dark);
    let c = |v: [f32; 3]| Color { r: v[0], g: v[1], b: v[2], a: 1.0 };
    let mut root = el().w_full().h_full().fill(c(bottom)).gradient(c(top), c(bottom)).child(viewport(s));
    if let Some(e) = &s.error {
        root = root.child(
            col().absolute(0.0, 0.0).w_full().h_full().center().child(
                frost(col(), &l).key("error").padding(24.0).gap(8.0).max_w(520.0).child(text("Nothing to review yet").font(STRONG).color(l.text)).child(text(e.clone()).font(BODY).color(l.sub).wrap()),
            ),
        );
    }
    root.child(overlays(s, &l))
        .child(toolbar(s, &l))
        .child(bottom_bar(s, &l))
        .child(tour(s, &l))
        .child(triad(s, &l))
        .child(side::sidebar(s, &l))
        .child(toast(s, &l))
        .child(help(s, &l))
}

fn viewport(s: &App) -> El {
    let cursor = match (s.gesture, s.mode) {
        (Some(Gesture::Pan), _) => Cursor::Grabbing,
        (_, Mode::Pin | Mode::Measure) => Cursor::Crosshair,
        _ => Cursor::Default,
    };
    el().key("viewport")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .cursor(cursor)
        .on_layout(|s: &mut App, r| s.vp = r)
        .pointer_buttons([PointerButton::Middle])
        .on_drag(|s: &mut App, d: &mut Drag, tx: &mut Tx| {
            let middle = tx.button() == PointerButton::Middle;
            let shift = tx.mods().shift;
            s.drag(d.phase, Vec2::new(d.x, d.y), Vec2::new(d.dx, d.dy), middle, shift);
            if d.phase == rune::Phase::Move {
                s.hover = None;
            }
        })
        .on_wheel(|s: &mut App, e, _| {
            s.wheel(Vec2::new(e.x, e.y), e.dy, e.precise);
            true
        })
        .on_pointer_move_local(|s: &mut App, e, _| {
            let p = Vec2::new(e.x, e.y);
            s.pointer = Some(p);
            if s.gesture.is_none() {
                s.hover = if s.mode == Mode::Measure { s.pick_snapped(p, false) } else { s.pick(p) };
            }
        })
        .on_hover_change(|s: &mut App, inside, _| {
            if !inside {
                s.pointer = None;
                s.hover = None;
            }
        })
        .child(el().key("scene").w_full().h_full().effect(SLOT))
}

struct Placed {
    rects: Vec<[f32; 4]>,
}

impl Placed {
    fn free(&mut self, r: [f32; 4]) -> bool {
        let hit = self.rects.iter().any(|q| r[0] < q[0] + q[2] && q[0] < r[0] + r[2] && r[1] < q[1] + q[3] && q[1] < r[1] + r[3]);
        if !hit {
            self.rects.push(r);
        }
        !hit
    }
}

/// A dot on the model with a pill label beside it.
fn callout(key: String, at: Vec2, label: &str, c: Color, strong: bool) -> El {
    let w = label.chars().count() as f32 * 6.3 + 18.0;
    let pill = row()
        .h(20.0)
        .px(8.0)
        .center_y()
        .radius(10.0)
        .fill(c.alpha(if strong { 1.0 } else { 0.88 }))
        .shadow(2.0, 8.0, Color::BLACK.alpha(0.3))
        .child(text(label.to_owned()).font(if strong { STRONG } else { LABEL }).color(Color::WHITE));
    el().key(key)
        .absolute(at.x - 4.0, at.y - 26.0)
        .w(w + 4.0)
        .h(30.0)
        .child(el().absolute(0.0, 22.0).size(8.0, 8.0).radius(4.0).fill(c).border(1.5, Color::WHITE))
        .child(el().absolute(3.25, 16.0).size(1.5, 8.0).fill(c))
        .child(pill.absolute(-4.0, -2.0))
}

fn overlays(s: &App, l: &Look) -> El {
    let mut layer = el().key("overlays").absolute(0.0, 0.0).w_full().h_full().instant_changes();
    let Some(scene) = &s.scene else { return layer };
    let mut placed = Placed { rects: Vec::new() };
    let in_view = |p: Vec2| p.x > 0.0 && p.y > 0.0 && p.x < s.vp[2] - SIDEBAR_W - MARGIN * 2.0 && p.y < s.vp[3];
    let shown = |w: Vec3| s.kept(w);

    if let (Some(w), Some(loc)) = (s.locate_world(), &s.locate)
        && let Some(p) = s.project(w)
    {
        layer = layer.child(
            el().key("locate").absolute(p.x - 14.0, p.y - 14.0).size(28.0, 28.0).radius(14.0).border(2.5, l.pink).fill(l.pink.alpha(0.15)),
        );
        let label = loc.label.clone();
        placed.free([p.x, p.y - 30.0, label.len() as f32 * 6.3 + 20.0, 22.0]);
        layer = layer.child(callout("locate-label".into(), p, &label, l.pink, true));
    }

    // Comment pins: drafts first, then threads.
    let mut pins: Vec<(String, Vec3, Color, String, Option<i64>)> = Vec::new();
    for (i, d) in s.drafts.iter().enumerate() {
        if let Some(w) = s.draft_world(d) {
            pins.push((format!("draft-{}", d.key), w, l.accent, format!("new {}", i + 1), None));
        }
        if d.anchor["kind"] == "measure"
            && let Some(w) = s.anchor_world(&d.anchor, true)
        {
            pins.push((format!("draft-{}-b", d.key), w, l.accent, String::new(), None));
        }
    }
    for t in &s.threads {
        if t.status == "resolved" && !s.show_resolved && s.hot_thread != Some(t.id) {
            continue;
        }
        let view_ok = t.anchor["view"].as_str().is_none_or(|v| v == s.layout.name() || t.anchor["frame"] != "assembly");
        if let Some(w) = s.thread_world(t).filter(|_| view_ok) {
            let c = if t.status == "resolved" { l.faint } else { l.blue };
            pins.push((format!("thread-{}", t.id), w, c, format!("#{}", t.id), Some(t.id)));
        }
    }
    for (key, w, c, label, id) in pins {
        let Some(p) = s.project(w).filter(|p| in_view(*p) && shown(w)) else { continue };
        let hidden = s.occluded(w);
        let hot = id.is_some() && id == s.hot_thread;
        if label.is_empty() {
            layer = layer.child(el().key(key).absolute(p.x - 5.0, p.y - 5.0).size(10.0, 10.0).radius(5.0).fill(c).border(2.0, Color::WHITE));
            continue;
        }
        let w_ = label.len() as f32 * 7.0 + 14.0;
        placed.free([p.x - w_ * 0.5, p.y - 30.0, w_, 24.0]);
        let mut pin = col()
            .key(key)
            .absolute(p.x - w_ * 0.5, p.y - 31.0)
            .w(w_)
            .items_center()
            .opacity(if hidden { 0.35 } else { 1.0 })
            .child(
                el().h(22.0).px(7.0).radius(11.0).center().fill(c).border(if hot { 2.0 } else { 0.0 }, Color::WHITE).shadow(2.0, 8.0, Color::BLACK.alpha(0.35))
                    .child(text(label).font(STRONG).color(Color::WHITE)),
            )
            .child(el().w(2.0).h(6.0).fill(c))
            .child(el().size(6.0, 6.0).radius(3.0).fill(c).border(1.0, Color::WHITE));
        if let Some(id) = id {
            pin = pin.cursor(Cursor::Pointer).on_press_tx(move |s: &mut App, tx: &mut Tx| {
                s.tab = crate::app::Tab::Comments;
                s.hot_thread = Some(id);
                tx.scroll_into_view(format!("thread-{id}"));
            });
        }
        layer = layer.child(pin);
    }

    // Measurement.
    let pts: Vec<Vec2> = s.measure.iter().filter_map(|h| s.project(h.world)).collect();
    if pts.len() == 2 {
        let (a, b) = (pts[0], pts[1]);
        let lo = a.min(b) - Vec2::splat(4.0);
        let size = (a.max(b) - lo + Vec2::splat(4.0)).max(Vec2::splat(1.0));
        let shape = Shape::new().line([a.x - lo.x, a.y - lo.y], [b.x - lo.x, b.y - lo.y], 2.0, LineCap::Round, l.text.alpha(0.9));
        layer = layer.child(canvas(shape).key("measure-line").absolute(lo.x, lo.y).size(size.x, size.y));
        let d = s.measure[1].world.distance(s.measure[0].world);
        let mid = (a + b) * 0.5;
        let label = format!("{d:.2} mm");
        let w_ = label.len() as f32 * 7.0 + 16.0;
        placed.free([mid.x - w_ * 0.5, mid.y - 12.0, w_, 24.0]);
        layer = layer.child(
            el().key("measure-label").absolute(mid.x - w_ * 0.5, mid.y - 12.0).w(w_).h(24.0).radius(12.0).center().fill(if l.dark { Color::BLACK.alpha(0.8) } else { Color::WHITE.alpha(0.92) }).border(1.0, l.border).shadow(2.0, 8.0, Color::BLACK.alpha(0.3))
                .child(text(label).font(STRONG).color(l.text)),
        );
    }
    for (i, p) in pts.iter().enumerate() {
        layer = layer.child(el().key(format!("measure-{i}")).absolute(p.x - 5.0, p.y - 5.0).size(10.0, 10.0).radius(5.0).fill(l.text).border(2.0, if l.dark { Color::BLACK } else { Color::WHITE }));
    }
    if s.mode == Mode::Measure
        && let Some(h) = &s.hover
        && let Some(p) = s.project(h.world)
    {
        let ring = el().key("snap").absolute(p.x - 7.0, p.y - 7.0).size(14.0, 14.0).radius(if h.snap.is_some() { 2.0 } else { 7.0 }).border(2.0, l.accent);
        layer = layer.child(ring);
        if let Some((k, r)) = h.snap {
            let label = match r {
                Some(r) => format!("{} ⌀{:.2}", k.name(), r * 2.0),
                None => k.name().to_owned(),
            };
            layer = layer.child(el().key("snap-label").absolute(p.x + 10.0, p.y + 6.0).h(18.0).px(6.0).radius(5.0).center().fill(l.accent).child(text(label).font(SMALL).color(Color::WHITE)));
        }
    }

    // Feature labels, nearest first so a far label gives way to a near one.
    if s.show_labels {
        let eye = s.cam.now.eye();
        let mut feats: Vec<(usize, Vec3, f32)> = scene
            .features
            .iter()
            .enumerate()
            .filter(|(_, f)| f.view.as_deref() != Some("assembly") || s.layout == Layout::Assembly)
            .filter_map(|(i, f)| {
                let pi = s.part_index(&f.part, "part")?;
                let (_, _, m, a) = *s.posed().get(pi)?;
                (a > 0.5).then(|| {
                    let w = m.transform_point3(f.point);
                    (i, w, w.distance(eye))
                })
            })
            .collect();
        feats.sort_by(|a, b| a.2.total_cmp(&b.2));
        for (i, w, _) in feats {
            let hot = s.hot_feature == Some(i) || s.tour == Some(i);
            let Some(p) = s.project(w).filter(|p| in_view(*p) && shown(w)) else { continue };
            if !hot && s.occluded(w) {
                continue;
            }
            let name = &scene.features[i].name;
            let w_ = name.chars().count() as f32 * 6.3 + 18.0;
            if !placed.free([p.x - 4.0, p.y - 28.0, w_, 22.0]) && !hot {
                continue;
            }
            layer = layer.child(callout(format!("feature-{i}"), p, name, l.teal, hot).cursor(Cursor::Pointer).on_press_tx(move |s: &mut App, tx: &mut Tx| {
                s.show_feature(i);
                s.tab = crate::app::Tab::Features;
                tx.scroll_into_view(format!("feature-card-{i}"));
            }));
        }
    }
    layer
}

fn toolbar(s: &App, l: &Look) -> El {
    let has_asm = s.scene.as_ref().is_some_and(|sc| sc.has_assembly);
    let modes = row()
        .gap(2.0)
        .child(tool(l, "mode-nav", "Navigate", Some("␣"), s.mode == Mode::Navigate, |s, _| s.set_mode(Mode::Navigate)))
        .child(tool(l, "mode-pin", "Pin", Some("1"), s.mode == Mode::Pin, |s, _| s.set_mode(Mode::Pin)))
        .child(tool(l, "mode-measure", "Measure", Some("2"), s.mode == Mode::Measure, |s, _| s.set_mode(Mode::Measure)));
    let mut layouts = row().gap(2.0).child(tool(l, "layout-plate", "Plate", Some("⇥"), s.layout == Layout::Plate, |s, _| s.set_layout(Layout::Plate)));
    if has_asm {
        layouts = layouts.child(tool(l, "layout-assembly", "Assembly", None, s.layout == Layout::Assembly, |s, _| s.set_layout(Layout::Assembly)));
    }
    let mut cams = row().gap(2.0);
    for (k, label) in [("iso", "Iso"), ("front", "Front"), ("right", "Right"), ("top", "Top")] {
        cams = cams.child(tool(l, &format!("cam-{k}"), label, None, false, move |s, _| s.view_preset(k)));
    }
    cams = cams.child(tool(l, "cam-fit", "Fit", Some("F"), false, |s, _| s.fit(true)));
    let mut section = row().gap(2.0).center_y().child(text("Section").font(LABEL).color(l.sub).mr(4.0));
    for (i, a) in ["X", "Y", "Z"].iter().enumerate() {
        section = section.child(tool(l, &format!("section-{a}"), a, None, s.section == Some(i), move |s, _| {
            s.section = if s.section == Some(i) { None } else { Some(i) };
        }));
    }
    if s.section.is_some() {
        section = section
            .child(el().w(6.0))
            .child(slider(l, "section-pos", 110.0, s.section_t, true, |s, v| s.section_t = v))
            .child(tool(l, "section-flip", "Flip", None, s.section_flip, |s, _| s.section_flip = !s.section_flip));
    }
    frost(row(), l)
        .key("toolbar")
        .absolute(MARGIN, MARGIN)
        .h(40.0)
        .px(6.0)
        .gap(8.0)
        .center_y()
        .child(row().gap(8.0).center_y().child(modes).child(sep(l)).child(layouts).child(sep(l)).child(cams).child(sep(l)).child(section))
}

fn fmt3(v: Vec3) -> String {
    format!("{:.2}, {:.2}, {:.2}", v.x, v.y, v.z)
}

fn bottom_bar(s: &App, l: &Look) -> El {
    let mut bar = row().key("bottom").absolute(MARGIN, s.vp[3] - MARGIN - 40.0).gap(8.0).h(40.0);
    let mut readout = frost(row(), l).key("readout").h(40.0).px(12.0).gap(10.0).center_y();
    let mut any = false;
    if s.measure.len() == 2 {
        let (a, b) = (&s.measure[0], &s.measure[1]);
        let d = b.world - a.world;
        let leg = |n: &str, v: f32, c: Color| row().gap(4.0).center_y().child(text(n.to_owned()).font(CAPS).color(c)).child(text(format!("{v:+.2}")).font(MONO).color(l.text));
        readout = readout
            .child(text(format!("{:.2} mm", d.length())).font(STRONG).color(l.text))
            .child(leg("ΔX", d.x, style::hx(0xd23c3c)))
            .child(leg("ΔY", d.y, style::hx(0x2e9b45)))
            .child(leg("ΔZ", d.z, style::hx(0x4c8dff)));
        if let Some(axis) = crate::app::axis_of(d) {
            readout = readout.child(badge(&format!("along {axis}"), l.teal));
        }
        readout = readout.child(style::button(l, "measure-comment", "Comment on this", |s, _| s.measure_draft()));
        any = true;
    } else if let Some(h) = &s.hover {
        let name = s.part(h.part).map_or("", |p| p.name.as_str()).to_owned();
        readout = readout.child(text(name).font(STRONG).color(l.text)).child(text(fmt3(h.local)).font(MONO).color(l.sub));
        if let Some((k, r)) = h.snap {
            readout = readout.child(badge(k.name(), l.accent));
            if let Some(r) = r {
                readout = readout.child(text(format!("⌀ {:.2}", r * 2.0)).font(MONO).color(l.sub));
            }
        }
        any = true;
    } else if s.mode == Mode::Measure {
        let hint = if s.measure.len() == 1 { "Click the second point" } else { "Click a point to measure from. Shift-click places it freely." };
        readout = readout.child(text(hint).font(BODY).color(l.sub));
        any = true;
    } else if s.mode == Mode::Pin {
        readout = readout.child(text("Click the model to pin a comment").font(BODY).color(l.sub));
        any = true;
    }
    if any {
        bar = bar.child(readout);
    }
    if s.layout == Layout::Assembly {
        bar = bar.child(
            frost(row(), l).key("explode").h(40.0).px(12.0).gap(10.0).center_y().child(text("Explode").font(LABEL).color(l.sub)).child(slider(l, "explode-slider", 140.0, s.explode.to / 1.5, true, |s, v| s.set_explode(v * 1.5))),
        );
    }
    bar
}

/// The walkthrough's caption: the feature being shown, what it is, and the parameters behind it.
fn tour(s: &App, l: &Look) -> El {
    let (Some(i), Some(sc)) = (s.tour, &s.scene) else { return el().key("tour-none") };
    let Some(f) = sc.features.get(i) else { return el().key("tour-none") };
    let w = 520.0f32.min(s.vp[2] - SIDEBAR_W - MARGIN * 4.0);
    let x = (s.vp[2] - SIDEBAR_W - MARGIN - w) * 0.5;
    let mut chips = row().gap(4.0).flex_wrap();
    for (k, v) in &f.params {
        chips = chips.child(el().h(18.0).px(6.0).radius(5.0).center().fill(l.chip_on).child(text(format!("{k} = {v}")).font(MONO).color(l.sub)));
    }
    let nav = row().gap(6.0).center_y()
        .child(style::button(l, "tour-prev", "←", |s, _| s.tour_step(-1)))
        .child(text(format!("{} / {}", i + 1, sc.features.len())).font(MONO).color(l.sub))
        .child(style::button(l, "tour-next", "→", |s, _| s.tour_step(1)))
        .child(style::button(l, "tour-end", "Done", |s, _| s.tour = None));
    frost(col(), l)
        .key("tour")
        .pin(Some(x), None, None, Some(MARGIN + 52.0))
        .w(w)
        .padding(14.0)
        .gap(6.0)
        .transition(rune::Transition::opacity())
        .child(row().w_full().gap(8.0).center_y().child(style::dot(l.teal, 9.0)).child(text(f.name.clone()).font(STRONG).color(l.text).grow(1.0)).child(nav))
        .child(text(f.text.clone()).font(BODY).color(l.sub).w_full().wrap())
        .child(chips)
}

fn triad(s: &App, l: &Look) -> El {
    let size = 64.0;
    let o = s.cam.now;
    let (right, up) = o.basis();
    let c = Vec2::splat(size * 0.5);
    let axes = [(Vec3::X, "X", style::hx(0xd23c3c)), (Vec3::Y, "Y", style::hx(0x2e9b45)), (Vec3::Z, "Z", style::hx(0x4c8dff))];
    let mut shape = Shape::new();
    let mut e = el().key("triad").absolute(s.vp[2] - SIDEBAR_W - MARGIN * 2.0 - size - 8.0, s.vp[3] - MARGIN - size).size(size, size).instant_changes();
    let mut ends = Vec::new();
    for (v, n, col) in axes {
        let p = c + Vec2::new(v.dot(right), -v.dot(up)) * 22.0;
        shape = shape.line([c.x, c.y], [p.x, p.y], 2.0, LineCap::Round, col);
        ends.push((n, p, col));
    }
    e = e.child(canvas(shape).absolute(0.0, 0.0).size(size, size));
    for (n, p, col) in ends {
        e = e.child(el().absolute(p.x - 6.0, p.y - 8.0).size(12.0, 16.0).center().child(text(n).font(CAPS).color(col)));
    }
    let _ = l;
    e
}

fn toast(s: &App, l: &Look) -> El {
    let Some((msg, _)) = &s.toast else { return el().key("toast-none") };
    let w = s.vp[2] - SIDEBAR_W - MARGIN;
    row().key("toast-row").absolute(0.0, 64.0).w(w.max(200.0)).justify_center().child(
        frost(row(), l).key("toast").h(34.0).px(14.0).center().transition(rune::Transition::opacity()).child(text(msg.clone()).font(LABEL).color(l.text)),
    )
}

fn help(s: &App, l: &Look) -> El {
    if !s.help {
        return el().key("help-none");
    }
    let rows = [
        ("Left-drag", "Rotate"),
        ("Shift- or middle-drag", "Pan"),
        ("Wheel", "Zoom toward the pointer"),
        ("Space", "Navigate / last tool"),
        ("1 / 2", "Pin / Measure"),
        ("Esc", "Cancel, then back to Navigate"),
        ("Tab", "Plate / Assembly"),
        ("F", "Fit"),
        ("← →", "Walk through features"),
        ("B", "Open in Bambu Studio"),
        ("Ctrl+Enter", "Send to Claude"),
        ("?", "This window"),
    ];
    let mut list = col().gap(6.0);
    for (k, v) in rows {
        list = list.child(row().gap(12.0).center_y().child(el().w(150.0).child(kbd(l, k))).child(text(v).font(BODY).color(l.text)));
    }
    col().key("help-scrim").absolute(0.0, 0.0).w_full().h_full().center().fill(Color::BLACK.alpha(0.25)).on_press(|s: &mut App| s.help = false).child(
        frost(col(), l)
            .key("help")
            .padding(20.0)
            .gap(14.0)
            .on_press(|_: &mut App| {})
            .child(row().center_y().child(text("Controls").font(style::TITLE).color(l.text).grow(1.0)).child(style::button(l, "help-close", "Close", |s, _| s.help = false)))
            .child(list),
    )
}
