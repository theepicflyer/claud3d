//! The floating sidebar: build summary, then Comments, Report, Features and Parts tabs, and Send.

use glam::Vec3;
use rune::{Color, Cursor, El, ScrollSpec, SurfaceFit, col, el, row, text, text_input};
use serde_json::Value;

use crate::app::{App, MARGIN, SIDEBAR_W, Tab};
use crate::data::{Finding, Scene, Thread};
use crate::style::{BODY, CAPS, LABEL, Look, MONO, SMALL, STAT, STRONG, TITLE, badge, button, dot, frost, kbd, link};

const PAD: f32 = 14.0;
const INNER: f32 = SIDEBAR_W - PAD * 2.0;

pub fn sidebar(s: &App, l: &Look) -> El {
    let body = match s.tab {
        Tab::Comments => comments(s, l),
        Tab::Report => report(s, l),
        Tab::Features => features(s, l),
        Tab::Parts => parts(s, l),
    };
    frost(col(), l)
        .key("sidebar")
        .pin(None, Some(MARGIN), Some(MARGIN), Some(MARGIN))
        .w(SIDEBAR_W)
        .child(header(s, l))
        .child(tabs(s, l))
        .child(
            col().key(format!("tab-body-{:?}", s.tab)).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).scroll_edge_fade(12.0).child(body.w_full().px(PAD).py(10.0)),
        )
        .child(footer(s, l))
}

fn built(at: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (date, time) = at.split_once(' ').unwrap_or((at, ""));
    let d: Vec<&str> = date.split('-').collect();
    match (d.as_slice(), time.get(..5)) {
        ([_, m, day], Some(hm)) => {
            let mi = m.parse::<usize>().unwrap_or(1).clamp(1, 12) - 1;
            format!("{} {}, {hm}", MONTHS[mi], day.trim_start_matches('0'))
        }
        _ => at.to_owned(),
    }
}

fn stat(l: &Look, value: String, caption: String) -> El {
    col().grow(1.0).px(10.0).py(7.0).radius(9.0).fill(l.chip).gap(1.0).child(text(value).font(STAT).color(l.text)).child(text(caption).font(CAPS).color(l.faint))
}

fn header(s: &App, l: &Look) -> El {
    let Some(sc) = &s.scene else {
        return col().key("header").px(PAD).pt(PAD).pb(8.0).child(text("claud3d review").font(TITLE).color(l.text));
    };
    let mut info = format!("Built {}", built(&sc.built_at));
    if !sc.commit.is_empty() {
        info += &format!(" · {}", sc.commit);
    }
    if sc.dirty {
        info += " + uncommitted";
    }
    let mut h = col()
        .key("header")
        .px(PAD)
        .pt(PAD)
        .pb(10.0)
        .gap(6.0)
        .child(row().gap(8.0).center_y().child(text(sc.project.clone()).font(TITLE).color(l.text).truncate().shrink()).child(badge(&sc.status, l.status(&sc.status))))
        .child(text(info).font(SMALL).color(l.sub));
    if let Some(sl) = &sc.report.slice {
        let tiles = if sl.ok {
            let plates = sl.plates.filter(|p| *p > 1).map_or(String::new(), |p| format!(" · {p} plates"));
            row()
                .gap(6.0)
                .mt(4.0)
                .child(stat(l, sl.time.clone().unwrap_or_default(), format!("PRINT TIME{plates}")))
                .child(stat(l, format!("{:.1} g", sl.grams.unwrap_or(0.0)), sl.material.clone().unwrap_or_default().to_uppercase()))
                .child(stat(l, sl.nozzle.clone().unwrap_or_default().split(' ').next().unwrap_or("").to_owned(), "NOZZLE".into()))
        } else if sl.skipped {
            row().child(text("Not sliced: slice the STL files yourself").font(SMALL).color(l.sub))
        } else {
            row().child(text(format!("Slicing failed: {}", sl.message)).font(SMALL).color(l.fail).w_full().wrap())
        };
        h = h.child(tiles);
    }
    if s.server.is_some() && sc.report.slice.as_ref().is_some_and(|x| x.ok) {
        h = h.child(
            row().key("open-studio").mt(2.0).gap(6.0).h(24.0).px(8.0).radius(7.0).center_y().fill(l.chip).hover(l.hover).cursor(Cursor::Pointer).self_start()
                .on_press(|s: &mut App| s.open_studio())
                .child(kbd(l, "B"))
                .child(text("Open in Bambu Studio").font(LABEL).color(l.text)),
        );
    }
    h
}

