//! Chart formatting at draw time: resolved fonts, element boxes, outlines with dashes, value
//! scales (bounds, units, log, reverse), tick marks, markers, smoothed lines, the legend and data
//! label placement.

use deckcraft_color::Rgba;
use deckcraft_model::Shape;
use deckcraft_model::chart::Chart;
use deckcraft_model::chart_format::{AxisFormat, ChartText, Crosses, LabelPos, TickMark, format_number};
use deckcraft_model::resolve::Ctx;
use deckcraft_model::style::{ColorRef, Fill, Line};
use deckcraft_model::text::{Align, Anchor, Bullet, TextBody};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _, Vec2};

use crate::canvas::Canvas;
use crate::color;

/// A chart text element's font, resolved.
#[derive(Clone, Debug)]
pub(super) struct Text {
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: Rgba,
    pub font: Option<String>,
}

impl Text {
    /// Rough width of `s` in this font (chart layout only needs an estimate).
    pub fn width(&self, s: &str) -> f64 {
        s.chars().count() as f64 * self.size * if self.bold { 0.6 } else { 0.55 }
    }
}

/// The chart's own text: [`ChartFormat::text`](deckcraft_model::chart_format::ChartFormat::text)
/// over the chart's text colour, at `size` unless it sets one.
pub(super) fn base_text(rctx: &Ctx, c: &Chart, size: f64) -> (ChartText, Text) {
    let t = c.format.text.over(&ChartText { color: c.text_color.clone(), ..Default::default() });
    let r = Text {
        size: t.size.unwrap_or(size),
        bold: t.bold.unwrap_or(false),
        italic: t.italic.unwrap_or(false),
        color: t.color.as_ref().map(|col| rctx.color(col, None)).unwrap_or(Rgba::rgb(0x59, 0x59, 0x59)),
        font: t.font.clone(),
    };
    (t, r)
}

/// An element's font over the chart's text; `size` and `bold` are the element's defaults.
pub(super) fn text(rctx: &Ctx, el: &ChartText, base: &(ChartText, Text), size: f64, bold: bool) -> Text {
    let (bt, bt_r) = base;
    Text {
        size: el.size.or(bt.size).unwrap_or(size).clamp(1.0, 400.0),
        bold: el.bold.or(bt.bold).unwrap_or(bold),
        italic: el.italic.or(bt.italic).unwrap_or(false),
        color: el.color.as_ref().map(|col| rctx.color(col, None)).unwrap_or(bt_r.color),
        font: el.font.clone().or_else(|| bt.font.clone()),
    }
}

/// Draw one line of chart text in `r`, turned `rot` degrees about the rect's centre.
#[allow(clippy::too_many_arguments)]
pub(super) fn label(ctx: &mut dyn Canvas, rctx: &Ctx, m: Affine, s: &str, r: Rect, t: &Text, align: Align, anchor: Anchor, rot: f64) {
    if s.is_empty() || !r.is_finite() {
        return;
    }
    let mut body = TextBody::from_text(s);
    body.body.inset_l = Some(0.0);
    body.body.inset_r = Some(0.0);
    body.body.inset_t = Some(0.0);
    body.body.inset_b = Some(0.0);
    body.body.anchor = Some(anchor);
    body.body.wrap = Some(false);
    for p in &mut body.paragraphs {
        p.props.align = Some(align);
        p.props.bullet = Some(Bullet::None);
        p.props.margin_left = Some(0.0);
        p.props.indent = Some(0.0);
        for run in &mut p.runs {
            run.props.size = Some(t.size);
            run.props.bold = Some(t.bold);
            run.props.italic = Some(t.italic);
            run.props.font.clone_from(&t.font);
            run.props.fill = Some(Fill::solid(ColorRef::rgb(t.color)));
        }
    }
    let m = if rot != 0.0 && rot.is_finite() {
        let c = r.center().to_vec2();
        m * Affine::translate(c) * Affine::rotate(rot.to_radians()) * Affine::translate(-c)
    } else {
        m
    };
    let tmp = Shape { text: Some(body.clone()), ..Default::default() };
    crate::draw_text(ctx, rctx, &tmp, &body, r, m, &deckcraft_text::NoFields, None);
}

