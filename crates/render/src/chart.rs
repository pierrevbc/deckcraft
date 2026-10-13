//! Charts: title, plot area with axes and gridlines, series marks, legend.
//!
//! [`draw`] lays out the frame (chart box, title, legend) with the chart's formatting, then a
//! [`Painter`] fills the plot area for the chart's family: pie and doughnut, pie and bar of pie,
//! radar, scatter and bubble, or category charts (column, bar, line, area, stock, combo). Every
//! family draws with the chart's formatting: fonts, element boxes, axis scales and labels, series
//! fills and lines, markers and data labels ([`format`]).

use deckcraft_color::{ColorTransform, Rgba};
use deckcraft_model::chart::{BarShape, Chart, ChartType, Series};
use deckcraft_model::chart_format::{AxisFormat, Crosses, LabelPos, TickLabelPos, palette_color};
use deckcraft_model::resolve::Ctx;
use deckcraft_model::style::{Fill, Line};
use deckcraft_model::text::{Align, Anchor};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _};

use crate::canvas::Canvas;
use crate::color;

mod format;
use format::{Entry, KeyMarker, Scale, Text, label};

/// Colour of series/point `i`: the chart's palette (theme accents, then darker and lighter
/// rounds).
pub fn series_color(ctx: &Ctx, c: &Chart, i: usize) -> Rgba {
    let n = if c.vary_colors && c.series.len() <= 1 {
        c.categories.len().max(c.series.first().map(|s| s.values.len()).unwrap_or(0))
    } else {
        c.series.len()
    };
    ctx.color(&palette_color(&c.palette, i, n), None)
}

/// The main colour of a fill: its colour, a gradient's last stop, a pattern's foreground.
fn fill_color(ctx: &Ctx, f: &Fill) -> Option<Rgba> {
    match f {
        Fill::Solid { color: cc } => Some(ctx.color(cc, None)),
        Fill::Gradient(g) => g.stops.last().map(|st| ctx.color(&st.color, None)),
        Fill::Pattern(p) => Some(ctx.color(&p.fg, None)),
        _ => None,
    }
}

/// The colour of series `i`: its own fill's, or its palette colour.
fn series_fill(ctx: &Ctx, c: &Chart, i: usize, s: Option<&Series>) -> Rgba {
    s.and_then(|s| s.fill.as_ref()).and_then(|f| fill_color(ctx, f)).unwrap_or_else(|| series_color(ctx, c, i))
}

/// Colour of point `i` of a series whose points vary in colour: its own fill's, else the
/// palette's.
fn point_color(rctx: &Ctx, c: &Chart, s: Option<&Series>, i: usize) -> Rgba {
    s.and_then(|s| s.point_fills.iter().find(|(k, _)| *k == i)).and_then(|(_, f)| fill_color(rctx, f)).unwrap_or_else(|| series_color(rctx, c, i))
}

/// Fill a mark with its own fill (gradients too), else `auto`.
fn paint_mark(ctx: &mut dyn Canvas, rctx: &Ctx, own: Option<&Fill>, auto: Rgba, path: &BezPath, m: Affine) {
    match own {
        Some(Fill::None) => {}
        Some(f @ (Fill::Gradient(_) | Fill::Pattern(_) | Fill::Solid { .. })) => format::fill(ctx, rctx, f, path, path.bounding_box(), m),
        _ => {
            ctx.set_transform(m);
            ctx.set_paint(color(auto, 1.0));
            ctx.fill_path(path);
        }
    }
}

/// The marker a series drawn as type `k` shows: the type says whether markers show, the series
/// which symbol (a circle by default; `"none"` hides them).
fn marker_symbol(k: ChartType, s: &Series) -> Option<&str> {
    if !k.markers() {
        return None;
    }
    match s.marker.as_deref() {
        Some("none") => None,
        Some("auto" | "") | None => Some("circle"),
        Some(sym) => Some(sym),
    }
}

/// Where the legend goes for `c:legendPos`, and the plot area left over.
/// `size` is the legend block (width, height); the result is its top-left corner and the plot area.
/// `r` (also the OOXML default and the fallback for unknown values) and `l` are vertical stacks centred
/// on the area, `tr` is a vertical stack in the top-right corner, `t` and `b` are centred rows.
/// An overlaid legend leaves the plot area as it was.
fn legend_layout(pos: &str, size: (f64, f64), area: Rect, base: f64, overlay: bool) -> (Point, Rect) {
    let (lw, lh) = size;
    let gap = base * 0.5;
    let cx = (area.x0 + area.x1) / 2.0;
    let cy = ((area.y0 + area.y1) / 2.0 - lh / 2.0).max(area.y0);
    let mut plot = area;
    let origin = match pos {
        "b" => {
            plot.y1 = area.y1 - lh - gap;
            Point::new(cx - lw / 2.0, area.y1 - lh)
        }
        "t" => {
            plot.y0 = area.y0 + lh + gap;
            Point::new(cx - lw / 2.0, area.y0)
        }
        "l" => {
            plot.x0 = area.x0 + lw + gap;
            Point::new(area.x0, cy)
        }
        "tr" => {
            plot.x1 = area.x1 - lw - gap;
            Point::new(area.x1 - lw, area.y0)
        }
        _ => {
            plot.x1 = area.x1 - lw - gap;
            Point::new(area.x1 - lw, cy)
        }
    };
    (origin, if overlay { area } else { plot })
}

/// A category label on two lines, split at its middle space or dash ("03.11.2025 - 09.11.2025").
fn split_label(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mid = chars.len() / 2;
    let best = chars.iter().enumerate().filter(|(_, c)| c.is_whitespace()).map(|(i, _)| i).min_by_key(|i| i.abs_diff(mid));
    match best {
        Some(i) if i > 0 && i + 1 < chars.len() => {
            let (a, b) = chars.split_at(i);
            format!("{}\n{}", a.iter().collect::<String>().trim_end(), b.iter().collect::<String>().trim_start())
        }
        _ => name.to_string(),
    }
}

/// As many major units as a value axis `len` long has room for labels (PowerPoint's automatic
/// unit): a label every 1.9 text heights up a vertical axis, every 6 across a horizontal one.
fn max_intervals(len: f64, base: f64, horizontal: bool) -> usize {
    let per = if horizontal { base * 6.0 } else { base * 1.9 };
    let n = (len / per.max(1e-9)).floor();
    if n.is_finite() { n.clamp(2.0, 12.0) as usize } else { 2 }
}

pub fn draw(ctx: &mut dyn Canvas, rctx: &Ctx, c: &Chart, m: Affine, w: f64, h: f64) {
    let f = &c.format;
    // The file's text sizes when it has them (PowerPoint draws chart text at them); otherwise a size
    // that suits the chart.
    let size = |s: Option<f64>| s.filter(|s| s.is_finite()).map(|s| s.clamp(1.0, 400.0));
    let base = size(c.text_size).unwrap_or_else(|| (h.min(w) / 22.0).clamp(7.0, 18.0));
    let title_size = size(c.title_size).unwrap_or(base * 1.4);
    let chart_text = format::base_text(rctx, c, base);
    if c.chart_fill.is_some() || f.chart_line.is_some() {
        format::element_box(ctx, rctx, c.chart_fill.as_ref(), f.chart_line.as_ref(), Rect::new(0.0, 0.0, w, h), m, f.rounded_corners);
    }
    let mut area = Rect::new(w * 0.04, h * 0.04, w * 0.96, h * 0.96);
    if let Some(t) = &c.title {
        let title_text = deckcraft_model::chart_format::ChartText { size: Some(title_size), ..f.title_text.clone() };
        let tt = format::text(rctx, &title_text, &chart_text, title_size, false);
        let th = tt.size * 1.3;
        let tw = (tt.width(t) + tt.size).min(w);
        let r = Rect::new((w - tw) / 2.0, area.y0, (w + tw) / 2.0, area.y0 + th);
        if f.title_fill.is_some() || f.title_line.is_some() {
            format::element_box(ctx, rctx, f.title_fill.as_ref(), f.title_line.as_ref(), r.inflate(tt.size * 0.2, 0.0), m, false);
        }
        label(ctx, rctx, m, t, Rect::new(0.0, area.y0, w, area.y0 + th), &tt, Align::Center, Anchor::Top, 0.0);
        if !f.title_overlay {
            area.y0 += th + tt.size * 0.45;
        }
    }
    let pie = c.kind.is_pie();
    let vary_points = c.vary_colors && c.series.len() <= 1 && !c.kind.is_stock() && c.kind != ChartType::Combo;
    // A pie, or a single series whose points vary in colour, lists its categories; other
    // charts list their series, a line (and marker) for the series drawn as lines.
    let legend_items: Vec<Entry> = if pie || vary_points {
        let s = c.series.first();
        c.categories
            .iter()
            .enumerate()
            .map(|(i, n)| Entry { name: n.clone(), color: point_color(rctx, c, s, i), line: false, marker: None })
            .collect()
    } else {
        c.series
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let k = drawn_kind(c, s);
                let lined = k.has_lines() || (k.is_radar() && k != ChartType::FilledRadar);
                let fill = series_fill(rctx, c, i, Some(s));
                if !lined {
                    return Entry { name: s.name.clone(), color: fill, line: false, marker: None };
                }
                let marker = marker_symbol(k, s).map(|sym| KeyMarker {
                    symbol: sym.to_string(),
                    color: fill,
                    fill: s.format.marker_fill.clone(),
                    line: s.format.marker_line.clone(),
                });
                let col = format::line_color(rctx, s.line.as_ref(), fill);
                Entry { name: s.name.clone(), color: col.unwrap_or(fill), line: !k.points_only() && col.is_some(), marker }
            })
            .collect()
    };
    let lt = format::text(rctx, &f.legend_text, &chart_text, base, false);
    format::legend(ctx, rctx, c, &legend_items, &mut area, &lt, m);
    let mut p = Painter::new(ctx, rctx, c, m, base, &chart_text);
    if pie {
        if c.kind.is_of_pie() {
            p.of_pie(area);
        } else {
            p.pie(area);
        }
    } else if c.kind.is_radar() {
        p.radar(area);
    } else if c.kind.is_xy() {
        p.xy(area);
    } else {
        p.category(area);
    }
}

/// How a series of a category chart is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Class {
    Bar,
    Line,
    Area,
}

/// The type a series of a category chart is drawn as: its own in a combo chart
/// ([`Chart::series_kind`]), as columns or lines for the types a combination can't hold.
fn drawn_kind(c: &Chart, s: &Series) -> ChartType {
    match c.series_kind(s) {
        k if c.kind == ChartType::Combo && (k.is_pie() || k.is_xy()) => ChartType::Column,
        k if c.kind == ChartType::Combo && (k.is_radar() || k.is_stock()) => ChartType::Line,
        k => k,
    }
}

fn class_of(k: ChartType) -> Class {
    if k.is_area() {
        Class::Area
    } else if k.is_line() || k == ChartType::Surface {
        Class::Line
    } else {
        Class::Bar
    }
}