fn tabs(s: &App, l: &Look) -> El {
    let open = s.threads.iter().filter(|t| t.status == "open").count() + s.drafts.len();
    let (fails, warns) = s.scene.as_ref().map_or((0, 0), |sc| problem_counts(sc));
    let list = [
        (Tab::Comments, "Comments", if open > 0 { open.to_string() } else { String::new() }),
        (Tab::Report, "Report", if fails + warns > 0 { (fails + warns).to_string() } else { String::new() }),
        (Tab::Features, "Features", String::new()),
        (Tab::Parts, "Parts", String::new()),
    ];
    let tw = (INNER - 4.0) / 4.0;
    let idx = list.iter().position(|x| x.0 == s.tab).unwrap_or(0) as f32;
    let mut bar = row().key("tabs").w(INNER).h(30.0).mx(PAD).mb(4.0).radius(9.0).fill(l.chip).padding(2.0)
        .child(el().key("tab-indicator").absolute(2.0 + idx * tw, 2.0).w(tw).h(26.0).radius(7.0).fill(if l.dark { Color::WHITE.alpha(0.14) } else { Color::WHITE }).shadow(1.0, 3.0, Color::BLACK.alpha(0.15)).motion(rune::Spring::new(0.32, 0.12)));
    for (tab, name, count) in list {
        let on = tab == s.tab;
        let mut r = row().key(format!("tab-{name}")).w(tw).h(26.0).center().gap(5.0).cursor(Cursor::Pointer).on_press(move |s: &mut App| s.tab = tab).child(text(name).font(LABEL).color(if on { l.text } else { l.sub }));
        if !count.is_empty() {
            let c = if tab == Tab::Report { if fails > 0 { l.fail } else { l.warn } } else { l.accent };
            r = r.child(el().h(16.0).min_w(16.0).px(4.0).radius(8.0).center().fill(c).child(text(count).font(TextStyleSmall::S).color(Color::WHITE)));
        }
        bar = bar.child(r);
    }
    bar
}

struct TextStyleSmall;
impl TextStyleSmall {
    const S: rune::tok::TextStyle = rune::tok::TextStyle::new(10.0, 700.0);
}

fn problem_counts(sc: &Scene) -> (usize, usize) {
    let all = sc.report.parts.iter().flat_map(|p| p.findings.iter()).chain(sc.report.spec.iter()).chain(sc.report.assembly.iter());
    all.fold((0, 0), |(f, w), x| match x.status.as_str() {
        "FAIL" => (f + 1, w),
        "WARN" => (f, w + 1),
        _ => (f, w),
    })
}

fn footer(s: &App, l: &Look) -> El {
    let (n, m) = s.pending();
    let ready = n + m > 0 && s.server.is_some() && !s.sending;
    let waiting = s.drafts.iter().any(|d| d.shot.is_none() && !d.edit.text.trim().is_empty());
    let label = if s.sending {
        "Sending…".to_owned()
    } else if n + m > 0 {
        let mut bits = Vec::new();
        if n > 0 {
            bits.push(format!("{n} new"));
        }
        if m > 0 {
            bits.push(format!("{m} repl{}", if m > 1 { "ies" } else { "y" }));
        }
        format!("Send to Claude ({})", bits.join(", "))
    } else {
        "Send to Claude".to_owned()
    };
    let empty = s.drafts.len() - n;
    let note = if s.server.is_none() {
        "No review server: comments can't be sent from here.".to_owned()
    } else if waiting {
        "Taking screenshots…".to_owned()
    } else if empty > 0 {
        format!("{empty} empty draft{} will be skipped.", if empty > 1 { "s" } else { "" })
    } else {
        String::new()
    };
    let mut f = col().key("footer").px(PAD).pt(10.0).pb(PAD).gap(6.0).child(el().h(1.0).w_full().fill(l.sep).mb(4.0)).child(
        row().key("send").w_full().h(38.0).radius(10.0).center().gap(8.0)
            .fill(if ready { l.accent } else { l.accent.alpha(0.35) })
            .hover(if ready { Color::WHITE.alpha(0.1) } else { Color::TRANSPARENT })
            .cursor(if ready { Cursor::Pointer } else { Cursor::Default })
            .on_press(|s: &mut App| s.send())
            .child(text(label).font(STRONG).color(Color::WHITE.alpha(if ready { 1.0 } else { 0.7 })))
            .child(kbd(l, "Ctrl ↵").opacity(if ready { 0.9 } else { 0.5 })),
    );
    if !note.is_empty() {
        f = f.child(text(note).font(SMALL).color(l.faint));
    }
    f
}