/// Rect for a tick label of width `w` turned `rot` degrees, hanging from `at` (the tick, below a
/// horizontal axis): its end meets the tick, as spreadsheet charts place slanted labels.
pub(super) fn slanted(at: Point, w: f64, size: f64, rot: f64) -> Rect {
    let a = rot.to_radians();
    let half = Vec2::new(w / 2.0 * a.cos(), w / 2.0 * a.sin());
    let drop = Vec2::new(0.0, size * 0.6);
    let c = if rot < 0.0 { at + drop - half } else { at + drop + half };
    Rect::from_center_size(c, (w, size * 1.4))
}

/// Paint a fill over `path`.
pub(super) fn fill(ctx: &mut dyn Canvas, rctx: &Ctx, f: &Fill, path: &BezPath, bounds: Rect, m: Affine) {
    crate::paint::fill_path(ctx, rctx, f, None, path, bounds, m, deckcraft_geom::preset::FillMode::Norm, 1.0, rctx.pres);
}

/// Colour of a line's fill, `None` for no line.
pub(super) fn line_color(rctx: &Ctx, l: Option<&Line>, auto: Rgba) -> Option<Rgba> {
    match l.and_then(|l| l.fill.as_ref()) {
        Some(Fill::None) => None,
        Some(Fill::Solid { color: c }) => Some(rctx.color(c, None)),
        Some(Fill::Gradient(g)) => g.stops.first().map(|s| rctx.color(&s.color, None)).or(Some(auto)),
        Some(Fill::Pattern(p)) => Some(rctx.color(&p.fg, None)),
        _ => Some(auto),
    }
}

/// Stroke `path` with line `l` (colour `auto` and width `width` when it sets none). Nothing for
/// a `noFill` line.
#[allow(clippy::too_many_arguments)]
pub(super) fn stroke(ctx: &mut dyn Canvas, rctx: &Ctx, l: Option<&Line>, auto: Rgba, width: f64, path: &BezPath, m: Affine, round: bool) {
    let Some(col) = line_color(rctx, l, auto) else { return };
    let w = l.and_then(|l| l.width).filter(|w| w.is_finite()).unwrap_or(width).clamp(0.0, 200.0);
    if w <= 0.0 {
        return;
    }
    let mut sk = kurbo::Stroke::new(w);
    if round {
        sk = sk.with_join(kurbo::Join::Round).with_caps(kurbo::Cap::Round);
    }
    if let Some(d) = l.and_then(|l| l.dash.as_ref()) {
        let pat: Vec<f64> = d.pattern().iter().map(|v| v * w.max(0.75)).collect();
        if !pat.is_empty() && pat.iter().all(|v| *v > 0.0) {
            sk = sk.with_dashes(0.0, pat);
        }
    }
    ctx.set_transform(m);
    ctx.set_paint(color(col, 1.0));
    ctx.set_stroke(sk);
    ctx.stroke_path(path);
}

/// A chart element's box: its fill, then its outline.
pub(super) fn element_box(ctx: &mut dyn Canvas, rctx: &Ctx, f: Option<&Fill>, l: Option<&Line>, r: Rect, m: Affine, rounded: bool) {
    let path = if rounded { kurbo::RoundedRect::from_rect(r, r.width().min(r.height()) * 0.04).to_path(0.1) } else { r.to_path(0.1) };
    if let Some(f) = f {
        fill(ctx, rctx, f, &path, r, m);
    }
    if l.is_some() {
        stroke(ctx, rctx, l, Rgba::rgb(0xD9, 0xD9, 0xD9), 0.75, &path, m, false);
    }
}

