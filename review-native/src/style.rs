//! Colours and the few small widgets the panels share.

use rune::tok::{self, TextStyle};
use rune::{Color, Cursor, El, Tx, el, row, text};

use crate::app::App;

pub const BODY: TextStyle = TextStyle::new(13.0, 400.0);
pub const SMALL: TextStyle = TextStyle::new(11.5, 400.0);
pub const LABEL: TextStyle = TextStyle::new(12.0, 500.0);
pub const STRONG: TextStyle = TextStyle::new(13.0, 600.0);
pub const CAPS: TextStyle = TextStyle::new(10.5, 650.0).tracking(0.6);
pub const TITLE: TextStyle = TextStyle::new(17.0, 650.0);
pub const STAT: TextStyle = TextStyle::new(15.0, 650.0).features("'tnum' 1");
pub const MONO: TextStyle = TextStyle::new(11.5, 500.0).features("'tnum' 1");

pub fn hx(c: u32) -> Color {
    let ch = |v: u32| rune::color::srgb_decode((v & 0xff) as f32 / 255.0);
    Color { r: ch(c >> 16), g: ch(c >> 8), b: ch(c), a: 1.0 }
}

#[derive(Clone, Copy)]
pub struct Look {
    pub dark: bool,
    pub panel: Color,
    pub border: Color,
    pub text: Color,
    pub sub: Color,
    pub faint: Color,
    pub chip: Color,
    pub chip_on: Color,
    pub hover: Color,
    pub sep: Color,
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub fail: Color,
    pub blue: Color,
    pub teal: Color,
    pub pink: Color,
    pub shadow: Color,
    pub field: Color,
}

pub fn look(dark: bool) -> Look {
    let t = tok::theme(dark);
    if dark {
        Look {
            dark,
            panel: rune::oklcha(0.215, 0.006, 265.0, 0.72),
            border: Color::WHITE.alpha(0.09),
            text: t.text,
            sub: t.text_secondary,
            faint: rune::oklch(0.56, 0.008, 265.0),
            chip: Color::WHITE.alpha(0.06),
            chip_on: Color::WHITE.alpha(0.16),
            hover: Color::WHITE.alpha(0.08),
            sep: Color::WHITE.alpha(0.07),
            accent: hx(0xd9731a),
            ok: hx(0x3fb46a),
            warn: hx(0xe0a526),
            fail: hx(0xe5534b),
            blue: hx(0x4c8dff),
            teal: hx(0x17a596),
            pink: hx(0xd6336c),
            shadow: Color::BLACK.alpha(0.45),
            field: Color::BLACK.alpha(0.22),
        }
    } else {
        Look {
            dark,
            panel: rune::oklcha(0.985, 0.002, 265.0, 0.74),
            border: Color::BLACK.alpha(0.08),
            text: t.text,
            sub: t.text_secondary,
            faint: rune::oklch(0.6, 0.008, 265.0),
            chip: Color::BLACK.alpha(0.05),
            chip_on: Color::BLACK.alpha(0.12),
            hover: Color::BLACK.alpha(0.06),
            sep: Color::BLACK.alpha(0.07),
            accent: hx(0xd9731a),
            ok: hx(0x2e9b55),
            warn: hx(0xc98a0c),
            fail: hx(0xd23c3c),
            blue: hx(0x2f6fd6),
            teal: hx(0x139c8c),
            pink: hx(0xc2255c),
            shadow: Color::BLACK.alpha(0.18),
            field: Color::WHITE.alpha(0.7),
        }
    }
}

impl Look {
    pub fn status(&self, s: &str) -> Color {
        match s {
            "PASS" => self.ok,
            "WARN" => self.warn,
            "FAIL" => self.fail,
            _ => self.faint,
        }
    }
}

/// Makes `e` a floating frosted panel over the 3D view.
pub fn frost(e: El, l: &Look) -> El {
    e.fill(l.panel).backdrop_blur(22.0).border(1.0, l.border).radius(14.0).shadow(10.0, 36.0, l.shadow)
}

pub fn kbd(l: &Look, k: &str) -> El {
    el().h(16.0).min_w(16.0).px(4.0).radius(4.0).center().border(1.0, l.border).fill(l.chip).child(text(k.to_owned()).font(TextStyle::new(10.0, 600.0)).color(l.sub))
}

pub fn badge(text_: &str, c: Color) -> El {
    el().h(18.0).px(7.0).radius(9.0).center().fill(c.alpha(0.18)).child(text(text_.to_owned()).font(TextStyle::new(10.5, 700.0).tracking(0.4)).color(c))
}

pub fn dot(c: Color, d: f32) -> El {
    el().size(d, d).radius(d * 0.5).fill(c)
}

/// A toolbar button: highlighted when `on`, with an optional key hint.
pub fn tool(l: &Look, key: &str, label: &str, hint: Option<&str>, on: bool, f: impl Fn(&mut App, &mut Tx) + 'static) -> El {
    let mut r = row()
        .key(key.to_owned())
        .h(28.0)
        .px(9.0)
        .gap(6.0)
        .center_y()
        .radius(8.0)
        .fill(if on { l.chip_on } else { Color::TRANSPARENT })
        .hover(l.hover)
        .cursor(Cursor::Pointer)
        .on_press_tx(f);
    if let Some(h) = hint {
        r = r.child(kbd(l, h));
    }
    r.child(text(label.to_owned()).font(LABEL).color(if on { l.text } else { l.sub }))
}

pub fn button(l: &Look, key: &str, label: &str, f: impl Fn(&mut App, &mut Tx) + 'static) -> El {
    row()
        .key(key.to_owned())
        .h(26.0)
        .px(10.0)
        .center()
        .radius(7.0)
        .fill(l.chip)
        .hover(l.hover)
        .cursor(Cursor::Pointer)
        .on_press_tx(f)
        .child(text(label.to_owned()).font(LABEL).color(l.text))
}

pub fn link(l: &Look, key: &str, label: &str, c: Color, f: impl Fn(&mut App, &mut Tx) + 'static) -> El {
    let _ = l;
    el().key(key.to_owned()).py(2.0).cursor(Cursor::Pointer).on_press_tx(f).child(text(label.to_owned()).font(LABEL).color(c))
}

pub fn sep(l: &Look) -> El {
    el().w(1.0).h(18.0).fill(l.sep)
}

/// A horizontal slider over 0..1 that calls `f` with the new value while dragged.
pub fn slider(l: &Look, key: &str, w: f32, value: f32, enabled: bool, f: impl Fn(&mut App, f32) + 'static) -> El {
    let v = value.clamp(0.0, 1.0);
    let knob = el().size(14.0, 14.0).radius(7.0).fill(if enabled { Color::WHITE } else { l.faint }).shadow(1.0, 3.0, Color::BLACK.alpha(0.35)).absolute(v * (w - 14.0), 7.0);
    let fill = el().h(4.0).w((v * (w - 14.0) + 7.0).max(0.0)).radius(2.0).fill(if enabled { l.accent } else { l.chip_on }).absolute(0.0, 12.0);
    let track = el().h(4.0).w(w).radius(2.0).fill(l.chip_on).absolute(0.0, 12.0);
    let mut e = el().key(key.to_owned()).w(w).h(28.0).instant_changes().child(track).child(fill).child(knob);
    if enabled {
        e = e.cursor(Cursor::Pointer).on_drag(move |s: &mut App, d: &mut rune::Drag, tx: &mut Tx| {
            tx.instant();
            f(s, ((d.x - 7.0) / (w - 14.0)).clamp(0.0, 1.0));
        });
    }
    e
}