// ---------- comments ----------

pub fn anchor_text(a: &Value) -> String {
    if a.is_null() {
        return "General comment on this view".into();
    }
    let part = a["part"].as_str().unwrap_or("?");
    let near = a["feature"].as_str().map_or(String::new(), |f| format!(" · near {f}"));
    let v = |k: &str| crate::data::anchor_vec(a, k).map_or(String::new(), |p| format!("({:.1}, {:.1}, {:.1})", p.x, p.y, p.z));
    match a["kind"].as_str() {
        Some("measure") => format!("{part} → {}: {:.2} mm", a["part_b"].as_str().unwrap_or("?"), a["distance"].as_f64().unwrap_or(0.0)),
        Some("circle") if near.is_empty() => format!("Circle r≈{:.1} mm on {part} {}", a["radius"].as_f64().unwrap_or(0.0), v("point")),
        Some("circle") => format!("Circle on {part}{near}"),
        _ if !near.is_empty() => format!("Point on {part}{near}"),
        _ => format!("Point on {part} {}", v("point")),
    }
}

fn card(l: &Look, key: String, accent: Color, hot: bool) -> El {
    col().key(key).w_full().radius(11.0).fill(if hot { accent.alpha(0.12) } else { l.chip }).border(1.0, if hot { accent.alpha(0.7) } else { l.border }).padding(10.0).gap(7.0)
}

fn comments(s: &App, l: &Look) -> El {
    let mut c = col().key("comments").gap(10.0);
    c = c.child(
        row().w_full().center_y().gap(8.0)
            .child(button(l, "add-general", "+ General comment", |s, _| s.general_draft()))
            .child(el().grow(1.0))
            .child(toggle(l, "show-resolved", "Show resolved", s.show_resolved, |s| s.show_resolved = !s.show_resolved)),
    );
    if s.drafts.is_empty() && s.threads.iter().all(|t| t.status == "resolved" && !s.show_resolved) {
        c = c.child(
            col().w_full().py(18.0).gap(6.0).items_center()
                .child(text("No open comments").font(STRONG).color(l.sub))
                .child(text("Press 1 and click the model to pin one, or 2 to measure.").font(SMALL).color(l.faint).w_full().wrap().align(rune::Align::Center)),
        );
    }
    for (i, d) in s.drafts.iter().enumerate() {
        let key = d.key;
        let mut k = card(l, format!("draft-card-{key}"), l.accent, true)
            .child(
                row().w_full().gap(6.0).center_y()
                    .child(dot(l.accent, 8.0))
                    .child(text(format!("New {}", i + 1)).font(STRONG).color(l.text))
                    .child(text(anchor_text(&d.anchor)).font(SMALL).color(l.sub).truncate().shrink().grow(1.0))
                    .child(link(l, &format!("draft-del-{key}"), "Discard", l.faint, move |s, _| s.drafts.retain(|d| d.key != key))),
            )
            .child(
                text_input(&d.edit, BODY)
                    .key(format!("draft-input-{key}"))
                    .placeholder("What should change?", l.faint)
                    .color(l.text)
                    .w_full()
                    .min_h(54.0)
                    .padding(8.0)
                    .radius(8.0)
                    .fill(l.field)
                    .border(1.0, l.border)
                    .on_edit(move |s: &mut App| s.draft_edit(key))
                    .on_submit(|_: &mut App, tx: &mut rune::Tx| tx.blur()),
            );
        k = k.child(match &d.shot {
            Some(shot) => {
                let h = ((INNER - 20.0) * shot.size.1 as f32 / shot.size.0.max(1) as f32).min(190.0);
                el().w_full().h(h).radius(8.0).pixels_fit(&shot.pixels, SurfaceFit::Fit)
            }
            None => el().w_full().h(24.0).center().child(text("Taking a screenshot…").font(SMALL).color(l.faint)),
        });
        c = c.child(k);
    }
    for t in s.threads.iter().rev() {
        if t.status == "resolved" && !s.show_resolved {
            continue;
        }
        c = c.child(thread(s, l, t));
    }
    c
}