/// Points joined straight, smooth (a Catmull-Rom spline) or in steps (across, then up or down).
fn line_path(pts: &[Point], smooth: bool, stepped: bool) -> BezPath {
    if !stepped {
        return format::polyline(pts, smooth);
    }
    let mut p = BezPath::new();
    let Some(first) = pts.first() else { return p };
    p.move_to(*first);
    for w in pts.windows(2) {
        if let [a, b] = w {
            p.line_to((b.x, a.y));
            p.line_to(*b);
        }
    }
    p
}

/// An annular sector (a pie slice when `r0` is 0) from angle `a0` sweeping `sweep`.
fn sector(cen: Point, r0: f64, r1: f64, a0: f64, sweep: f64) -> BezPath {
    let mut p = BezPath::new();
    let at = |r: f64, a: f64| Point::new(cen.x + r * a.cos(), cen.y + r * a.sin());
    if sweep.abs() >= std::f64::consts::TAU - 1e-9 {
        p.extend(kurbo::Circle::new(cen, r1).to_path(0.1));
        if r0 > 0.0 {
            // A full ring: the inner circle the other way round.
            let inner = kurbo::Circle::new(cen, r0).to_path(0.1);
            p.extend(inner.reverse_subpaths());
        }
        return p;
    }
    p.move_to(at(r1, a0));
    p.extend(kurbo::Arc::new(cen, (r1, r1), a0, sweep, 0.0).append_iter(0.1));
    if r0 > 0.0 {
        p.line_to(at(r0, a0 + sweep));
        p.extend(kurbo::Arc::new(cen, (r0, r0), a0 + sweep, -sweep, 0.0).append_iter(0.1));
    } else {
        p.line_to(cen);
    }
    p.close_path();
    p
}

/// What runs along the category axis.
#[derive(Clone, Debug)]
enum Across {
    /// `n` categories; `edges`: points on the first and last ticks (area charts) rather than in
    /// the middle of their bands.
    Cats { n: usize, edges: bool },
    /// X values (scatter and bubble charts).
    Values(Scale),
}

/// The plot area of a chart with axes, and where values and categories fall in it.
struct Frame {
    plot: Rect,
    horiz: bool,
    across: Across,
    /// Categories: how many, and the width of each one's band.
    n: usize,
    band: f64,
    primary: Scale,
    secondary: Option<Scale>,
    /// The value where the category axis crosses the primary value axis.
    cross_v: f64,
    /// Where the value axis line runs across the category axis.
    val_line_pos: f64,
    /// The value axis sits at the far side (right, or top for bar charts).
    val_far: bool,
}

impl Frame {
    /// Position of `v` along the value axis of scale `sc`.
    fn val(&self, sc: &Scale, v: f64) -> f64 {
        let k = sc.frac(v);
        if self.horiz { self.plot.x0 + k * self.plot.width() } else { self.plot.y1 - k * self.plot.height() }
    }
    fn reverse(&self, c: &Chart) -> bool {
        c.format.cat_axis.reverse
    }
    /// Middle of category `i` (its tick for area charts) along the category axis.
    fn cat_mid(&self, c: &Chart, i: usize) -> f64 {
        let slot = if self.reverse(c) { self.n.saturating_sub(i + 1) } else { i };
        match &self.across {
            Across::Cats { edges: true, .. } if self.n > 1 => self.plot.x0 + self.plot.width() * slot as f64 / (self.n - 1) as f64,
            // Bar charts list categories from the bottom up.
            _ if self.horiz => self.plot.y1 - self.band * (slot as f64 + 0.5),
            _ => self.plot.x0 + self.band * (slot as f64 + 0.5),
        }
    }
    /// The boundary before category `k` (and after the last for `k == n`).
    fn cat_edge(&self, k: usize) -> f64 {
        if self.horiz { self.plot.y1 - self.band * k as f64 } else { self.plot.x0 + self.band * k as f64 }
    }
    /// Position of X value `v` (scatter and bubble charts).
    fn x(&self, v: f64) -> f64 {
        match &self.across {
            Across::Values(xs) => self.plot.x0 + xs.frac(v) * self.plot.width(),
            Across::Cats { .. } => self.plot.x0,
        }
    }
    /// Point `v` of category `i` on scale `sc`.
    fn pt(&self, c: &Chart, sc: &Scale, i: usize, v: f64) -> Point {
        if self.horiz { Point::new(self.val(sc, v), self.cat_mid(c, i)) } else { Point::new(self.cat_mid(c, i), self.val(sc, v)) }
    }
    /// The scale of the primary or secondary value axis.
    fn scale(&self, secondary: bool) -> &Scale {
        if secondary { self.secondary.as_ref().unwrap_or(&self.primary) } else { &self.primary }
    }
    /// Where bars and areas start on the primary or secondary value axis.
    fn base(&self, secondary: bool) -> f64 {
        if secondary { self.scale(true).crossing(None) } else { self.cross_v }
    }
    /// Where the category axis line runs along the value axis.
    fn zero(&self) -> f64 {
        self.val(&self.primary, self.cross_v)
    }
}

/// Fonts of the chart's text elements.
struct Fonts {
    labels: Text,
    cat: Text,
    val: Text,
    cat_title: Text,
    val_title: Text,
    /// The chart's own text colour.
    ink: Rgba,
}

/// Draws one chart: the frame (fill, title, legend) is done; this fills the plot area.
struct Painter<'a, 'b, 'c> {
    ctx: &'a mut dyn Canvas,
    rctx: &'a Ctx<'b>,
    c: &'c Chart,
    m: Affine,
    base: f64,
    t: Fonts,
    grid: Rgba,
}

/// A series of a category chart: its index, the series, the type it is drawn as, and whether it
/// is on the secondary axis.
type Listed<'s> = (usize, &'s Series, ChartType, bool);

impl<'a, 'b, 'c> Painter<'a, 'b, 'c> {
    fn new(
        ctx: &'a mut dyn Canvas,
        rctx: &'a Ctx<'b>,
        c: &'c Chart,
        m: Affine,
        base: f64,
        chart_text: &(deckcraft_model::chart_format::ChartText, Text),
    ) -> Self {
        let f = &c.format;
        let t = Fonts {
            labels: format::text(rctx, &f.labels.text, chart_text, base * 0.9, false),
            cat: format::text(rctx, &f.cat_axis.text, chart_text, base, false),
            val: format::text(rctx, &f.val_axis.text, chart_text, base, false),
            cat_title: format::text(rctx, &f.cat_axis.title_text, chart_text, base, false),
            val_title: format::text(rctx, &f.val_axis.title_text, chart_text, base, false),
            ink: chart_text.1.color,
        };
        Painter { ctx, rctx, c, m, base, t, grid: Rgba::rgb(0xD9, 0xD9, 0xD9) }
    }
    fn fill(&mut self, p: &BezPath, col: Rgba, alpha: f64) {
        self.ctx.set_transform(self.m);
        self.ctx.set_paint(color(col, alpha));
        self.ctx.fill_path(p);
    }
    /// Fill `p` with `own` (gradients too), else `auto`.
    fn paint(&mut self, own: Option<&Fill>, auto: Rgba, p: &BezPath) {
        paint_mark(self.ctx, self.rctx, own, auto, p, self.m);
    }
    /// Stroke `p` with line `l` (dashes, width, colour), else `auto` `width` wide.
    fn line(&mut self, l: Option<&Line>, auto: Rgba, width: f64, p: &BezPath, round: bool) {
        format::stroke(self.ctx, self.rctx, l, auto, width, p, self.m, round);
    }
    fn text(&mut self, s: &str, r: Rect, t: &Text, align: Align, anchor: Anchor, rot: f64) {
        label(self.ctx, self.rctx, self.m, s, r, t, align, anchor, rot);
    }
    fn series_col(&self, i: usize, s: &Series) -> Rgba {
        series_fill(self.rctx, self.c, i, Some(s))
    }
    fn labels_on(&self, s: &Series) -> bool {
        s.data_labels.unwrap_or(self.c.data_labels)
    }
    /// Markers of a series drawn as `k` at `pts`.
    fn markers(&mut self, k: ChartType, s: &Series, pts: &[Point], col: Rgba) {
        let Some(sym) = marker_symbol(k, s) else { return };
        // Unless the series sets one, markers grow with the chart, from 5 to 10 points across.
        let size = s.format.marker_size.map(f64::from).unwrap_or((self.base * 0.6).clamp(5.0, 10.0));
        for q in pts {
            format::marker(self.ctx, self.rctx, sym, size, s.format.marker_fill.as_ref(), s.format.marker_line.as_ref(), col, *q, self.m);
        }
    }
    /// The data label of point `i` (value `v`) of series `s` next to `q`.
    fn point_label(&mut self, s: &Series, v: f64, i: usize, q: Point, auto: LabelPos) {
        let t = format::label_text(self.c, s, v, None, i);
        let lt = self.t.labels.clone();
        let pos = self.c.format.labels.position.unwrap_or(auto);
        let r = format::point_label_rect(q, pos, lt.width(&t) + lt.size * 0.4, lt.size * 1.3, lt.size * 0.3);
        self.text(&t, r, &lt, Align::Center, Anchor::Middle, 0.0);
    }