/// A value axis scale.
#[derive(Clone, Debug)]
pub(super) struct Scale {
    pub min: f64,
    pub max: f64,
    pub major: f64,
    pub minor: f64,
    pub log: Option<f64>,
    pub reverse: bool,
}

impl Scale {
    /// Where `v` falls, 0 at the start of the axis and 1 at its end.
    pub fn frac(&self, v: f64) -> f64 {
        let f = match self.log {
            Some(b) => {
                let l = |x: f64| x.max(1e-300).ln() / b.ln();
                let span = l(self.max) - l(self.min);
                if span.abs() < 1e-12 { 0.0 } else { (l(v) - l(self.min)) / span }
            }
            None => {
                let span = self.max - self.min;
                if span.abs() < 1e-12 { 0.0 } else { (v - self.min) / span }
            }
        };
        let f = if f.is_finite() { f.clamp(-0.5, 1.5) } else { 0.0 };
        if self.reverse { 1.0 - f } else { f }
    }
    /// Major tick values (at most 200).
    pub fn ticks(&self) -> Vec<f64> {
        self.steps(self.major)
    }
    /// Minor tick values (at most 1000).
    pub fn minor_ticks(&self) -> Vec<f64> {
        match self.log {
            Some(b) => {
                let mut out = vec![];
                for t in self.ticks() {
                    let n = (b.round() as i64).clamp(2, 20);
                    for k in 2..n {
                        let v = t * k as f64;
                        if v < self.max && out.len() < 1000 {
                            out.push(v);
                        }
                    }
                }
                out
            }
            None => self.steps(self.minor),
        }
    }
    fn steps(&self, step: f64) -> Vec<f64> {
        let mut out = vec![];
        match self.log {
            Some(b) => {
                let mut v = self.min;
                while v <= self.max * (1.0 + 1e-9) && out.len() < 200 && v.is_finite() && v > 0.0 {
                    out.push(v);
                    v *= b;
                }
            }
            None => {
                if !(step.is_finite() && step > 0.0) {
                    return vec![self.min, self.max];
                }
                let n = ((self.max - self.min) / step + 1e-9).floor();
                let n = if n.is_finite() { (n as i64).clamp(0, 1000) } else { 0 };
                for k in 0..=n {
                    out.push(self.min + k as f64 * step);
                }
            }
        }
        out
    }
    /// Where the other axis crosses this one.
    pub fn crossing(&self, crosses: Option<Crosses>) -> f64 {
        match crosses {
            Some(Crosses::Min) => self.min,
            Some(Crosses::Max) => self.max,
            Some(Crosses::At { value }) if value.is_finite() => value.clamp(self.min, self.max),
            _ => match self.log {
                Some(_) => self.min,
                None => 0.0f64.clamp(self.min, self.max),
            },
        }
    }
}

/// The value scale of a chart whose data spans `lo..hi`: fixed bounds and units where the axis
/// sets them, "nice" ones otherwise.
#[cfg(test)]
pub(super) fn value_scale(c: &Chart, lo: f64, hi: f64) -> Scale {
    axis_scale(&c.format.val_axis, lo, hi, true, (c.y_min, c.y_max))
}