fn toggle(l: &Look, key: &str, label: &str, on: bool, f: impl Fn(&mut App) + 'static) -> El {
    let knob = el().size(12.0, 12.0).radius(6.0).fill(Color::WHITE).absolute(if on { 12.0 } else { 2.0 }, 2.0).motion(rune::Spring::new(0.25, 0.1));
    row().key(key.to_owned()).gap(6.0).center_y().cursor(Cursor::Pointer).on_press(f)
        .child(text(label.to_owned()).font(SMALL).color(l.sub))
        .child(el().key(format!("{key}-sw")).w(26.0).h(16.0).radius(8.0).fill(if on { l.accent } else { l.chip_on }).child(knob))
}

fn thread(s: &App, l: &Look, t: &Thread) -> El {
    let id = t.id;
    let resolved = t.status == "resolved";
    let c = if resolved { l.faint } else { l.blue };
    let hot = s.hot_thread == Some(id);
    let pending = s.replies.get(&id);
    let status = pending.and_then(|r| r.status).unwrap_or(if resolved { "resolved" } else { "open" });
    let mut k = card(l, format!("thread-{id}"), c, hot)
        .on_hover_change(move |s: &mut App, inside, _| {
            if inside {
                s.hot_thread = Some(id);
            } else if s.hot_thread == Some(id) {
                s.hot_thread = None;
            }
        })
        .child(
            row().w_full().gap(6.0).center_y()
                .child(el().h(18.0).px(6.0).radius(9.0).center().fill(c).child(text(format!("#{id}")).font(LABEL).color(Color::WHITE)))
                .child(text(anchor_text(&t.anchor)).font(SMALL).color(l.sub).truncate().shrink().grow(1.0))
                .child(badge(if status == "resolved" { "RESOLVED" } else { "OPEN" }, if status == "resolved" { l.faint } else { l.blue })),
        );
    for (by, msg) in &t.messages {
        let claude = by == "claude";
        k = k.child(
            col().w_full().gap(2.0).pl(8.0).border_sides([0.0, 0.0, 0.0, 2.0], if claude { l.accent.alpha(0.8) } else { l.sep })
                .child(text(if claude { "Claude" } else { "You" }).font(CAPS).color(if claude { l.accent } else { l.faint }))
                .child(text(msg.clone()).font(BODY).color(l.text).w_full().wrap()),
        );
    }
    let mut actions = row().w_full().gap(12.0).center_y();
    if !t.camera.is_null() {
        let cam = t.camera.clone();
        actions = actions.child(link(l, &format!("view-{id}"), "Show", l.blue, move |s, _| s.go_to_camera(&cam)));
    }
    if t.screenshot.as_ref().is_some_and(|p| p.exists()) {
        let p = t.screenshot.clone().unwrap_or_default();
        actions = actions.child(link(l, &format!("shot-{id}"), "Screenshot", l.blue, move |_, tx| {
            let _ = tx.open_url(&format!("file://{}", p.display()));
        }));
    }
    actions = actions.child(link(l, &format!("reply-{id}"), "Reply", l.blue, move |s, _| {
        s.reply_open = Some(id);
        s.reply_edit(id).focused = true;
    }));
    let flip = if status == "resolved" { "Reopen" } else { "Resolve" };
    actions = actions.child(link(l, &format!("status-{id}"), flip, l.blue, move |s, _| {
        let now = if resolved { "resolved" } else { "open" };
        let r = s.replies.entry(id).or_default();
        let want = if r.status.unwrap_or(now) == "resolved" { "open" } else { "resolved" };
        r.status = if want == now { None } else { Some(want) };
    }));
    if pending.is_some_and(|r| r.status.is_some()) {
        actions = actions.child(el().grow(1.0)).child(text("will be sent").font(SMALL).color(l.accent));
    }
    k = k.child(actions);
    if s.reply_open == Some(id) || pending.is_some_and(|r| !r.edit.text.is_empty()) {
        let edit = pending.map(|r| &r.edit);
        let empty = rune::TextEdit::new("");
        k = k.child(
            text_input(edit.unwrap_or(&empty), BODY)
                .key(format!("reply-input-{id}"))
                .placeholder("Reply…", l.faint)
                .color(l.text)
                .w_full()
                .min_h(40.0)
                .padding(8.0)
                .radius(8.0)
                .fill(l.field)
                .border(1.0, l.border)
                .on_edit(move |s: &mut App| s.reply_edit(id))
                .on_submit(|_: &mut App, tx: &mut rune::Tx| tx.blur()),
        );
    }
    k
}

// ---------- report ----------

fn mark(l: &Look, status: &str) -> El {
    let (glyph, c) = match status {
        "PASS" => ("✓", l.ok),
        "WARN" => ("!", l.warn),
        "FAIL" => ("×", l.fail),
        _ => ("·", l.faint),
    };
    el().size(16.0, 16.0).radius(8.0).center().fill(c).child(text(glyph).font(rune::tok::TextStyle::new(10.5, 800.0)).color(Color::WHITE))
}