    /// Pie, exploded pie and doughnut (a ring per series, the first inside): slices from the
    /// first slice angle, exploded by series or by point, a tilt and a rim for 3-D pies.
    fn pie(&mut self, area: Rect) {
        let c = self.c;
        let doughnut = c.kind.is_doughnut();
        let rings: Vec<&Series> = if doughnut { c.series.iter().collect() } else { c.series.first().into_iter().collect() };
        // How far a slice moves out, in radii: its own explosion, its series', else the type's.
        let explode = |s: &Series, i: usize| -> f64 {
            let own = s.format.point_explosion.iter().find(|(k, _)| *k == i).map(|(_, x)| *x);
            let kind = c.kind.exploded().then(|| c.variant.explosion_or_default());
            f64::from(own.or(s.format.explosion).or(kind).unwrap_or(0).min(400)) / 100.0
        };
        let max_x = rings.iter().flat_map(|s| (0..s.values.len().min(100_000)).map(|i| explode(s, i))).fold(0.0, f64::max);
        let outside = rings.iter().any(|s| self.labels_on(s)) && c.format.labels.position == Some(LabelPos::OutsideEnd);
        let room = if outside { 0.8 } else { 0.95 };
        // A 3-D pie is tilted (flattened by the sine of its X rotation) and given a rim.
        let tilt = c.variant.three_d.filter(|_| !doughnut).map(|t| f64::from(t.rot_x.clamp(10, 90)).to_radians().sin());
        let k = tilt.unwrap_or(1.0);
        let full = (area.width() / 2.0).min(area.height() / (2.0 * k + 0.2 * f64::from(u8::from(tilt.is_some())))) * room;
        let depth = if tilt.is_some() { full * 0.2 } else { 0.0 };
        let r = full / (1.0 + max_x);
        let cen = Point::new(area.center().x, area.center().y - depth / 2.0);
        let squash = Affine::translate(cen.to_vec2()) * Affine::scale_non_uniform(1.0, k) * Affine::translate(-cen.to_vec2());
        let hole = if doughnut { r * c.hole_size.clamp(0.1, 0.9) } else { 0.0 };
        let ring_w = (r - hole) / rings.len().max(1) as f64;
        let angle = c.format.first_slice_angle;
        let start = -std::f64::consts::FRAC_PI_2 + if angle.is_finite() { angle.to_radians() } else { 0.0 };
        struct Slice<'s> {
            ce: Point,
            r0: f64,
            r1: f64,
            a0: f64,
            sweep: f64,
            s: &'s Series,
            si: usize,
            i: usize,
            v: f64,
            total: f64,
        }
        let mut slices: Vec<Slice> = vec![];
        for (ri, s) in rings.iter().enumerate() {
            let vals: Vec<f64> = s.values.iter().map(|v| v.filter(|v| v.is_finite()).unwrap_or(0.0).max(0.0)).collect();
            let total: f64 = vals.iter().sum();
            if total <= 0.0 || !total.is_finite() {
                continue;
            }
            let (r0, r1) = (hole + ring_w * ri as f64, hole + ring_w * (ri + 1) as f64);
            let mut a0 = start;
            for (i, v) in vals.iter().enumerate() {
                let sweep = v / total * std::f64::consts::TAU;
                let mid = a0 + sweep / 2.0;
                let off = r * explode(s, i);
                let ce = Point::new(cen.x + off * mid.cos(), cen.y + off * mid.sin());
                slices.push(Slice { ce, r0, r1, a0, sweep, s, si: ri, i, v: *v, total });
                a0 += sweep;
            }
        }
        let vary = c.vary_colors || doughnut;
        let auto = |p: &Self, sl: &Slice| if vary { point_color(p.rctx, c, Some(sl.s), sl.i) } else { series_fill(p.rctx, c, sl.si, Some(sl.s)) };
        if depth > 0.0 {
            // The rim: the front half of each slice's edge (angles 0 to 180° point down).
            let down = Affine::translate((0.0, depth));
            for sl in &slices {
                let (a0, a1) = (sl.a0.rem_euclid(std::f64::consts::TAU), sl.a0.rem_euclid(std::f64::consts::TAU) + sl.sweep);
                for turn in [0.0, std::f64::consts::TAU] {
                    let (a, b) = ((a0 - turn).max(0.0), (a1 - turn).min(std::f64::consts::PI));
                    if b <= a {
                        continue;
                    }
                    let top = squash * kurbo::Arc::new(sl.ce, (sl.r1, sl.r1), a, b - a, 0.0).into_path(0.1);
                    let bottom = down * squash * kurbo::Arc::new(sl.ce, (sl.r1, sl.r1), b, a - b, 0.0).into_path(0.1);
                    let mut rim = top.clone();
                    for el in bottom.elements().iter() {
                        match el {
                            kurbo::PathEl::MoveTo(p) => rim.line_to(*p),
                            other => rim.push(*other),
                        }
                    }
                    rim.close_path();
                    let dark = deckcraft_color::apply(auto(self, sl), &[ColorTransform::Shade(70000)]);
                    self.fill(&rim, dark, 1.0);
                }
            }
        }
        for sl in &slices {
            let path = squash * sector(sl.ce, sl.r0, sl.r1, sl.a0, sl.sweep);
            let own = sl.s.point_fills.iter().find(|(k, _)| *k == sl.i).map(|(_, f)| f).or(if vary { None } else { sl.s.fill.as_ref() });
            let col = auto(self, sl);
            self.paint(own, col, &path);
            self.line(sl.s.line.as_ref(), Rgba::WHITE, 1.0, &path, true);
        }
        for sl in &slices {
            if !self.labels_on(sl.s) {
                continue;
            }
            let t = format::label_text(c, sl.s, sl.v, Some(sl.v / sl.total), sl.i);
            let pos = c.format.labels.position.unwrap_or(LabelPos::BestFit);
            let k = match pos {
                _ if doughnut => (sl.r0 + sl.r1) / 2.0 / sl.r1,
                LabelPos::Center => (hole / r + 1.0) / 2.0,
                LabelPos::InsideEnd => 0.8,
                LabelPos::OutsideEnd => 1.0,
                _ => 0.65,
            };
            let mut lt = self.t.labels.clone();
            let (tw, th) = (lt.width(&t) + lt.size * 0.4, lt.size * 1.3);
            let mid = sl.a0 + sl.sweep / 2.0;
            let mut lp = squash * Point::new(sl.ce.x + sl.r1 * k * mid.cos(), sl.ce.y + sl.r1 * k * mid.sin());
            if pos == LabelPos::OutsideEnd && !doughnut {
                lp.x += (tw / 2.0 + lt.size * 0.3) * mid.cos();
                lp.y += (th / 2.0 + lt.size * 0.2) * mid.sin();
                if c.format.labels.text.color.is_none() {
                    lt.color = self.t.ink;
                }
            } else if c.format.labels.text.color.is_none() {
                lt.color = Rgba::WHITE;
            }
            self.text(&t, Rect::from_center_size(lp, (tw, th)), &lt, Align::Center, Anchor::Middle, 0.0);
        }
    }

    /// Pie of pie and bar of pie: the last points of the series drawn apart, in a second pie or
    /// a stacked bar, and as one slice of the first pie.
    fn of_pie(&mut self, area: Rect) {
        let c = self.c;
        let Some(s) = c.series.first() else { return };
        let vals: Vec<f64> = s.values.iter().map(|v| v.filter(|v| v.is_finite()).unwrap_or(0.0).max(0.0)).collect();
        let n = vals.len();
        let k = c.variant.split_count(n);
        let split = n - k;
        let (main_vals, rest) = vals.split_at(split);
        let other: f64 = rest.iter().sum();
        let total: f64 = main_vals.iter().sum::<f64>() + other;
        if total <= 0.0 || !total.is_finite() {
            return;
        }
        let ratio = f64::from(c.variant.second_size_or_default()) / 100.0;
        let r = (area.height() / 2.0 * 0.9).min(area.width() / (2.0 + 2.0 * ratio + 0.6) * 0.95);
        let cen = Point::new(area.x0 + area.width() * 0.5 - (r * ratio + r * 0.3), area.center().y);
        let cen2 = Point::new(cen.x + r + r * 0.6 + r * ratio, cen.y);
        let vary = c.vary_colors;
        let own = |i: usize| s.point_fills.iter().find(|(k, _)| *k == i).map(|(_, f)| f).or(if vary { None } else { s.fill.as_ref() });
        let auto = |p: &Self, i: usize| if vary { point_color(p.rctx, c, Some(s), i) } else { series_fill(p.rctx, c, 0, Some(s)) };
        // (path, point, value, its share) of every mark, for the labels.
        let mut marks: Vec<(Point, usize, f64)> = vec![];
        // The combined slice faces the second plot, centred on 3 o'clock.
        let other_sweep = other / total * std::f64::consts::TAU;
        let mut a0 = other_sweep / 2.0;
        for (i, v) in main_vals.iter().enumerate() {
            let sweep = v / total * std::f64::consts::TAU;
            let path = sector(cen, 0.0, r, a0, sweep);
            let col = auto(self, i);
            self.paint(own(i), col, &path);
            self.line(s.line.as_ref(), Rgba::WHITE, 1.0, &path, true);
            let mid = a0 + sweep / 2.0;
            marks.push((Point::new(cen.x + r * 0.65 * mid.cos(), cen.y + r * 0.65 * mid.sin()), i, *v));
            a0 += sweep;
        }
        let other_path = sector(cen, 0.0, r, -other_sweep / 2.0, other_sweep);
        let other_col = series_color(self.rctx, c, n);
        self.fill(&other_path, other_col, 1.0);
        self.line(s.line.as_ref(), Rgba::WHITE, 1.0, &other_path, true);
        let rest_total: f64 = rest.iter().sum();
        if rest_total > 0.0 {
            let edge = |a: f64| Point::new(cen.x + r * a.cos(), cen.y + r * a.sin());
            let (top, bottom) = (edge(-other_sweep / 2.0), edge(other_sweep / 2.0));
            let r2 = r * ratio;
            let (t2, b2) = if c.kind == ChartType::BarOfPie {
                let bw = r2 * 0.5;
                let bar = Rect::new(cen2.x - bw / 2.0, cen2.y - r2, cen2.x + bw / 2.0, cen2.y + r2);
                let mut y = bar.y1;
                for (j, v) in rest.iter().enumerate() {
                    let hgt = v / rest_total * bar.height();
                    let seg = Rect::new(bar.x0, y - hgt, bar.x1, y);
                    let col = auto(self, split + j);
                    self.paint(own(split + j), col, &seg.to_path(0.1));
                    self.line(s.line.as_ref(), Rgba::WHITE, 1.0, &seg.to_path(0.1), true);
                    marks.push((seg.center(), split + j, *v));
                    y -= hgt;
                }
                (Point::new(bar.x0, bar.y0), Point::new(bar.x0, bar.y1))
            } else {
                let mut b0 = -std::f64::consts::FRAC_PI_2;
                for (j, v) in rest.iter().enumerate() {
                    let sweep = v / rest_total * std::f64::consts::TAU;
                    let path = sector(cen2, 0.0, r2, b0, sweep);
                    let col = auto(self, split + j);
                    self.paint(own(split + j), col, &path);
                    self.line(s.line.as_ref(), Rgba::WHITE, 1.0, &path, true);
                    let mid = b0 + sweep / 2.0;
                    marks.push((Point::new(cen2.x + r2 * 0.65 * mid.cos(), cen2.y + r2 * 0.65 * mid.sin()), split + j, *v));
                    b0 += sweep;
                }
                (Point::new(cen2.x, cen2.y - r2), Point::new(cen2.x, cen2.y + r2))
            };
            let mut lines = BezPath::new();
            lines.move_to(top);
            lines.line_to(t2);
            lines.move_to(bottom);
            lines.line_to(b2);
            self.line(None, Rgba::rgb(0x80, 0x80, 0x80), 0.75, &lines, true);
        }
        if self.labels_on(s) {
            let mut lt = self.t.labels.clone();
            if c.format.labels.text.color.is_none() {
                lt.color = Rgba::WHITE;
            }
            for (q, i, v) in marks {
                let t = format::label_text(c, s, v, Some(v / total), i);
                let r = Rect::from_center_size(q, (lt.width(&t) + lt.size * 0.4, lt.size * 1.3));
                self.text(&t, r, &lt, Align::Center, Anchor::Middle, 0.0);
            }
        }
    }

    /// Radar (net): a spoke per category, a closed line or filled shape per series.
    fn radar(&mut self, area: Rect) {
        let c = self.c;
        let base = self.base;
        let (cat_ax, val_ax) = (&c.format.cat_axis, &c.format.val_axis);
        let ncat = c.categories.len().max(c.series.iter().map(|s| s.values.len()).max().unwrap_or(0)).min(10_000);
        if ncat == 0 {
            return;
        }
        let (lo, hi) = c.value_range();
        let r = (area.width().min(area.height()) / 2.0 - base * 1.6).max(4.0);
        let sc = format::axis_scale(val_ax, lo, hi, true, (c.y_min, c.y_max, c.y_major), max_intervals(r, base, false), !c.kind.percent());
        let cen = area.center();
        let dir = if cat_ax.reverse { -1.0 } else { 1.0 };
        let angle = |i: usize| -std::f64::consts::FRAC_PI_2 + dir * i as f64 / ncat as f64 * std::f64::consts::TAU;
        let at = |i: usize, f: f64| {
            let a = angle(i);
            Point::new(cen.x + r * f.clamp(0.0, 1.2) * a.cos(), cen.y + r * f.clamp(0.0, 1.2) * a.sin())
        };
        // Web: a polygon per tick, a spoke per category, the value labels up the first spoke.
        let ticks = sc.ticks();
        let val_t = self.t.val.clone();
        for (k, v) in ticks.iter().enumerate() {
            let f = sc.frac(*v);
            let mut web = BezPath::new();
            for i in 0..ncat {
                if i == 0 { web.move_to(at(i, f)) } else { web.line_to(at(i, f)) }
            }
            web.close_path();
            if c.gridlines || k + 1 == ticks.len() {
                let g = self.grid;
                self.line(val_ax.major_grid_line.as_ref(), g, 0.75, &web, false);
            }
            if !c.y_hidden && val_ax.label_pos != Some(TickLabelPos::None) {
                let q = at(0, f);
                let s = format::value_label(c, false, *v, false);
                self.text(
                    &s,
                    Rect::new(q.x - base * 3.2, q.y - base * 0.7, q.x - base * 0.3, q.y + base * 0.7),
                    &val_t,
                    Align::Right,
                    Anchor::Middle,
                    0.0,
                );
            }
        }
        let mut spokes = BezPath::new();
        for i in 0..ncat {
            spokes.move_to(cen);
            spokes.line_to(at(i, 1.0));
        }
        let g = self.grid;
        self.line(cat_ax.major_grid_line.as_ref().or(cat_ax.line.as_ref()), g, 0.75, &spokes, false);
        if !cat_ax.hidden && cat_ax.label_pos != Some(TickLabelPos::None) {
            let cat_t = self.t.cat.clone();
            for (i, name) in c.categories.iter().enumerate().take(ncat) {
                let a = angle(i);
                let q = Point::new(cen.x + (r + base * 0.9) * a.cos(), cen.y + (r + base * 0.9) * a.sin());
                let align = if a.cos() > 0.3 {
                    Align::Left
                } else if a.cos() < -0.3 {
                    Align::Right
                } else {
                    Align::Center
                };
                let rect = match align {
                    Align::Left => Rect::new(q.x, q.y - base, q.x + base * 8.0, q.y + base),
                    Align::Right => Rect::new(q.x - base * 8.0, q.y - base, q.x, q.y + base),
                    _ => Rect::new(q.x - base * 4.0, q.y - base, q.x + base * 4.0, q.y + base),
                };
                self.text(name, rect, &cat_t, align, Anchor::Middle, 0.0);
            }
        }
        for (si, s) in c.series.iter().enumerate() {
            let shown: Vec<(usize, f64)> =
                (0..ncat).filter_map(|i| s.values.get(i).copied().flatten().filter(|v| v.is_finite()).map(|v| (i, v))).collect();
            let pts: Vec<Point> = shown.iter().map(|(i, v)| at(*i, sc.frac(*v))).collect();
            if pts.is_empty() {
                continue;
            }
            let mut shape = line_path(&pts, false, false);
            shape.close_path();
            let col = self.series_col(si, s);
            if c.kind == ChartType::FilledRadar {
                match &s.fill {
                    Some(f) => format::fill(self.ctx, self.rctx, f, &shape, shape.bounding_box(), self.m),
                    None => self.fill(&shape, col, 0.8),
                }
                if s.line.is_some() {
                    self.line(s.line.as_ref(), col, 1.0, &shape, true);
                }
            } else if !c.kind.points_only() {
                self.line(s.line.as_ref(), col, 2.25, &shape, true);
            }
            self.markers(c.kind, s, &pts, col);
            if self.labels_on(s) {
                for (q, (i, v)) in pts.iter().zip(&shown) {
                    self.point_label(s, *v, *i, *q, LabelPos::Above);
                }
            }
        }
    }

    /// Lay out the axes of a chart with a category (or X) axis and a value axis, a secondary
    /// value axis on the other side when there is one: axis titles, gridlines and tick labels.
    /// `pct`: the primary and secondary axes show percentages.
    fn frame(&mut self, area: Rect, across: Across, primary: Scale, secondary: Option<Scale>, pct: (bool, bool)) -> Frame {
        let c = self.c;
        let base = self.base;
        let (cat_ax, val_ax) = (&c.format.cat_axis, &c.format.val_axis);
        let horiz = c.kind.horizontal() && matches!(across, Across::Cats { .. });
        let n = match &across {
            Across::Cats { n, .. } => (*n).max(1),
            Across::Values(_) => 1,
        };
        let (cat_t, val_t) = (self.t.cat.clone(), self.t.val.clone());
        let val_labels = !c.y_hidden && val_ax.label_pos != Some(TickLabelPos::None);
        let sec_labels = !c.y2_hidden;
        let cat_labels = !cat_ax.hidden && cat_ax.label_pos != Some(TickLabelPos::None);
        let ticks = primary.ticks();
        let val_strings: Vec<String> = ticks.iter().map(|v| format::value_label(c, false, *v, pct.0)).collect();
        let sec_ticks = secondary.as_ref().map(Scale::ticks).unwrap_or_default();
        let sec_strings: Vec<String> = sec_ticks.iter().map(|v| format::value_label(c, true, *v, pct.1)).collect();
        let val_w = val_strings.iter().map(|s| val_t.width(s)).fold(0.0, f64::max) + base * 0.5;
        let sec_w = sec_strings.iter().map(|s| val_t.width(s)).fold(0.0, f64::max) + base * 0.5;
        // Category names, or the X axis' numbers, and where they go.
        let cat_strings: Vec<(String, Option<f64>)> = match &across {
            Across::Cats { n, .. } => c.categories.iter().take(*n).map(|s| (s.clone(), None)).collect(),
            Across::Values(xs) => xs.ticks().into_iter().map(|v| (format::x_label(c, v), Some(v))).collect(),
        };
        let cat_rot = cat_ax.rotation.filter(|r| r.is_finite()).unwrap_or(0.0);
        // Category labels wrap onto two lines when they don't fit their band (long date ranges).
        let wraps = !horiz
            && cat_rot == 0.0
            && matches!(across, Across::Cats { .. })
            && cat_strings.iter().any(|(s, _)| cat_t.width(s) > (area.width() * 0.85) / n as f64);
        let cat_strings: Vec<(String, Option<f64>)> =
            if wraps { cat_strings.into_iter().map(|(s, x)| (split_label(&s), x)).collect() } else { cat_strings };
        let longest = |s: &str| s.split('\n').map(|l| cat_t.width(l)).fold(0.0, f64::max);
        let cat_w = cat_strings.iter().map(|(s, _)| longest(s)).fold(0.0, f64::max);
        let cat_h =
            if cat_rot != 0.0 { cat_w * cat_rot.to_radians().sin().abs() + cat_t.size * 1.2 } else { cat_t.size * if wraps { 2.8 } else { 1.6 } };
        // Axis titles take room beside the axes.
        let (cat_title_t, val_title_t) = (self.t.cat_title.clone(), self.t.val_title.clone());
        let (bottom_title, left_title) = if horiz { (&c.axis_titles.1, &c.axis_titles.0) } else { (&c.axis_titles.0, &c.axis_titles.1) };
        let (bottom_tt, left_tt) = if horiz { (&val_title_t, &cat_title_t) } else { (&cat_title_t, &val_title_t) };
        let mut inner = area;
        if let Some(t) = bottom_title {
            let r = Rect::new(inner.x0, inner.y1 - bottom_tt.size * 1.4, inner.x1, inner.y1);
            self.text(t, r, bottom_tt, Align::Center, Anchor::Bottom, 0.0);
            inner.y1 -= bottom_tt.size * 1.6;
        }
        if let Some(t) = left_title {
            let x = inner.x0 + left_tt.size * 0.7;
            let len = left_tt.width(t) + left_tt.size;
            let r = Rect::from_center_size((x, inner.center().y), (len, left_tt.size * 1.4));
            self.text(t, r, left_tt, Align::Center, Anchor::Middle, -90.0);
            inner.x0 += left_tt.size * 1.6;
        }
        // The value axis sits at the far side when it crosses at the last category, or when the
        // categories run in reverse; its labels go with it (to the other side when "high").
        let val_at_end = matches!(val_ax.crosses, Some(Crosses::Max));
        let val_far = val_at_end != cat_ax.reverse;
        let val_labels_far = val_far != (val_ax.label_pos == Some(TickLabelPos::High));
        let (cat_gutter, val_gutter) = if horiz {
            (if cat_labels { cat_w + base * 0.6 } else { base * 0.3 }, if val_labels { val_t.size * 1.6 } else { base * 0.3 })
        } else {
            (if cat_labels { cat_h } else { base * 0.3 }, if val_labels { val_w } else { base * 0.3 })
        };
        let sec_gutter = match (&secondary, horiz) {
            (None, _) => 0.0,
            (Some(_), _) if !sec_labels => base * 0.3,
            (Some(_), true) => val_t.size * 1.6,
            (Some(_), false) => sec_w,
        };
        let most = inner.width().min(inner.height()) * 0.45;
        let (cat_gutter, val_gutter, sec_gutter) = (cat_gutter.min(most), val_gutter.min(most), sec_gutter.min(most));
        let plot = match (horiz, val_labels_far) {
            (true, false) => Rect::new(inner.x0 + cat_gutter, inner.y0 + sec_gutter, inner.x1, inner.y1 - val_gutter),
            (true, true) => Rect::new(inner.x0 + cat_gutter, inner.y0 + val_gutter, inner.x1, inner.y1 - sec_gutter),
            (false, false) => Rect::new(inner.x0 + val_gutter, inner.y0, inner.x1 - sec_gutter, inner.y1 - cat_gutter),
            (false, true) => Rect::new(inner.x0 + sec_gutter, inner.y0, inner.x1 - val_gutter, inner.y1 - cat_gutter),
        };
        if c.plot_fill.is_some() {
            format::element_box(self.ctx, self.rctx, c.plot_fill.as_ref(), None, plot, self.m, false);
        }
        let band = if horiz { plot.height() / n as f64 } else { plot.width() / n as f64 };
        let cross_v = primary.crossing(cat_ax.crosses);
        let mut fr = Frame { plot, horiz, across, n, band, primary, secondary, cross_v, val_line_pos: 0.0, val_far };
        fr.val_line_pos = match (&fr.across, horiz, val_far) {
            (Across::Values(xs), _, _) => fr.x(xs.crossing(val_ax.crosses)),
            (_, true, true) => plot.y0,
            (_, true, false) => plot.y1,
            (_, false, true) => plot.x1,
            (_, false, false) => plot.x0,
        };
        // Gridlines: minor, then major, for both axes. They run across the plot, perpendicular
        // to their axis.
        let grid_col = self.grid;
        let grid = |p: &mut Self, pos: f64, value_axis: bool, l: Option<&Line>, width: f64| {
            let mut g = BezPath::new();
            if value_axis != horiz {
                g.move_to((plot.x0, pos));
                g.line_to((plot.x1, pos));
            } else {
                g.move_to((pos, plot.y0));
                g.line_to((pos, plot.y1));
            }
            p.line(l, grid_col, width, &g, false);
        };
        if val_ax.minor_gridlines {
            for v in fr.primary.minor_ticks() {
                grid(self, fr.val(&fr.primary, v), true, val_ax.minor_grid_line.as_ref(), 0.5);
            }
        }
        if c.gridlines {
            for v in &ticks {
                grid(self, fr.val(&fr.primary, *v), true, val_ax.major_grid_line.as_ref(), 0.75);
            }
        }
        match &fr.across {
            Across::Values(xs) => {
                if cat_ax.minor_gridlines {
                    for v in xs.minor_ticks() {
                        grid(self, fr.x(v), false, cat_ax.minor_grid_line.as_ref(), 0.5);
                    }
                }
                if cat_ax.major_gridlines {
                    for v in xs.ticks() {
                        grid(self, fr.x(v), false, cat_ax.major_grid_line.as_ref(), 0.75);
                    }
                }
            }
            Across::Cats { edges, .. } => {
                let edges = *edges && n > 1;
                if cat_ax.minor_gridlines && !edges {
                    for k in 0..n {
                        grid(self, fr.cat_edge(k) + if horiz { -band / 2.0 } else { band / 2.0 }, false, cat_ax.minor_grid_line.as_ref(), 0.5);
                    }
                }
                if cat_ax.major_gridlines {
                    if edges {
                        for i in 0..n {
                            grid(self, fr.cat_mid(c, i), false, cat_ax.major_grid_line.as_ref(), 0.75);
                        }
                    } else {
                        for k in 0..=n {
                            grid(self, fr.cat_edge(k), false, cat_ax.major_grid_line.as_ref(), 0.75);
                        }
                    }
                }
            }
        }
        // Value labels, the secondary axis' on the other side.
        let val_rot = val_ax.rotation.filter(|r| r.is_finite()).unwrap_or(0.0);
        let value_labels = |p: &mut Self, sc: &Scale, ticks: &[f64], strings: &[String], high: bool, w: f64, rot: f64| {
            for (v, s) in ticks.iter().zip(strings) {
                let pos = fr.val(sc, *v);
                if horiz {
                    let y = if high { plot.y0 - val_t.size * 1.5 } else { plot.y1 + 2.0 };
                    let r = Rect::new(pos - val_t.width(s) / 2.0 - base, y, pos + val_t.width(s) / 2.0 + base, y + val_t.size * 1.5);
                    p.text(s, r, &val_t, Align::Center, Anchor::Top, rot);
                } else if high {
                    let r = Rect::new(plot.x1 + base * 0.4, pos - val_t.size, plot.x1 + w, pos + val_t.size);
                    p.text(s, r, &val_t, Align::Left, Anchor::Middle, rot);
                } else {
                    let r = Rect::new(plot.x0 - w, pos - val_t.size, plot.x0 - base * 0.4, pos + val_t.size);
                    p.text(s, r, &val_t, Align::Right, Anchor::Middle, rot);
                }
            }
        };
        if val_labels {
            let primary = fr.primary.clone();
            value_labels(self, &primary, &ticks, &val_strings, val_labels_far, val_w, val_rot);
        }
        if let Some(sec) = fr.secondary.clone().filter(|_| sec_labels) {
            value_labels(self, &sec, &sec_ticks, &sec_strings, !val_labels_far, sec_w, 0.0);
        }
        // Category labels (or the X axis' numbers).
        if cat_labels {
            let high = cat_ax.label_pos == Some(TickLabelPos::High);
            for (i, (name, x)) in cat_strings.iter().enumerate() {
                let (mid, span) = match x {
                    Some(v) => (fr.x(*v), cat_t.width(name) + cat_t.size),
                    None => (fr.cat_mid(c, i), band),
                };
                if horiz {
                    let r = if high {
                        Rect::new(plot.x1 + base * 0.4, mid - band / 2.0, plot.x1 + cat_w + base, mid + band / 2.0)
                    } else {
                        Rect::new(plot.x0 - cat_w - base * 0.6, mid - band / 2.0, plot.x0 - base * 0.4, mid + band / 2.0)
                    };
                    self.text(name, r, &cat_t, if high { Align::Left } else { Align::Right }, Anchor::Middle, cat_rot);
                } else if cat_rot != 0.0 {
                    let at = Point::new(mid, if high { plot.y0 - cat_h } else { plot.y1 + 2.0 });
                    let r = format::slanted(at, cat_t.width(name) + cat_t.size * 0.3, cat_t.size, cat_rot);
                    self.text(name, r, &cat_t, Align::Center, Anchor::Middle, cat_rot);
                } else {
                    let y = if high { plot.y0 - cat_h } else { plot.y1 + 3.0 };
                    let r = Rect::new(mid - span / 2.0, y, mid + span / 2.0, y + cat_h);
                    self.text(name, r, &cat_t, Align::Center, Anchor::Top, 0.0);
                }
            }
        }
        fr
    }

    /// The plot area's border over the marks, then the axis lines and tick marks.
    fn axis_lines(&mut self, fr: &Frame) {
        let c = self.c;
        let (cat_ax, val_ax) = (&c.format.cat_axis, &c.format.val_axis);
        let plot = fr.plot;
        if c.format.plot_line.is_some() {
            let g = self.grid;
            self.line(c.format.plot_line.as_ref(), g, 0.75, &plot.to_path(0.1), false);
        }
        let tick_len = (self.base * 0.35).max(2.0);
        let auto = Rgba::rgb(0xBF, 0xBF, 0xBF);
        let zero = fr.zero();
        if !cat_ax.hidden {
            let mut ax = BezPath::new();
            let mut tk = BezPath::new();
            if fr.horiz {
                ax.move_to((zero, plot.y0));
                ax.line_to((zero, plot.y1));
                for k in 0..=fr.n {
                    format::tick(&mut tk, cat_ax.major_tick, Point::new(zero, fr.cat_edge(k)), false, -1.0, tick_len);
                }
            } else {
                ax.move_to((plot.x0, zero));
                ax.line_to((plot.x1, zero));
                match &fr.across {
                    Across::Values(xs) => {
                        for v in xs.ticks() {
                            format::tick(&mut tk, cat_ax.major_tick, Point::new(fr.x(v), zero), true, 1.0, tick_len);
                        }
                        for v in xs.minor_ticks() {
                            format::tick(&mut tk, cat_ax.minor_tick, Point::new(fr.x(v), zero), true, 1.0, tick_len * 0.6);
                        }
                    }
                    Across::Cats { edges, .. } if *edges && fr.n > 1 => {
                        for i in 0..fr.n {
                            format::tick(&mut tk, cat_ax.major_tick, Point::new(fr.cat_mid(c, i), zero), true, 1.0, tick_len);
                        }
                    }
                    Across::Cats { .. } => {
                        for k in 0..=fr.n {
                            format::tick(&mut tk, cat_ax.major_tick, Point::new(fr.cat_edge(k), zero), true, 1.0, tick_len);
                        }
                    }
                }
            }
            self.line(cat_ax.line.as_ref(), auto, 0.75, &ax, false);
            self.line(cat_ax.line.as_ref(), auto, 0.75, &tk, false);
        }
        // Scatter charts draw their Y axis line as their X axis line; category charts only when
        // it is formatted.
        let xy = matches!(fr.across, Across::Values(_));
        if !c.y_hidden && (xy || val_ax.line.is_some() || val_ax.major_tick.is_some() || val_ax.minor_tick.is_some()) {
            let mut ax = BezPath::new();
            let mut tk = BezPath::new();
            let pos = fr.val_line_pos;
            let out = if fr.val_far { -1.0 } else { 1.0 };
            let sc = &fr.primary;
            if fr.horiz {
                ax.move_to((plot.x0, pos));
                ax.line_to((plot.x1, pos));
                for v in sc.ticks() {
                    format::tick(&mut tk, val_ax.major_tick, Point::new(fr.val(sc, v), pos), true, out, tick_len);
                }
                for v in sc.minor_ticks() {
                    format::tick(&mut tk, val_ax.minor_tick, Point::new(fr.val(sc, v), pos), true, out, tick_len * 0.6);
                }
            } else {
                ax.move_to((pos, plot.y0));
                ax.line_to((pos, plot.y1));
                for v in sc.ticks() {
                    format::tick(&mut tk, val_ax.major_tick, Point::new(pos, fr.val(sc, v)), false, -out, tick_len);
                }
                for v in sc.minor_ticks() {
                    format::tick(&mut tk, val_ax.minor_tick, Point::new(pos, fr.val(sc, v)), false, -out, tick_len * 0.6);
                }
            }
            if xy || val_ax.line.is_some() {
                self.line(val_ax.line.as_ref(), auto, 0.75, &ax, false);
            }
            self.line(val_ax.line.as_ref(), auto, 0.75, &tk, false);
        }
    }

    /// Scatter and bubble charts: values along both axes.
    fn xy(&mut self, area: Rect) {
        let c = self.c;
        let (cat_ax, val_ax) = (&c.format.cat_axis, &c.format.val_axis);
        let xs_of = |s: &Series| -> Vec<Option<f64>> {
            if s.x_values.is_empty() { (1..=s.values.len().min(100_000)).map(|v| Some(v as f64)).collect() } else { s.x_values.clone() }
        };
        let finite = |v: &Option<f64>| v.filter(|v| v.is_finite());
        let all_x: Vec<f64> = c.series.iter().flat_map(|s| xs_of(s).iter().filter_map(finite).collect::<Vec<_>>()).collect();
        let all_y: Vec<f64> = c.series.iter().flat_map(|s| s.values.iter().filter_map(finite).collect::<Vec<_>>()).collect();
        let range = |v: &[f64]| v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(*x), b.max(*x)));
        let (xl, xh) = range(&all_x);
        let (yl, yh) = range(&all_y);
        let base = self.base;
        let xs = format::axis_scale(
            cat_ax,
            xl,
            xh,
            false,
            (cat_ax.min, cat_ax.max, cat_ax.major_unit),
            max_intervals(area.width() - base * 4.5, base, true),
            true,
        );
        let ys = format::axis_scale(
            val_ax,
            yl,
            yh,
            c.kind == ChartType::Bubble,
            (c.y_min, c.y_max, c.y_major),
            max_intervals(area.height() - base * 1.8, base, false),
            true,
        );
        let fr = self.frame(area, Across::Values(xs), ys, None, (false, false));
        let plot = fr.plot;
        let py = |v: f64| fr.val(&fr.primary, v);
        // Bubbles: the largest a quarter of the plot across, areas in proportion to the sizes.
        let max_size = c.series.iter().flat_map(|s| s.sizes.iter().filter_map(finite)).fold(0.0f64, |a, b| a.max(b.abs()));
        let max_r = plot.width().min(plot.height()) / 8.0;
        let vary = c.vary_colors && c.series.len() <= 1;
        for (si, s) in c.series.iter().enumerate() {
            let col = self.series_col(si, s);
            let pts: Vec<(Point, Option<f64>, f64, usize)> = xs_of(s)
                .iter()
                .zip(&s.values)
                .enumerate()
                .filter_map(|(i, (x, y))| {
                    let (x, y) = (finite(x)?, finite(y)?);
                    Some((Point::new(fr.x(x), py(y)), s.sizes.get(i).copied().flatten(), y, i))
                })
                .collect();
            if c.kind == ChartType::Bubble {
                for (q, size, _, i) in &pts {
                    let size = size.unwrap_or(1.0);
                    if size.is_nan() || size <= 0.0 || max_size <= 0.0 {
                        continue;
                    }
                    let r = (size / max_size).sqrt() * max_r;
                    let b = kurbo::Circle::new(*q, r.max(1.0)).to_path(0.1);
                    let point_fill = s.point_fills.iter().find(|(k, _)| k == i).map(|(_, f)| f);
                    let auto = if vary { point_color(self.rctx, c, Some(s), *i) } else { col };
                    match point_fill.or(if vary { None } else { s.fill.as_ref() }) {
                        Some(f) => self.paint(Some(f), auto, &b),
                        None => self.fill(&b, auto, 0.75),
                    }
                    self.line(s.line.as_ref(), auto, 0.75, &b, false);
                }
            } else {
                let line: Vec<Point> = pts.iter().map(|p| p.0).collect();
                if !c.kind.points_only() && line.len() > 1 {
                    let path = line_path(&line, s.smooth, c.variant.stepped);
                    self.line(s.line.as_ref(), col, 2.25, &path, true);
                }
                self.markers(c.kind, s, &line, col);
            }
            if self.labels_on(s) {
                let auto = if c.kind == ChartType::Bubble { LabelPos::Center } else { LabelPos::Above };
                for (q, _, v, i) in &pts {
                    self.point_label(s, *v, *i, *q, auto);
                }
            }
        }
        self.axis_lines(&fr);
    }

    /// Column, bar, line, area, stock and combo charts: categories along one axis, values along
    /// the other, a secondary value axis on the right when series use it.
    fn category(&mut self, area: Rect) {
        let c = self.c;
        let ncat = c.categories.len().max(c.series.iter().map(|s| s.values.len()).max().unwrap_or(0)).clamp(1, 100_000);
        let stock = c.kind.is_stock();
        let list: Vec<Listed> = c
            .series
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if stock {
                    let volume = c.kind.has_volume() && i == 0;
                    (i, s, if volume { ChartType::Column } else { c.kind }, c.kind.has_volume() && !volume)
                } else {
                    (i, s, drawn_kind(c, s), c.on_secondary(s))
                }
            })
            .collect();
        let has_secondary = list.iter().any(|l| l.3) && list.iter().any(|l| !l.3);
        let on = |sec: bool| list.iter().filter(move |l| l.3 == sec && has_secondary || !has_secondary && !sec);
        let val_ax = &c.format.val_axis;
        let base = self.base;
        let horiz = c.kind.horizontal();
        let axis_len = if horiz { area.width() - base * 4.5 } else { area.height() - base * 1.8 };
        let intervals = max_intervals(axis_len, base, horiz);
        let scale_for = |sec: bool| -> (Scale, bool) {
            let series: Vec<&Listed> = on(sec).collect();
            let axis = if sec { AxisFormat::default() } else { val_ax.clone() };
            let fixed = if sec { (c.y2_min, c.y2_max, c.y2_major) } else { (c.y_min, c.y_max, c.y_major) };
            if series.iter().any(|l| l.2.percent()) {
                return (format::axis_scale(&axis, 0.0, 1.0, true, fixed, intervals, false), true);
            }
            let prices = series.iter().any(|l| l.2.is_stock());
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for i in 0..ncat {
                let mut stacks: [(f64, f64); 3] = [(0.0, 0.0); 3];
                for (_, s, k, _) in &series {
                    let Some(v) = s.values.get(i).copied().flatten().filter(|v| v.is_finite()) else { continue };
                    if k.stacked() {
                        let Some(st) = stacks.get_mut(class_of(*k) as usize) else { continue };
                        if v >= 0.0 {
                            st.0 += v
                        } else {
                            st.1 += v
                        }
                        hi = hi.max(st.0);
                        lo = lo.min(st.1);
                    } else {
                        hi = hi.max(v);
                        lo = lo.min(v);
                    }
                }
            }
            if !lo.is_finite() {
                (lo, hi) = (0.0, 1.0);
            }
            (format::axis_scale(&axis, lo, hi, !prices, fixed, intervals, true), false)
        };
        let (primary, pct1) = scale_for(false);
        let secondary = has_secondary.then(|| scale_for(true));
        let pct2 = secondary.as_ref().is_some_and(|s| s.1);
        // Area charts put their points on the category ticks, from edge to edge.
        let edges = !c.kind.horizontal() && ncat > 1 && list.iter().all(|l| class_of(l.2) == Class::Area);
        let fr = self.frame(area, Across::Cats { n: ncat, edges }, primary, secondary.map(|s| s.0), (pct1, pct2));
        // Percent stacks: the total of each category per (axis, class).
        let total = |sec: bool, class: Class, i: usize| -> f64 {
            list.iter()
                .filter(|l| l.3 == sec && class_of(l.2) == class)
                .map(|l| l.1.values.get(i).copied().flatten().filter(|v| v.is_finite()).unwrap_or(0.0).abs())
                .sum::<f64>()
                .max(1e-9)
        };
        // Areas first, then bars, then lines on top.
        let mut acc: Vec<[f64; 2]> = vec![[0.0; 2]; ncat];
        for (si, s, k, sec) in list.iter().filter(|l| class_of(l.2) == Class::Area) {
            let sc = fr.scale(*sec);
            let mut top = vec![];
            let mut bottom = vec![];
            for i in 0..ncat {
                let v = s.values.get(i).copied().flatten().filter(|v| v.is_finite()).unwrap_or(0.0);
                let v = if k.percent() { v / total(*sec, Class::Area, i) } else { v };
                let b = if k.stacked() { acc.get(i).map_or(0.0, |a| a[usize::from(*sec)]) } else { fr.base(*sec) };
                let v = if k.stacked() { v } else { v - b };
                if k.stacked()
                    && let Some(a) = acc.get_mut(i)
                {
                    a[usize::from(*sec)] = b + v;
                }
                top.push(fr.pt(c, sc, i, b + v));
                bottom.push(fr.pt(c, sc, i, b));
            }
            let mut shape = line_path(&top, false, false);
            for q in bottom.iter().rev() {
                shape.line_to(*q);
            }
            shape.close_path();
            let col = self.series_col(*si, s);
            match &s.fill {
                Some(f) => self.paint(Some(f), col, &shape),
                None => self.fill(&shape, col, 0.85),
            }
            if s.line.is_some() {
                self.line(s.line.as_ref(), col, 1.0, &shape, true);
            }
            if self.labels_on(s) {
                for (i, q) in top.iter().enumerate() {
                    if let Some(v) = s.values.get(i).copied().flatten() {
                        self.point_label(s, v, i, *q, LabelPos::Above);
                    }
                }
            }
        }
        self.bars(&list, &fr, &total);
        let mut acc: Vec<[f64; 2]> = vec![[0.0; 2]; ncat];
        for (si, s, k, sec) in list.iter().filter(|l| class_of(l.2) == Class::Line) {
            let sc = fr.scale(*sec);
            let mut pts = vec![];
            let mut vals = vec![];
            for i in 0..ncat {
                let Some(v) = s.values.get(i).copied().flatten().filter(|v| v.is_finite()) else { continue };
                let shown = if k.percent() { v / total(*sec, Class::Line, i) } else { v };
                let shown = if k.stacked() {
                    let b = acc.get(i).map_or(0.0, |a| a[usize::from(*sec)]);
                    if let Some(a) = acc.get_mut(i) {
                        a[usize::from(*sec)] = b + shown;
                    }
                    b + shown
                } else {
                    shown
                };
                pts.push(fr.pt(c, sc, i, shown));
                vals.push((i, v));
            }
            let col = self.series_col(*si, s);
            if !k.points_only() && pts.len() > 1 {
                let path = line_path(&pts, s.smooth, c.variant.stepped);
                self.line(s.line.as_ref(), col, 2.25, &path, true);
            }
            self.markers(*k, s, &pts, col);
            if self.labels_on(s) {
                for (q, (i, v)) in pts.iter().zip(&vals) {
                    self.point_label(s, *v, *i, *q, LabelPos::Right);
                }
            }
        }
        if stock {
            let sc = fr.scale(c.kind.has_volume()).clone();
            self.stock(&fr, &|i, v| fr.pt(c, &sc, i, v));
        }
        self.axis_lines(&fr);
    }

    /// The bars of a category chart: clustered side by side (with the chart's gap and overlap),
    /// or stacked per axis.
    fn bars(&mut self, list: &[Listed], fr: &Frame, total: &dyn Fn(bool, Class, usize) -> f64) {
        let c = self.c;
        let bars: Vec<&Listed> = list.iter().filter(|l| class_of(l.2) == Class::Bar && !l.2.is_stock()).collect();
        if bars.is_empty() {
            return;
        }
        // Slots side by side in a category: one per series, or one per stack.
        let mut slots: Vec<(bool, Option<usize>)> = vec![];
        let slot_of = |l: &Listed| if l.2.stacked() { (l.3, None) } else { (l.3, Some(l.0)) };
        for l in &bars {
            let key = slot_of(l);
            if !slots.contains(&key) {
                slots.push(key);
            }
        }
        // Bars `bw` wide, each `step` after the last, a gap of `gap` bar widths per category.
        let gap = c.gap_width.clamp(0.0, 5.0);
        let nf = slots.len().max(1) as f64;
        let overlap = if nf > 1.0 && !bars.iter().all(|l| l.2.stacked()) { c.overlap.clamp(-1.0, 1.0) } else { 0.0 };
        let span = (nf - (nf - 1.0) * overlap).max(0.1);
        let band = fr.band;
        let bw = band / (span + gap).max(0.1);
        let step = bw * (1.0 - overlap);
        let occupied = bw * span;
        let three_d = c.variant.three_d.filter(|_| c.kind.is_column_or_bar());
        let vary_points = c.vary_colors && c.series.len() <= 1;
        let (horiz, plot) = (fr.horiz, fr.plot);
        let reverse = c.format.cat_axis.reverse;
        let lab = self.t.labels.clone();
        for i in 0..fr.n {
            let mut stacks: Vec<(bool, f64, f64)> = vec![];
            for l in &bars {
                let (si, s, k, sec) = **l;
                let sc = fr.scale(sec);
                let raw = s.values.get(i).copied().flatten().filter(|v| v.is_finite()).unwrap_or(0.0);
                let v = if k.percent() { raw / total(sec, Class::Bar, i) } else { raw };
                let slot = slots.iter().position(|x| *x == slot_of(l)).unwrap_or(0);
                let start = band * i as f64 + (band - occupied) / 2.0 + step * slot as f64;
                let cross = fr.base(sec);
                let (a, b) = if k.stacked() {
                    let at = match stacks.iter().position(|x| x.0 == sec) {
                        Some(p) => p,
                        None => {
                            stacks.push((sec, 0.0, 0.0));
                            stacks.len() - 1
                        }
                    };
                    let Some(st) = stacks.get_mut(at) else { continue };
                    if v >= 0.0 {
                        st.1 += v;
                        (st.1 - v, st.1)
                    } else {
                        st.2 += v;
                        (st.2, st.2 - v)
                    }
                } else {
                    (cross.min(v), cross.max(v))
                };
                let r = if horiz {
                    Rect::new(fr.val(sc, a), plot.y1 - start - bw, fr.val(sc, b), plot.y1 - start)
                } else {
                    Rect::new(plot.x0 + start, fr.val(sc, b), plot.x0 + start + bw, fr.val(sc, a))
                };
                // Reversed categories mirror the whole cluster, series order included.
                let r = match (reverse, horiz) {
                    (true, true) => Rect::new(r.x0, plot.y0 + plot.y1 - r.y1, r.x1, plot.y0 + plot.y1 - r.y0),
                    (true, false) => Rect::new(plot.x0 + plot.x1 - r.x1, r.y0, plot.x0 + plot.x1 - r.x0, r.y1),
                    _ => r,
                };
                let r = r.abs();
                let path = r.to_path(0.1);
                let point_fill = s.point_fills.iter().find(|(k, _)| *k == i).map(|(_, f)| f);
                let auto = if vary_points { series_color(self.rctx, c, i) } else { series_fill(self.rctx, c, si, Some(s)) };
                let negative = !k.stacked() && raw < cross || k.stacked() && raw < 0.0;
                let own = point_fill.or(if vary_points { None } else { s.fill.as_ref() });
                match three_d {
                    Some(t) => {
                        let col = own.and_then(|f| fill_color(self.rctx, f)).unwrap_or(auto);
                        self.bar_3d(r, col, t.shape, horiz);
                        if s.line.is_some() {
                            self.line(s.line.as_ref(), auto, 0.75, &path, false);
                        }
                    }
                    None if negative && s.format.invert_if_negative && point_fill.is_none() => {
                        self.paint(None, Rgba::WHITE, &path);
                        let l = s.line.clone().unwrap_or_default();
                        self.line(Some(&l), auto, 0.75, &path, false);
                    }
                    None => {
                        self.paint(own, auto, &path);
                        if s.line.is_some() {
                            self.line(s.line.as_ref(), auto, 0.75, &path, false);
                        }
                    }
                }
                if self.labels_on(s) {
                    let t = format::label_text(c, s, raw, None, i);
                    let pos = c.format.labels.position.unwrap_or(if k.stacked() { LabelPos::Center } else { LabelPos::OutsideEnd });
                    let lr = format::bar_label_rect(r, pos, horiz, negative, lab.width(&t) + lab.size * 0.4, lab.size * 1.3);
                    self.text(&t, lr, &lab, Align::Center, Anchor::Middle, 0.0);
                }
            }
        }
    }

    /// A bar of a 3-D chart, drawn with a simple depth: a box with its top and side, a cylinder,
    /// a cone or a pyramid.
    fn bar_3d(&mut self, r: Rect, col: Rgba, shape: BarShape, horiz: bool) {
        let dark = deckcraft_color::apply(col, &[ColorTransform::Shade(70000)]);
        let light = deckcraft_color::apply(col, &[ColorTransform::Tint(75000)]);
        let d = if horiz { r.height() } else { r.width() } * 0.3;
        let poly = |pts: &[Point]| {
            let mut p = BezPath::new();
            for (k, q) in pts.iter().enumerate() {
                if k == 0 { p.move_to(*q) } else { p.line_to(*q) }
            }
            p.close_path();
            p
        };
        match (shape, horiz) {
            (BarShape::Cone | BarShape::ConeToMax | BarShape::Pyramid | BarShape::PyramidToMax, false) => {
                let tip = Point::new(r.center().x, r.y0);
                let front = poly(&[Point::new(r.x0, r.y1), Point::new(r.x1, r.y1), tip]);
                self.fill(&front, col, 1.0);
                if matches!(shape, BarShape::Pyramid | BarShape::PyramidToMax) {
                    let side = poly(&[Point::new(r.x1, r.y1), Point::new(r.x1 + d, r.y1 - d), tip]);
                    self.fill(&side, dark, 1.0);
                }
            }
            (BarShape::Cone | BarShape::ConeToMax | BarShape::Pyramid | BarShape::PyramidToMax, true) => {
                let tip = Point::new(r.x1, r.center().y);
                let front = poly(&[Point::new(r.x0, r.y0), Point::new(r.x0, r.y1), tip]);
                self.fill(&front, col, 1.0);
            }
            (BarShape::Cylinder, false) => {
                let ry = (r.width() * 0.15).min(r.height().max(1.0));
                self.fill(&r.to_path(0.1), col, 1.0);
                self.fill(&kurbo::Ellipse::new(Point::new(r.center().x, r.y1), (r.width() / 2.0, ry), 0.0).to_path(0.1), col, 1.0);
                self.fill(&kurbo::Ellipse::new(Point::new(r.center().x, r.y0), (r.width() / 2.0, ry), 0.0).to_path(0.1), light, 1.0);
            }
            (BarShape::Cylinder, true) => {
                let rx = (r.height() * 0.15).min(r.width().max(1.0));
                self.fill(&r.to_path(0.1), col, 1.0);
                self.fill(&kurbo::Ellipse::new(Point::new(r.x1, r.center().y), (rx, r.height() / 2.0), 0.0).to_path(0.1), light, 1.0);
            }
            (BarShape::Box, false) => {
                let top = poly(&[Point::new(r.x0, r.y0), Point::new(r.x0 + d, r.y0 - d), Point::new(r.x1 + d, r.y0 - d), Point::new(r.x1, r.y0)]);
                let side = poly(&[Point::new(r.x1, r.y0), Point::new(r.x1 + d, r.y0 - d), Point::new(r.x1 + d, r.y1 - d), Point::new(r.x1, r.y1)]);
                self.fill(&side, dark, 1.0);
                self.fill(&top, light, 1.0);
                self.fill(&r.to_path(0.1), col, 1.0);
            }
            (BarShape::Box, true) => {
                let top = poly(&[Point::new(r.x0, r.y0), Point::new(r.x0 + d, r.y0 - d), Point::new(r.x1 + d, r.y0 - d), Point::new(r.x1, r.y0)]);
                let side = poly(&[Point::new(r.x1, r.y0), Point::new(r.x1 + d, r.y0 - d), Point::new(r.x1 + d, r.y1 - d), Point::new(r.x1, r.y1)]);
                self.fill(&top, light, 1.0);
                self.fill(&side, dark, 1.0);
                self.fill(&r.to_path(0.1), col, 1.0);
            }
        }
    }

    /// Stock prices: a high-low line per category, with a close tick, or an up or down bar from
    /// open to close (white when the price rose, dark when it fell).
    fn stock(&mut self, fr: &Frame, at: &dyn Fn(usize, f64) -> Point) {
        let c = self.c;
        let roles = c.stock_series();
        let ink = Rgba::rgb(0x40, 0x40, 0x40);
        let get = |s: Option<&Series>, i: usize| s.and_then(|s| s.values.get(i).copied().flatten()).filter(|v| v.is_finite());
        let half = fr.band * 0.2;
        for i in 0..fr.n {
            let (h, l) = (get(roles.high, i), get(roles.low, i));
            let (o, cl) = (get(roles.open, i), get(roles.close, i));
            let known: Vec<f64> = [h, l, o, cl].into_iter().flatten().collect();
            let hi = known.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let lo = known.iter().copied().fold(f64::INFINITY, f64::min);
            if !hi.is_finite() || !lo.is_finite() {
                continue;
            }
            let mut hl = BezPath::new();
            hl.move_to(at(i, hi));
            hl.line_to(at(i, lo));
            self.line(None, ink, 1.25, &hl, true);
            match (o, cl) {
                (Some(o), Some(cl)) => {
                    let (a, b) = (at(i, o), at(i, cl));
                    let body = Rect::new(a.x - half, a.y.min(b.y), a.x + half, a.y.max(b.y).max(a.y.min(b.y) + 1.0)).to_path(0.1);
                    self.fill(&body, if cl >= o { Rgba::WHITE } else { ink }, 1.0);
                    self.line(None, ink, 1.0, &body, true);
                }
                (None, Some(cl)) => {
                    let q = at(i, cl);
                    let mut tick = BezPath::new();
                    tick.move_to(q);
                    tick.line_to((q.x + half, q.y));
                    self.line(None, ink, 1.5, &tick, true);
                }
                _ => {}
            }
        }
        // Data labels: the closing price.
        if let Some(s) = roles.close.filter(|s| self.labels_on(s)) {
            for i in 0..fr.n {
                if let Some(v) = get(Some(s), i) {
                    self.point_label(s, v, i, at(i, v), LabelPos::Right);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use deckcraft_model::chart::{ChartPreset, ChartVariant};
    use deckcraft_model::chart_format::PALETTES;
    use deckcraft_model::style::{ColorRef, Dash, Gradient, GradientShape, GradientStop};
    use deckcraft_model::{Presentation, Shape, ShapeId, ShapeKind, Xfrm};

    use super::*;
    use crate::{Image, RenderOpts, render_slide};

    const AREA: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);

    #[test]
    fn right_legend_is_a_centred_column_and_narrows_the_plot() {
        let (o, plot) = legend_layout("r", (60.0, 100.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y), (340.0, 100.0));
        assert_eq!((plot.x0, plot.x1, plot.y0, plot.y1), (0.0, 335.0, 0.0, 300.0));
    }

    #[test]
    fn left_top_and_corner_positions() {
        let (o, plot) = legend_layout("l", (60.0, 100.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y, plot.x0, plot.x1), (0.0, 100.0, 65.0, 400.0));
        let (o, plot) = legend_layout("t", (100.0, 14.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y, plot.y0, plot.y1), (150.0, 0.0, 19.0, 300.0));
        let (o, plot) = legend_layout("tr", (60.0, 100.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y, plot.x1), (340.0, 0.0, 335.0));
    }

    #[test]
    fn bottom_legend_keeps_the_old_row_layout() {
        let (o, plot) = legend_layout("b", (100.0, 14.0), AREA, 10.0, false);
        assert_eq!((o.x, o.y, plot.y1, plot.x1), (150.0, 286.0, 281.0, 400.0));
    }

    #[test]
    fn overlay_does_not_shrink_the_plot() {
        let (o, plot) = legend_layout("r", (60.0, 100.0), AREA, 10.0, true);
        assert_eq!(plot, AREA);
        assert_eq!(o.x, 340.0);
    }

    fn render(c: &Chart) -> Image {
        let mut p = Presentation::default();
        if let Some(s) = p.slides.first_mut() {
            std::sync::Arc::make_mut(s).shapes = vec![Shape {
                id: ShapeId(900),
                xfrm: Some(Xfrm::new(40.0, 30.0, 880.0, 480.0)),
                kind: ShapeKind::Chart(Box::new(c.clone())),
                ..Default::default()
            }];
        }
        render_slide(&p, 0, &RenderOpts { scale: 0.5, ..Default::default() })
    }

    fn preset(id: &str) -> Chart {
        let mut c = Chart::sample(ChartType::Column);
        if let Some(p) = ChartPreset::find(id) {
            c = Chart::sample(p.kind);
            p.apply(&mut c);
        }
        c
    }

    /// Pixels that differ clearly between two renderings.
    fn diff(a: &Image, b: &Image) -> usize {
        a.pixels.chunks(4).zip(b.pixels.chunks(4)).filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 40)).count()
    }

    fn ink(img: &Image) -> usize {
        img.pixels.chunks(4).filter(|px| px.iter().take(3).any(|v| *v < 235)).count()
    }

    #[test]
    fn every_gallery_type_draws() {
        for p in ChartPreset::GALLERY {
            let mut c = preset(p.id);
            c.title = None;
            c.legend = None;
            let n = ink(&render(&c));
            assert!(n > 400, "{}: {n} pixels of ink", p.id);
        }
    }

    #[test]
    fn variants_draw_differently() {
        let pairs = [
            ("line", "linePoints"),
            ("line", "lineMarkers"),
            ("line", "lineSmooth"),
            ("line", "lineStepped"),
            ("stackedLine", "percentLine"),
            ("area", "stackedArea"),
            ("stackedArea", "percentArea"),
            ("pie", "explodedPie"),
            ("doughnut", "explodedDoughnut"),
            ("pie", "pieOfPie"),
            ("pieOfPie", "barOfPie"),
            ("scatter", "scatterLines"),
            ("scatterLines", "scatterSmooth"),
            ("scatterLines", "scatterStepped"),
            ("radar", "radarMarkers"),
            ("radar", "radarPoints"),
            ("radar", "filledRadar"),
            ("stock", "stockOhlc"),
            ("stockVhlc", "stockVohlc"),
            ("column", "column3d"),
            ("column3d", "columnCylinder"),
            ("columnCone", "columnCylinder"),
            ("columnLine", "stackedColumnLine"),
            ("columnLine", "columnLineSecondary"),
        ];
        for (a, b) in pairs {
            let d = diff(&render(&preset(a)), &render(&preset(b)));
            assert!(d > 200, "{a} and {b} look the same ({d} pixels apart)");
        }
    }

    /// A stacked area chart is drawn in bands: the first series' band stays visible under the
    /// others.
    #[test]
    fn stacked_areas_do_not_cover_each_other() {
        let mut c = preset("stackedArea");
        c.title = None;
        c.legend = None;
        (c.y_min, c.y_max) = (Some(0.0), Some(15.0));
        let all = render(&c);
        c.series.truncate(1);
        let first = render(&c);
        let first_col = series_color_rgb(&preset("stackedArea"), 0);
        let count = |img: &Image| img.pixels.chunks(4).filter(|px| px.iter().take(3).zip(first_col.iter()).all(|(a, b)| a.abs_diff(*b) < 30)).count();
        // The first band blends with white at 85 %: compare with the same band drawn alone.
        assert!(count(&all) * 10 > count(&first) * 8, "{} vs {}", count(&all), count(&first));
    }

    /// Colour `i` of chart `c`'s palette in the default theme, blended at the area opacity over
    /// white.
    fn series_color_rgb(c: &Chart, i: usize) -> [u8; 3] {
        let p = Presentation::default();
        let ctx = Ctx::for_master(&p, &p.masters[0]);
        let c = series_color(&ctx, c, i);
        let mix = |v: u8| (f64::from(v) * 0.85 + 255.0 * 0.15).round() as u8;
        [mix(c.r), mix(c.g), mix(c.b)]
    }

    #[test]
    fn odd_data_never_panics() {
        let odd: Vec<Box<dyn Fn(&mut Chart)>> = vec![
            Box::new(|c| c.series.clear()),
            Box::new(|c| c.categories.clear()),
            Box::new(|c| {
                c.series.truncate(1);
                if let Some(s) = c.series.first_mut() {
                    s.values.truncate(1);
                }
            }),
            Box::new(|c| c.series.iter_mut().for_each(|s| s.values = vec![Some(f64::NAN), Some(f64::INFINITY), None, Some(-3.0)])),
            Box::new(|c| c.series.iter_mut().for_each(|s| s.values = vec![Some(-1e300), Some(1e300)])),
            Box::new(|c| c.series.iter_mut().for_each(|s| s.values.iter_mut().for_each(|v| *v = Some(0.0)))),
            Box::new(|c| {
                c.series.iter_mut().for_each(|s| {
                    s.x_values = vec![Some(f64::NAN), Some(2.0)];
                    s.sizes = vec![Some(-4.0), None, Some(f64::INFINITY)];
                })
            }),
            Box::new(|c| {
                c.variant = ChartVariant { explosion: u32::MAX, split_pos: u32::MAX, second_size: u32::MAX, ..c.variant.clone() };
                c.hole_size = f64::NAN;
                c.y_min = Some(10.0);
                c.y_max = Some(-10.0);
            }),
            Box::new(|c| {
                c.categories = (0..2000).map(|i| i.to_string()).collect();
                c.series.iter_mut().for_each(|s| s.values = (0..2000).map(|i| Some(f64::from(i % 7))).collect());
            }),
            Box::new(|c| {
                // Hostile formatting: log axis over negative data, tiny units, huge markers and
                // explosions, NaN angles.
                c.format.val_axis.log_base = Some(10.0);
                c.y_major = Some(1e-300);
                (c.y2_min, c.y2_max, c.y2_major) = (Some(f64::NAN), Some(-1e300), Some(-1.0));
                c.y_format = Some("[\"0.E+;;;\\".into());
                c.text_size = Some(f64::NAN);
                c.title_size = Some(1e300);
                c.format.cat_axis.min = Some(f64::NAN);
                c.format.cat_axis.max = Some(-1e300);
                c.format.cat_axis.reverse = true;
                c.format.cat_axis.rotation = Some(f64::INFINITY);
                c.format.first_slice_angle = f64::NAN;
                c.overlap = f64::NAN;
                c.gap_width = f64::INFINITY;
                c.data_labels = true;
                c.series.iter_mut().for_each(|s| {
                    s.format.marker_size = Some(u32::MAX);
                    s.format.explosion = Some(u32::MAX);
                    s.format.point_explosion = vec![(0, u32::MAX), (usize::MAX, 3)];
                    s.format.invert_if_negative = true;
                    s.values = vec![Some(-5.0), Some(0.0), Some(3.0)];
                });
            }),
        ];
        for p in ChartPreset::GALLERY {
            for f in &odd {
                let mut c = preset(p.id);
                f(&mut c);
                let _ = render(&c);
            }
        }
    }

    /// The chart fields upstream's reader fills draw here too: the legend's overlay, the value
    /// axes' number formats, units and deletion (a combo chart's secondary axis too), the text
    /// sizes and the chart's language.
    #[test]
    fn upstream_chart_fields_change_the_picture() {
        let base = preset("columnLineSecondary");
        assert!(base.series.iter().any(|s| base.on_secondary(s)));
        let changes: [(&str, fn(&mut Chart)); 9] = [
            ("legend overlay", |c| {
                c.legend = Some("r".into());
                c.legend_overlay = true;
            }),
            ("value axis format", |c| c.y_format = Some("0 \"pts\"".into())),
            ("value axis unit", |c| c.y_major = Some(2.0)),
            ("value axis hidden", |c| c.y_hidden = true),
            ("secondary axis hidden", |c| c.y2_hidden = true),
            ("secondary axis bounds", |c| (c.y2_min, c.y2_max) = (Some(-50.0), Some(50.0))),
            ("text size", |c| c.text_size = Some(16.0)),
            ("title size", |c| c.title_size = Some(30.0)),
            // The same format in German: only the decimal marks differ.
            ("language", |c| c.lang = Some("de-DE".into())),
        ];
        let mut base = base;
        base.legend = Some("r".into());
        base.title = Some("Sales".into());
        base.y_format = Some("#,##0.0".into());
        let before = render(&base);
        for (what, f) in changes {
            let mut c = base.clone();
            f(&mut c);
            assert!(diff(&before, &render(&c)) > 0, "{what} changes nothing");
        }
    }

    /// Formatting shows on every type: a gradient fill, a dashed thick line, data labels, a log
    /// axis, the legend on the left, and every palette; each changes the picture.
    #[test]
    fn every_gallery_type_takes_the_formatting() {
        let gradient = Fill::Gradient(Gradient {
            stops: vec![
                GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::rgb(0xFF, 0x00, 0x00)) },
                GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgb(0x00, 0x00, 0xFF)) },
            ],
            shape: GradientShape::Linear { angle: 90.0, scaled: false },
            rotate_with_shape: true,
            tile: [0.0; 4],
        });
        let dashed = Line {
            width: Some(4.0),
            dash: Some(Dash::Preset { name: "dash".into() }),
            fill: Some(Fill::solid(ColorRef::rgb(Rgba::rgb(0, 0x80, 0)))),
            ..Default::default()
        };
        for p in ChartPreset::GALLERY {
            let plain = preset(p.id);
            let before = render(&plain);
            let mut c = plain.clone();
            c.series.iter_mut().for_each(|s| {
                s.fill = Some(gradient.clone());
                s.line = Some(dashed.clone());
            });
            c.data_labels = true;
            c.legend = Some("l".into());
            assert!(diff(&before, &render(&c)) > 200, "{}: formatting changes nothing", p.id);
            let mut c = plain.clone();
            c.format.val_axis.log_base = Some(10.0);
            let _ = render(&c);
            for (id, _) in PALETTES {
                let mut c = plain.clone();
                c.palette = (*id).to_string();
                let _ = render(&c);
            }
        }
    }
}