/// The scale of axis `a` over data spanning `lo..hi`: the `fixed` bounds and the axis' units,
/// base and direction where it sets them, "nice" ones otherwise. `zero`: the axis starts at
/// zero (bars, areas); otherwise only when the data spans more than a sixth of its distance
/// from zero (prices, X values).
pub(super) fn axis_scale(a: &AxisFormat, lo: f64, hi: f64, zero: bool, fixed: (Option<f64>, Option<f64>)) -> Scale {
    let reverse = a.reverse;
    let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi), lo.max(hi)) } else { (0.0, 1.0) };
    let (y_min, y_max) = (fixed.0.filter(|v| v.is_finite()), fixed.1.filter(|v| v.is_finite()));
    if let Some(b) = a.log_base.filter(|b| b.is_finite() && *b >= 2.0) {
        let lo = if lo > 0.0 { lo } else { 1.0 };
        let hi = if hi > lo { hi } else { lo * b };
        let pow = |x: f64, up: bool| {
            let e = x.ln() / b.ln();
            let e = if up { e.ceil() } else { e.floor() };
            b.powf(e.clamp(-300.0, 300.0))
        };
        let min = y_min.filter(|v| *v > 0.0).unwrap_or_else(|| pow(lo, false));
        let max = y_max.filter(|v| *v > min).unwrap_or_else(|| pow(hi, true).max(min * b));
        return Scale { min, max, major: b, minor: b, log: Some(b), reverse };
    }
    let far = |a: f64, b: f64| (b - a).abs() > a.abs().max(b.abs()) / 6.0;
    let (lo, hi) = if zero || (lo <= 0.0 && hi >= 0.0) || far(lo, hi) { (lo.min(0.0), hi.max(0.0)) } else { (lo, hi) };
    let (nmin, nmax, nstep) = deckcraft_model::chart::nice_scale(lo, hi);
    let min = y_min.unwrap_or(if y_max.is_some_and(|m| m <= nmin) { nmin - nstep * 5.0 } else { nmin });
    let max = y_max.filter(|m| *m > min).unwrap_or(if nmax > min { nmax } else { min + nstep.max(1.0) });
    let step = match a.major_unit.filter(|u| u.is_finite() && *u > 0.0) {
        Some(u) => u.max((max - min) / 1000.0),
        None if y_min.is_some() || y_max.is_some() => deckcraft_model::chart::nice_scale(min, max).2,
        None => nstep,
    };
    let minor = a.minor_unit.filter(|u| u.is_finite() && *u > 0.0).unwrap_or(step / 5.0).max((max - min) / 1000.0);
    Scale { min, max, major: step, minor, log: None, reverse }
}

/// Tick mark segment along a horizontal (`horizontal`) or vertical axis at `at`: `out` points
/// away from the plot.
pub(super) fn tick(path: &mut BezPath, mark: Option<TickMark>, at: Point, horizontal: bool, out: f64, len: f64) {
    let (a, b) = match mark {
        Some(TickMark::Outside) => (0.0, len),
        Some(TickMark::Inside) => (-len, 0.0),
        Some(TickMark::Cross) => (-len, len),
        _ => return,
    };
    let (a, b) = (a * out, b * out);
    if horizontal {
        path.move_to((at.x, at.y + a));
        path.line_to((at.x, at.y + b));
    } else {
        path.move_to((at.x + a, at.y));
        path.line_to((at.x + b, at.y));
    }
}