fn section_title(l: &Look, title: &str, extra: Option<(String, Color)>) -> El {
    let mut r = row().w_full().gap(8.0).center_y().mt(6.0).child(text(title.to_owned()).font(CAPS).color(l.faint));
    if let Some((t, c)) = extra {
        r = r.child(text(t).font(LABEL).color(c));
    }
    r
}

fn report(s: &App, l: &Look) -> El {
    let mut c = col().key("report").gap(8.0);
    let Some(sc) = &s.scene else { return c };
    let spec = &sc.report.spec;
    if !spec.is_empty() {
        let met = spec.iter().filter(|f| f.status == "PASS").count();
        c = c.child(section_title(l, "REQUIREMENTS", Some((format!("{met} of {} met", spec.len()), if met == spec.len() { l.ok } else { l.warn }))));
        for (i, f) in spec.iter().enumerate() {
            let (id, rest) = match f.detail.split_once(' ') {
                Some((a, b)) if a.starts_with('R') && a[1..].chars().all(|ch| ch.is_ascii_digit()) => (a.to_owned(), b.to_owned()),
                _ => (String::new(), f.detail.clone()),
            };
            c = c.child(
                row().key(format!("spec-{i}")).w_full().gap(8.0).items_start()
                    .child(mark(l, &f.status).mt(1.0))
                    .child(el().w(26.0).child(text(id).font(STRONG).color(l.text)))
                    .child(text(units(&rest)).font(BODY).color(l.text).grow(1.0).shrink().wrap()),
            );
        }
    }
    c = c.child(section_title(l, "PRINTABILITY", None));
    for (pi, p) in sc.report.parts.iter().enumerate() {
        let bad: Vec<&Finding> = p.findings.iter().filter(|f| f.status != "PASS").collect();
        c = c.child(
            row().key(format!("part-report-{pi}")).w_full().gap(8.0).center_y()
                .child(text(p.name.clone()).font(STRONG).color(l.text).grow(1.0))
                .child(badge(&p.status, l.status(&p.status))),
        );
        if bad.is_empty() {
            let checks: Vec<&str> = p.findings.iter().map(|f| f.check.as_str()).collect();
            c = c.child(text(format!("All checks pass: {}", checks.join(", "))).font(SMALL).color(l.faint).w_full().wrap());
        }
        for (fi, f) in bad.into_iter().enumerate() {
            c = c.child(finding(l, f, format!("{pi}-{fi}"), Some(p.name.clone())));
        }
    }
    let asm = &sc.report.assembly;
    if !asm.is_empty() {
        let mut gaps: Vec<(&Finding, f32)> = asm.iter().map(|f| (f, f.items.iter().filter_map(|i| i.gap).fold(f32::MAX, f32::min))).collect();
        gaps.sort_by(|a, b| a.1.total_cmp(&b.1));
        c = c.child(section_title(l, "ASSEMBLY", Some(("closest first".into(), l.faint))));
        for (gi, (f, _)) in gaps.into_iter().enumerate() {
            if f.status != "PASS" {
                c = c.child(finding(l, f, format!("asm-{gi}"), None));
            } else {
                c = c.child(row().w_full().gap(8.0).center_y().child(mark(l, "PASS")).child(text(units(&f.detail)).font(SMALL).color(l.sub).grow(1.0).shrink().wrap()));
            }
        }
    }
    if let Some(sl) = sc.report.slice.as_ref().filter(|x| !x.ok && !x.skipped) {
        c = c.child(section_title(l, "SLICE", None)).child(text(sl.message.clone()).font(BODY).color(l.fail).w_full().wrap());
    }
    c
}

fn units(t: &str) -> String {
    t.replace("mm^2", "mm²").replace("mm^3", "mm³").replace("cm^3", "cm³")
}

fn finding(l: &Look, f: &Finding, key: String, part: Option<String>) -> El {
    let mut c = col().key(format!("finding-{key}")).w_full().gap(4.0).child(
        row().w_full().gap(8.0).items_start()
            .child(mark(l, &f.status).mt(1.0))
            .child(col().grow(1.0).shrink().gap(1.0).child(text(f.check.clone()).font(STRONG).color(l.text)).child(text(units(&f.detail)).font(SMALL).color(l.sub).w_full().wrap())),
    );
    for (ii, it) in f.items.iter().enumerate().take(12) {
        let Some(at) = it.at else { continue };
        let label = format!("{} · ({:.1}, {:.1}, {:.1})", it.where_.clone().unwrap_or_else(|| it.text.clone()), at.x, at.y, at.z);
        let part = part.clone();
        let tag = it.text.clone();
        c = c.child(
            row().key(format!("loc-{key}-{ii}")).ml(24.0).h(22.0).px(6.0).gap(6.0).radius(6.0).center_y().hover(l.hover).cursor(Cursor::Pointer)
                .on_press(move |s: &mut App| s.locate(part.as_deref(), at, tag.clone()))
                .child(dot(l.pink, 6.0))
                .child(text(units(&label)).font(MONO).color(l.sub).truncate().shrink()),
        );
    }
    c
}

// ---------- features ----------

fn features(s: &App, l: &Look) -> El {
    let mut c = col().key("features").gap(8.0);
    let Some(sc) = &s.scene else { return c };
    c = c.child(
        row().w_full().gap(8.0).center_y()
            .child(button(l, "tour", if s.tour.is_some() { "Next feature  →" } else { "▶ Walkthrough" }, |s, _| s.tour_step(1)))
            .child(el().grow(1.0))
            .child(toggle(l, "labels", "Labels", s.show_labels, |s| s.show_labels = !s.show_labels)),
    );
    if sc.features.is_empty() {
        c = c.child(text("This build has no feature callouts.").font(SMALL).color(l.faint));
    }
    for (i, f) in sc.features.iter().enumerate() {
        let hot = s.hot_feature == Some(i) || s.tour == Some(i);
        let mut chips = row().gap(4.0).flex_wrap();
        for (k, v) in &f.params {
            chips = chips.child(el().h(18.0).px(6.0).radius(5.0).center().fill(l.chip_on).child(text(format!("{k} = {v}")).font(MONO).color(l.sub)));
        }
        let mut k = card(l, format!("feature-card-{i}"), l.teal, hot).cursor(Cursor::Pointer).on_press(move |s: &mut App| s.show_feature(i))
            .child(row().w_full().gap(6.0).center_y().child(dot(l.teal, 8.0)).child(text(f.name.clone()).font(STRONG).color(l.text).grow(1.0)).child(text(f.part.clone()).font(SMALL).color(l.faint)));
        if !f.text.is_empty() {
            k = k.child(text(f.text.clone()).font(SMALL).color(l.sub).w_full().wrap());
        }
        if !f.params.is_empty() {
            k = k.child(chips);
        }
        c = c.child(k);
    }
    if !sc.params.is_empty() {
        c = c.child(section_title(l, "ALL PARAMETERS", None));
        for (k, v) in &sc.params {
            c = c.child(row().w_full().gap(8.0).child(text(k.clone()).font(MONO).color(l.sub).grow(1.0)).child(text(v.clone()).font(MONO).color(l.text)));
        }
    }
    c
}

// ---------- parts ----------

fn parts(s: &App, l: &Look) -> El {
    let mut c = col().key("parts").gap(6.0);
    let Some(sc) = &s.scene else { return c };
    c = c.child(text("Click a part to show only it; click again to show all.").font(SMALL).color(l.faint).w_full().wrap());
    for (i, p) in sc.parts.iter().enumerate() {
        let key = p.key.clone();
        let on = s.solo.as_deref() == Some(p.key.as_str());
        let dims = format!("{:.1} × {:.1} × {:.1} mm", p.bbox[0], p.bbox[1], p.bbox[2]);
        let col_ = Color { r: p.color[0], g: p.color[1], b: p.color[2], a: 1.0 };
        let mut r = row().key(format!("part-{i}")).w_full().h(44.0).px(10.0).gap(10.0).radius(10.0).center_y()
            .fill(if on { l.chip_on } else { l.chip })
            .hover(l.hover)
            .cursor(Cursor::Pointer)
            .on_press(move |s: &mut App| s.toggle_solo(&key))
            .child(el().size(14.0, 14.0).radius(4.0).fill(col_))
            .child(col().grow(1.0).gap(1.0).child(text(p.name.clone()).font(STRONG).color(l.text)).child(text(dims).font(MONO).color(l.faint)));
        r = r.child(if p.printed { badge(if p.status.is_empty() { "—" } else { &p.status }, l.status(&p.status)) } else { badge("FIXTURE", l.faint) });
        c = c.child(r);
    }
    let _ = Vec3::ZERO;
    c
}