/// Draw a marker centred on `at`, `size` points across.
#[allow(clippy::too_many_arguments)]
pub(super) fn marker(
    ctx: &mut dyn Canvas,
    rctx: &Ctx,
    symbol: &str,
    size: f64,
    f: Option<&Fill>,
    l: Option<&Line>,
    auto: Rgba,
    at: Point,
    m: Affine,
) {
    let r = (size / 2.0).clamp(1.0, 36.0);
    let mut p = BezPath::new();
    let poly = |p: &mut BezPath, pts: &[(f64, f64)]| {
        for (k, (x, y)) in pts.iter().enumerate() {
            let q = (at.x + x * r, at.y + y * r);
            if k == 0 {
                p.move_to(q);
            } else {
                p.line_to(q);
            }
        }
        p.close_path();
    };
    let mut stroke_only = false;
    match symbol {
        "square" => poly(&mut p, &[(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]),
        "diamond" => poly(&mut p, &[(0.0, -1.2), (1.2, 0.0), (0.0, 1.2), (-1.2, 0.0)]),
        "triangle" => poly(&mut p, &[(0.0, -1.2), (1.1, 0.9), (-1.1, 0.9)]),
        "dash" => poly(&mut p, &[(-1.0, -0.25), (1.0, -0.25), (1.0, 0.25), (-1.0, 0.25)]),
        "dot" => p = kurbo::Circle::new(at, r * 0.5).to_path(0.1),
        "star" | "x" | "plus" => {
            stroke_only = true;
            let segs: &[((f64, f64), (f64, f64))] = match symbol {
                "x" => &[((-1.0, -1.0), (1.0, 1.0)), ((-1.0, 1.0), (1.0, -1.0))],
                "plus" => &[((-1.0, 0.0), (1.0, 0.0)), ((0.0, -1.0), (0.0, 1.0))],
                _ => &[((-1.0, -1.0), (1.0, 1.0)), ((-1.0, 1.0), (1.0, -1.0)), ((0.0, -1.0), (0.0, 1.0))],
            };
            for ((x0, y0), (x1, y1)) in segs {
                p.move_to((at.x + x0 * r, at.y + y0 * r));
                p.line_to((at.x + x1 * r, at.y + y1 * r));
            }
        }
        _ => p = kurbo::Circle::new(at, r).to_path(0.1),
    }
    if !stroke_only {
        match f {
            Some(f) => fill(ctx, rctx, f, &p, p.bounding_box(), m),
            None => {
                ctx.set_transform(m);
                ctx.set_paint(color(auto, 1.0));
                ctx.fill_path(&p);
            }
        }
    }
    if l.is_some() || stroke_only {
        stroke(ctx, rctx, l, auto, if stroke_only { (r * 0.4).max(0.75) } else { 0.75 }, &p, m, true);
    }
}

/// A line through `pts`, smoothed with Catmull-Rom curves when `smooth`.
pub(super) fn polyline(pts: &[Point], smooth: bool) -> BezPath {
    let mut p = BezPath::new();
    let Some(first) = pts.first() else { return p };
    p.move_to(*first);
    if !smooth || pts.len() < 3 {
        for q in pts.iter().skip(1) {
            p.line_to(*q);
        }
        return p;
    }
    let at = |i: isize| -> Point {
        let i = i.clamp(0, pts.len() as isize - 1) as usize;
        pts.get(i).copied().unwrap_or(*first)
    };
    for i in 0..pts.len() as isize - 1 {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let c1 = p1 + (p2 - p0) / 6.0;
        let c2 = p2 - (p3 - p1) / 6.0;
        p.curve_to(c1, c2, p2);
    }
    p
}

/// Text of the data label of one point.
pub(super) fn label_text(c: &Chart, value: f64, percent: Option<f64>, cat: usize, series: &str) -> String {
    let category = c.categories.get(cat).map(String::as_str).unwrap_or("");
    let mut l = c.format.labels.clone();
    if l.num_fmt.is_none() && c.format.val_axis.num_fmt.is_some() && !c.kind.percent() {
        l.num_fmt.clone_from(&c.format.val_axis.num_fmt);
    }
    l.text_for(value, percent, category, series)
}

/// The rect of a data label `w` wide next to a bar `r` (value end at `end_top`/`end_right`).
pub(super) fn bar_label_rect(r: Rect, pos: LabelPos, horiz: bool, negative: bool, w: f64, h: f64) -> Rect {
    let pad = h * 0.15;
    if horiz {
        let (end, base) = if negative { (r.x0, r.x1) } else { (r.x1, r.x0) };
        let dir = if negative { -1.0 } else { 1.0 };
        let cx = match pos {
            LabelPos::Center => r.center().x,
            LabelPos::InsideEnd => end - dir * (w / 2.0 + pad),
            LabelPos::InsideBase => base + dir * (w / 2.0 + pad),
            _ => end + dir * (w / 2.0 + pad),
        };
        Rect::from_center_size((cx, r.center().y), (w, h))
    } else {
        let (end, base) = if negative { (r.y1, r.y0) } else { (r.y0, r.y1) };
        let dir = if negative { 1.0 } else { -1.0 };
        let cy = match pos {
            LabelPos::Center => r.center().y,
            LabelPos::InsideEnd => end - dir * (h / 2.0 + pad),
            LabelPos::InsideBase => base + dir * (h / 2.0 + pad),
            _ => end + dir * (h / 2.0 + pad),
        };
        Rect::from_center_size((r.center().x, cy), (w.max(r.width()), h))
    }
}

/// The rect of a data label next to point `q`.
pub(super) fn point_label_rect(q: Point, pos: LabelPos, w: f64, h: f64, gap: f64) -> Rect {
    let c = match pos {
        LabelPos::Center => q,
        LabelPos::Below => Point::new(q.x, q.y + gap + h / 2.0),
        LabelPos::Left => Point::new(q.x - gap - w / 2.0, q.y),
        LabelPos::Right => Point::new(q.x + gap + w / 2.0, q.y),
        _ => Point::new(q.x, q.y - gap - h / 2.0),
    };
    Rect::from_center_size(c, (w, h))
}

/// One legend entry: name, colour, and whether it shows a line (line charts) rather than a box,
/// with the series' marker on it.
pub(super) struct Entry {
    pub name: String,
    pub color: Rgba,
    pub line: bool,
    pub marker: Option<KeyMarker>,
}

/// The marker on a legend entry's line.
pub(super) struct KeyMarker {
    pub symbol: String,
    pub color: Rgba,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
}

/// Lay out and draw the legend at the chart's legend position, taking its room from `area`
/// unless it overlays the plot.
#[allow(clippy::too_many_arguments)]
pub(super) fn legend(ctx: &mut dyn Canvas, rctx: &Ctx, c: &Chart, items: &[Entry], area: &mut Rect, t: &Text, m: Affine, w: f64) {
    let Some(pos) = c.legend.as_deref() else { return };
    if items.is_empty() {
        return;
    }
    let f = &c.format;
    let key = t.size * 0.7;
    let row_h = t.size * 1.35;
    let item_w = |e: &Entry| key + t.size * 0.4 + t.width(&e.name);
    let vertical = matches!(pos, "l" | "r" | "tr");
    let gap = t.size * 0.8;
    // Lay out: (x, y) of every entry relative to the legend's top left, and its size.
    let mut places = vec![];
    let (lw, lh) = if vertical {
        let lw = items.iter().map(item_w).fold(0.0, f64::max);
        for k in 0..items.len() {
            places.push((0.0, k as f64 * row_h));
        }
        (lw, row_h * items.len() as f64)
    } else {
        let max_w = (area.width() * 0.95).max(t.size * 4.0);
        let (mut x, mut y, mut lw) = (0.0, 0.0, 0.0f64);
        for e in items {
            let iw = item_w(e);
            if x > 0.0 && x + iw > max_w {
                x = 0.0;
                y += row_h;
            }
            places.push((x, y));
            x += iw + gap;
            lw = lw.max(x - gap);
        }
        (lw, y + row_h)
    };
    let pad = t.size * 0.3;
    let (bw, bh) = (lw + pad * 2.0, lh + pad * 2.0);
    let origin = match pos {
        "t" => Point::new((w - bw) / 2.0, area.y0),
        "l" => Point::new(area.x0, area.center().y - bh / 2.0),
        "r" => Point::new(area.x1 - bw, area.center().y - bh / 2.0),
        "tr" => Point::new(area.x1 - bw, area.y0),
        _ => Point::new((w - bw) / 2.0, area.y1 - bh),
    };
    if !f.legend_overlay {
        match pos {
            "t" => area.y0 += bh + t.size * 0.5,
            "l" => area.x0 += bw + t.size * 0.5,
            "r" | "tr" => area.x1 -= bw + t.size * 0.5,
            _ => area.y1 -= bh + t.size * 0.5,
        }
    }
    let boxr = Rect::from_origin_size(origin, (bw, bh));
    element_box(ctx, rctx, f.legend_fill.as_ref(), f.legend_line.as_ref(), boxr, m, false);
    for (e, (x, y)) in items.iter().zip(places) {
        let x = origin.x + pad + x;
        let y = origin.y + pad + y;
        ctx.set_transform(m);
        ctx.set_paint(color(e.color, 1.0));
        let mid = y + row_h / 2.0;
        if e.line {
            ctx.set_stroke(kurbo::Stroke::new((t.size * 0.15).max(1.0)));
            let mut p = BezPath::new();
            p.move_to((x, mid));
            p.line_to((x + key, mid));
            ctx.stroke_path(&p);
        } else if e.marker.is_none() {
            ctx.fill_rect(&Rect::new(x, mid - key / 2.0, x + key, mid + key / 2.0));
        }
        if let Some(k) = &e.marker {
            let at = Point::new(x + key / 2.0, mid);
            marker(ctx, rctx, &k.symbol, key * 0.7, k.fill.as_ref(), k.line.as_ref(), k.color, at, m);
        }
        let tx = x + key + t.size * 0.4;
        label(ctx, rctx, m, &e.name, Rect::new(tx, y, tx + t.width(&e.name) + t.size, y + row_h), t, Align::Left, Anchor::Middle, 0.0);
    }
}

/// Number on an axis.
pub(super) fn axis_number(v: f64, fmt: Option<&str>, percent: bool) -> String {
    match fmt {
        Some(f) => format_number(v, Some(f)),
        None if percent => format_number(v, Some("0%")),
        None => {
            if (v - v.round()).abs() < 1e-9 {
                format!("{}", v.round() as i64)
            } else {
                let s = format_number(v, Some("0.##########"));
                if s.is_empty() { format!("{v}") } else { s }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use deckcraft_model::chart::ChartType;

    use super::*;

    #[test]
    fn scales_honour_bounds_units_log_and_reverse() {
        let mut c = Chart::sample(ChartType::Column);
        let s = value_scale(&c, 0.0, 5.1);
        assert!(s.min <= 0.0 && s.max >= 5.1);
        c.y_min = Some(2.0);
        c.y_max = Some(6.0);
        c.format.val_axis.major_unit = Some(0.5);
        let s = value_scale(&c, 0.0, 5.1);
        assert_eq!((s.min, s.max, s.major), (2.0, 6.0, 0.5));
        assert_eq!(s.ticks().len(), 9);
        assert_eq!(s.frac(4.0), 0.5);
        c.format.val_axis.reverse = true;
        assert_eq!(value_scale(&c, 0.0, 5.1).frac(2.0), 1.0);
        let mut c = Chart::sample(ChartType::Column);
        c.format.val_axis.log_base = Some(10.0);
        let s = value_scale(&c, 3.0, 4500.0);
        assert_eq!((s.min, s.max), (1.0, 10_000.0));
        assert_eq!(s.ticks(), vec![1.0, 10.0, 100.0, 1000.0, 10_000.0]);
        assert!((s.frac(100.0) - 0.5).abs() < 1e-9);
        assert!(!s.minor_ticks().is_empty());
        // Hostile units never loop forever.
        let mut c = Chart::sample(ChartType::Column);
        c.format.val_axis.major_unit = Some(1e-300);
        c.format.val_axis.minor_unit = Some(f64::MIN_POSITIVE);
        let s = value_scale(&c, 0.0, 1e300);
        assert!(s.ticks().len() <= 1001 && s.minor_ticks().len() <= 1001);
    }

    #[test]
    fn smooth_lines_pass_through_their_points() {
        let pts = [Point::new(0.0, 0.0), Point::new(10.0, 5.0), Point::new(20.0, 0.0), Point::new(30.0, 8.0)];
        let p = polyline(&pts, true);
        assert_eq!(p.elements().len(), 4);
        assert!(polyline(&[], true).elements().is_empty());
    }
}
