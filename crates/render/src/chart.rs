//! Charts: title, plot area with axes and gridlines, series marks, legend.

use deckcraft_color::{ColorTransform, Rgba, SchemeSlot};
use deckcraft_model::Shape;
use deckcraft_model::chart::{BarShape, Chart, ChartType, Series, nice_scale};
use deckcraft_model::resolve::Ctx;
use deckcraft_model::style::{ColorRef, Fill};
use deckcraft_model::text::{Align, Anchor, Bullet, TextBody};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _};

use crate::canvas::Canvas;
use crate::color;

const SLOTS: [SchemeSlot; 6] =
    [SchemeSlot::Accent1, SchemeSlot::Accent2, SchemeSlot::Accent3, SchemeSlot::Accent4, SchemeSlot::Accent5, SchemeSlot::Accent6];

/// Colour of series/point `i`: accents, then darker and lighter rounds.
pub fn series_color(ctx: &Ctx, c: &Chart, i: usize) -> Rgba {
    let slot = SLOTS.get(i % 6).copied().unwrap_or(SchemeSlot::Accent1);
    let base = ctx.color(&ColorRef::scheme(slot), None);
    let mono = c.palette.starts_with("monochrome");
    let base = if mono { ctx.color(&ColorRef::scheme(SchemeSlot::Accent1), None) } else { base };
    let round = if mono { i } else { i / 6 };
    match round {
        0 => base,
        1 => deckcraft_color::apply(base, &[ColorTransform::Shade(60000)]),
        2 => deckcraft_color::apply(base, &[ColorTransform::Tint(60000)]),
        n => deckcraft_color::apply(base, &[ColorTransform::LumMod(100000 - (n as i32 % 5) * 12000)]),
    }
}

fn series_fill(ctx: &Ctx, c: &Chart, i: usize, s: Option<&Series>) -> Rgba {
    match s.and_then(|s| s.fill.as_ref()) {
        Some(Fill::Solid { color: cc }) => ctx.color(cc, None),
        _ => series_color(ctx, c, i),
    }
}

fn label(ctx: &mut dyn Canvas, rctx: &Ctx, m: Affine, text: &str, r: Rect, size: f64, align: Align, anchor: Anchor, col: Rgba, bold: bool) {
    let mut body = TextBody::from_text(text);
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
            run.props.size = Some(size);
            run.props.bold = Some(bold);
            run.props.fill = Some(Fill::solid(ColorRef::rgb(col)));
        }
    }
    let tmp = Shape { text: Some(body.clone()), ..Default::default() };
    crate::draw_text(ctx, rctx, &tmp, &body, r, m, &deckcraft_text::NoFields, None);
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

pub fn draw(ctx: &mut dyn Canvas, rctx: &Ctx, c: &Chart, m: Affine, w: f64, h: f64) {
    let text_col = c.text_color.as_ref().map(|t| rctx.color(t, None)).unwrap_or(Rgba::rgb(0x59, 0x59, 0x59));
    let grid_col = Rgba::rgb(0xD9, 0xD9, 0xD9);
    if let Some(f) = &c.chart_fill {
        crate::paint::fill_path(
            ctx,
            rctx,
            f,
            None,
            &Rect::new(0.0, 0.0, w, h).to_path(0.1),
            Rect::new(0.0, 0.0, w, h),
            m,
            deckcraft_geom::preset::FillMode::Norm,
            1.0,
            rctx.pres,
        );
    }
    let base = (h.min(w) / 22.0).clamp(7.0, 18.0);
    let mut area = Rect::new(w * 0.04, h * 0.04, w * 0.96, h * 0.96);
    if let Some(t) = &c.title {
        label(ctx, rctx, m, t, Rect::new(0.0, area.y0, w, area.y0 + base * 1.8), base * 1.4, Align::Center, Anchor::Top, text_col, false);
        area.y0 += base * 2.4;
    }
    let pie = c.kind.is_pie();
    // Legend at the bottom.
    // (name, colour, the series of a line, scatter or radar chart: its key is its line and marker)
    let legend_items: Vec<(String, Rgba, Option<&Series>)> = if pie {
        c.categories.iter().enumerate().map(|(i, n)| (n.clone(), point_color(rctx, c, c.series.first(), i), None)).collect()
    } else {
        c.series
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let k = series_kind(c, s);
                let lined = k.has_lines() || (k.is_radar() && k != ChartType::FilledRadar);
                (s.name.clone(), series_fill(rctx, c, i, Some(s)), lined.then_some(s))
            })
            .collect()
    };
    if c.legend.is_some() && !legend_items.is_empty() {
        let ly = area.y1 - base * 1.4;
        let item_w = (legend_items.iter().map(|(n, _, _)| n.chars().count() as f64 * base * 0.55 + base * 1.6).sum::<f64>()).min(w * 0.9);
        let mut x = (w - item_w) / 2.0;
        for (name, col, lined) in &legend_items {
            ctx.set_transform(m);
            ctx.set_paint(color(*col, 1.0));
            match lined {
                Some(s) => {
                    let k = series_kind(c, s);
                    let mid = Point::new(x + base * 0.35, ly + base * 0.65);
                    if !k.points_only() {
                        let mut key = BezPath::new();
                        key.move_to((x - base * 0.1, mid.y));
                        key.line_to((x + base * 0.8, mid.y));
                        ctx.set_stroke(kurbo::Stroke::new(2.0));
                        ctx.stroke_path(&key);
                    }
                    if k.markers() {
                        ctx.fill_path(&marker_path(s.marker.as_deref(), mid, (base * 0.3).clamp(2.5, 5.0)));
                    }
                }
                None => ctx.fill_rect(&Rect::new(x, ly + base * 0.3, x + base * 0.7, ly + base)),
            }
            let tw = name.chars().count() as f64 * base * 0.55 + base;
            label(ctx, rctx, m, name, Rect::new(x + base, ly, x + base + tw, ly + base * 1.3), base, Align::Left, Anchor::Middle, text_col, false);
            x += tw + base * 0.6;
        }
        area.y1 = ly - base * 0.5;
    }
    let mut p = Painter { ctx, rctx, c, m, base, ink: text_col, grid: grid_col };
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

/// Colour of point `i` of a pie-like series: its own fill, else the palette.
fn point_color(rctx: &Ctx, c: &Chart, s: Option<&Series>, i: usize) -> Rgba {
    s.and_then(|s| s.point_fills.iter().find(|(k, _)| *k == i))
        .and_then(|(_, f)| if let Fill::Solid { color: cc } = f { Some(rctx.color(cc, None)) } else { None })
        .unwrap_or_else(|| series_color(rctx, c, i))
}

/// A value axis: maps values to 0…1 along the plot.
#[derive(Clone, Copy, Debug)]
struct Scale {
    min: f64,
    max: f64,
    step: f64,
}

impl Scale {
    /// Covers `lo..hi`. `zero`: start at zero when the data is all positive (bars, areas); else
    /// only when the data spans more than a sixth of its maximum.
    fn new(lo: f64, hi: f64, zero: bool, fixed: (Option<f64>, Option<f64>)) -> Scale {
        let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi), lo.max(hi)) } else { (0.0, 1.0) };
        let keep_zero = zero || lo <= 0.0 || (hi - lo) > hi.abs() / 6.0;
        let lo = if keep_zero { lo.min(0.0) } else { lo };
        let (min, max, step) = nice_scale(fixed.0.unwrap_or(lo), fixed.1.unwrap_or(hi));
        let min = fixed.0.filter(|v| v.is_finite()).unwrap_or(min);
        let max = fixed.1.filter(|v| v.is_finite() && *v > min).unwrap_or(max);
        Scale { min, max, step }
    }
    fn percent() -> Scale {
        Scale { min: 0.0, max: 1.0, step: 0.2 }
    }
    fn frac(&self, v: f64) -> f64 {
        (v - self.min) / (self.max - self.min).max(1e-12)
    }
    /// The tick values, bounded.
    fn ticks(&self) -> impl Iterator<Item = f64> + '_ {
        let n = (((self.max - self.min) / self.step.max(1e-12)).round() as i64).clamp(1, 50);
        (0..=n).map(move |k| self.min + k as f64 * self.step)
    }
    fn label(&self, v: f64, percent: bool) -> String {
        if percent { format!("{}%", (v * 100.0).round() as i64) } else { fmt_num(v) }
    }
}

/// How a series of a category chart is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Class {
    Bar,
    Line,
    Area,
}

/// The type a series of a category chart is drawn as: its own in a combo chart.
fn series_kind(c: &Chart, s: &Series) -> ChartType {
    if c.kind != ChartType::Combo {
        return c.kind;
    }
    match s.kind.unwrap_or(ChartType::Column) {
        k if k.is_pie() || k.is_xy() || k == ChartType::Combo => ChartType::Column,
        k if k.is_radar() || k.is_stock() => ChartType::Line,
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
    let mut p = BezPath::new();
    let Some(first) = pts.first() else { return p };
    p.move_to(*first);
    for (k, w) in pts.windows(2).enumerate() {
        let (a, b) = match w {
            [a, b] => (*a, *b),
            _ => continue,
        };
        if stepped {
            p.line_to((b.x, a.y));
            p.line_to(b);
        } else if smooth {
            let before = if k == 0 { a } else { pts.get(k - 1).copied().unwrap_or(a) };
            let after = pts.get(k + 2).copied().unwrap_or(b);
            let c1 = a + (b - before) / 6.0;
            let c2 = b - (after - a) / 6.0;
            p.curve_to(c1, c2, b);
        } else {
            p.line_to(b);
        }
    }
    p
}

/// A marker shape centred on `at`, `r` its half size: circle unless `symbol` names another.
fn marker_path(symbol: Option<&str>, at: Point, r: f64) -> BezPath {
    let poly = |pts: &[(f64, f64)]| {
        let mut p = BezPath::new();
        for (k, (x, y)) in pts.iter().enumerate() {
            let q = Point::new(at.x + x * r, at.y + y * r);
            if k == 0 { p.move_to(q) } else { p.line_to(q) }
        }
        p.close_path();
        p
    };
    match symbol.unwrap_or("circle") {
        "square" => Rect::new(at.x - r, at.y - r, at.x + r, at.y + r).to_path(0.1),
        "diamond" => poly(&[(0.0, -1.3), (1.3, 0.0), (0.0, 1.3), (-1.3, 0.0)]),
        "triangle" => poly(&[(0.0, -1.3), (1.2, 1.0), (-1.2, 1.0)]),
        "x" | "star" | "plus" => {
            let t = 0.35;
            if symbol == Some("plus") {
                poly(&[(-t, -1.0), (t, -1.0), (t, -t), (1.0, -t), (1.0, t), (t, t), (t, 1.0), (-t, 1.0), (-t, t), (-1.0, t), (-1.0, -t), (-t, -t)])
            } else {
                let mut p = poly(&[(-1.0, -0.6), (-0.6, -1.0), (1.0, 0.6), (0.6, 1.0)]);
                p.extend(poly(&[(0.6, -1.0), (1.0, -0.6), (-0.6, 1.0), (-1.0, 0.6)]));
                p
            }
        }
        "dash" => Rect::new(at.x - r, at.y - r * 0.3, at.x + r, at.y + r * 0.3).to_path(0.1),
        "dot" => kurbo::Circle::new(at, r * 0.6).to_path(0.1),
        _ => kurbo::Circle::new(at, r).to_path(0.1),
    }
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

/// Draws one chart: the frame (fill, title, legend) is done; this fills the plot area.
struct Painter<'a, 'b, 'c> {
    ctx: &'a mut RenderContext,
    rctx: &'a Ctx<'b>,
    c: &'c Chart,
    m: Affine,
    base: f64,
    ink: Rgba,
    grid: Rgba,
}

impl Painter<'_, '_, '_> {
    fn fill(&mut self, p: &BezPath, col: Rgba, alpha: f64) {
        self.ctx.set_transform(self.m);
        self.ctx.set_paint(color(col, alpha));
        self.ctx.fill_path(p);
    }
    fn stroke(&mut self, p: &BezPath, col: Rgba, width: f64) {
        self.ctx.set_transform(self.m);
        self.ctx.set_paint(color(col, 1.0));
        self.ctx.set_stroke(kurbo::Stroke::new(width).with_join(kurbo::Join::Round).with_caps(kurbo::Cap::Round));
        self.ctx.stroke_path(p);
    }
    fn text(&mut self, t: &str, r: Rect, size: f64, align: Align, anchor: Anchor, col: Rgba) {
        label(self.ctx, self.rctx, self.m, t, r, size, align, anchor, col, false);
    }
    fn series_col(&self, i: usize, s: &Series) -> Rgba {
        series_fill(self.rctx, self.c, i, Some(s))
    }
    /// The colour and width of a series' line.
    fn series_line(&self, i: usize, s: &Series) -> (Rgba, f64) {
        let col = match s.line.as_ref().and_then(|l| l.fill.as_ref()) {
            Some(Fill::Solid { color: cc }) => self.rctx.color(cc, None),
            _ => self.series_col(i, s),
        };
        let width = s.line.as_ref().and_then(|l| l.width).filter(|w| w.is_finite() && *w > 0.0).map_or(2.25, |w| w.min(20.0));
        (col, width)
    }
    fn marker_r(&self) -> f64 {
        (self.base * 0.3).clamp(2.5, 5.0)
    }
    fn markers(&mut self, s: &Series, pts: &[Point], col: Rgba) {
        let r = self.marker_r();
        for q in pts {
            let p = marker_path(s.marker.as_deref(), *q, r);
            self.fill(&p, col, 1.0);
        }
    }
    fn value_label(&mut self, v: f64, q: Point) {
        let base = self.base;
        self.text(&fmt_num(v), Rect::new(q.x - 25.0, q.y - base * 1.4, q.x + 25.0, q.y), base * 0.85, Align::Center, Anchor::Bottom, self.ink);
    }

    /// Pie, exploded pie and doughnut (a ring per series, the first inside).
    fn pie(&mut self, area: Rect) {
        let c = self.c;
        let doughnut = c.kind.is_doughnut();
        let rings: Vec<&Series> = if doughnut { c.series.iter().collect() } else { c.series.first().into_iter().collect() };
        let explode = if c.kind.exploded() { f64::from(c.variant.explosion_or_default()) / 100.0 } else { 0.0 };
        // A 3-D pie is tilted (flattened by the sine of its X rotation) and given a rim.
        let tilt = c.variant.three_d.filter(|_| !doughnut).map(|t| f64::from(t.rot_x.clamp(10, 90)).to_radians().sin());
        let k = tilt.unwrap_or(1.0);
        let depth_of = |full: f64| if tilt.is_some() { full * 0.2 } else { 0.0 };
        let full = (area.width() / 2.0).min(area.height() / (2.0 * k + 0.2 * f64::from(u8::from(tilt.is_some())))) * 0.95;
        let depth = depth_of(full);
        let r = full / (1.0 + explode);
        let cen = Point::new(area.center().x, area.center().y - depth / 2.0);
        let squash = Affine::translate(cen.to_vec2()) * Affine::scale_non_uniform(1.0, k) * Affine::translate(-cen.to_vec2());
        let hole = if doughnut { r * c.hole_size.clamp(0.1, 0.9) } else { 0.0 };
        let ring_w = (r - hole) / rings.len().max(1) as f64;
        // (centre, inner and outer radius, start, sweep, colour, value)
        let mut slices: Vec<(Point, f64, f64, f64, f64, Rgba, f64)> = vec![];
        for (ri, s) in rings.iter().enumerate() {
            let vals: Vec<f64> = s.values.iter().map(|v| v.unwrap_or(0.0).max(0.0)).collect();
            let total: f64 = vals.iter().sum();
            if total <= 0.0 || !total.is_finite() {
                continue;
            }
            let (r0, r1) = (hole + ring_w * ri as f64, hole + ring_w * (ri + 1) as f64);
            let mut a0 = -std::f64::consts::FRAC_PI_2;
            for (i, v) in vals.iter().enumerate() {
                let sweep = v / total * std::f64::consts::TAU;
                let mid = a0 + sweep / 2.0;
                let off = r * explode;
                let ce = Point::new(cen.x + off * mid.cos(), cen.y + off * mid.sin());
                slices.push((ce, r0, r1, a0, sweep, point_color(self.rctx, c, Some(s), i), *v));
                a0 += sweep;
            }
        }
        if depth > 0.0 {
            // The rim: the front half of each slice's edge (angles 0 to 180° point down).
            let down = Affine::translate((0.0, depth));
            for (ce, _, r1, a0, sweep, col, _) in &slices {
                let (a, b) = (a0.max(0.0), (a0 + sweep).min(std::f64::consts::PI));
                if b <= a {
                    continue;
                }
                let top = squash * kurbo::Arc::new(*ce, (*r1, *r1), a, b - a, 0.0).into_path(0.1);
                let bottom = down * squash * kurbo::Arc::new(*ce, (*r1, *r1), b, a - b, 0.0).into_path(0.1);
                let mut rim = top.clone();
                for el in bottom.elements().iter() {
                    match el {
                        kurbo::PathEl::MoveTo(p) => rim.line_to(*p),
                        other => rim.push(*other),
                    }
                }
                rim.close_path();
                let dark = deckcraft_color::apply(*col, &[ColorTransform::Shade(70000)]);
                self.fill(&rim, dark, 1.0);
            }
        }
        for (ce, r0, r1, a0, sweep, col, v) in &slices {
            let path = squash * sector(*ce, *r0, *r1, *a0, *sweep);
            self.fill(&path, *col, 1.0);
            self.stroke(&path, Rgba::WHITE, 1.0);
            if c.data_labels {
                let mid = a0 + sweep / 2.0;
                let lr = if doughnut { (r0 + r1) / 2.0 } else { r1 * 0.65 };
                let lp = squash * Point::new(ce.x + lr * mid.cos(), ce.y + lr * mid.sin());
                let base = self.base;
                self.text(
                    &fmt_num(*v),
                    Rect::new(lp.x - 30.0, lp.y - 10.0, lp.x + 30.0, lp.y + 10.0),
                    base,
                    Align::Center,
                    Anchor::Middle,
                    Rgba::WHITE,
                );
            }
        }
    }

    /// Pie of pie and bar of pie: the last points of the series drawn apart, in a second pie or
    /// a stacked bar, and as one slice of the first pie.
    fn of_pie(&mut self, area: Rect) {
        let c = self.c;
        let Some(s) = c.series.first() else { return };
        let vals: Vec<f64> = s.values.iter().map(|v| v.unwrap_or(0.0).max(0.0)).collect();
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
        // The combined slice faces the second plot, centred on 3 o'clock.
        let other_sweep = other / total * std::f64::consts::TAU;
        let mut a0 = other_sweep / 2.0;
        for (i, v) in main_vals.iter().enumerate() {
            let sweep = v / total * std::f64::consts::TAU;
            let path = sector(cen, 0.0, r, a0, sweep);
            let col = point_color(self.rctx, c, Some(s), i);
            self.fill(&path, col, 1.0);
            self.stroke(&path, Rgba::WHITE, 1.0);
            a0 += sweep;
        }
        let other_path = sector(cen, 0.0, r, -other_sweep / 2.0, other_sweep);
        let other_col = series_color(self.rctx, c, n);
        self.fill(&other_path, other_col, 1.0);
        self.stroke(&other_path, Rgba::WHITE, 1.0);
        let rest_total: f64 = rest.iter().sum();
        if rest_total <= 0.0 {
            return;
        }
        let edge = |a: f64| Point::new(cen.x + r * a.cos(), cen.y + r * a.sin());
        let (top, bottom) = (edge(-other_sweep / 2.0), edge(other_sweep / 2.0));
        let r2 = r * ratio;
        let (t2, b2) = if c.kind == ChartType::BarOfPie {
            let bw = r2 * 0.5;
            let bar = Rect::new(cen2.x - bw / 2.0, cen2.y - r2, cen2.x + bw / 2.0, cen2.y + r2);
            let mut y = bar.y1;
            for (j, v) in rest.iter().enumerate() {
                let hgt = v / rest_total * bar.height();
                let seg = Rect::new(bar.x0, y - hgt, bar.x1, y).to_path(0.1);
                let col = point_color(self.rctx, c, Some(s), split + j);
                self.fill(&seg, col, 1.0);
                self.stroke(&seg, Rgba::WHITE, 1.0);
                y -= hgt;
            }
            (Point::new(bar.x0, bar.y0), Point::new(bar.x0, bar.y1))
        } else {
            let mut b0 = -std::f64::consts::FRAC_PI_2;
            for (j, v) in rest.iter().enumerate() {
                let sweep = v / rest_total * std::f64::consts::TAU;
                let path = sector(cen2, 0.0, r2, b0, sweep);
                let col = point_color(self.rctx, c, Some(s), split + j);
                self.fill(&path, col, 1.0);
                self.stroke(&path, Rgba::WHITE, 1.0);
                b0 += sweep;
            }
            (Point::new(cen2.x, cen2.y - r2), Point::new(cen2.x, cen2.y + r2))
        };
        let mut lines = BezPath::new();
        lines.move_to(top);
        lines.line_to(t2);
        lines.move_to(bottom);
        lines.line_to(b2);
        self.stroke(&lines, Rgba::rgb(0x80, 0x80, 0x80), 0.75);
    }

    /// Radar (net): a spoke per category, a closed line or filled shape per series.
    fn radar(&mut self, area: Rect) {
        let c = self.c;
        let base = self.base;
        let ncat = c.categories.len().max(c.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
        if ncat == 0 {
            return;
        }
        let (lo, hi) = c.value_range();
        let sc = Scale::new(lo, hi, true, (c.y_min, c.y_max));
        let r = (area.width().min(area.height()) / 2.0 - base * 1.6).max(4.0);
        let cen = area.center();
        let angle = |i: usize| -std::f64::consts::FRAC_PI_2 + i as f64 / ncat as f64 * std::f64::consts::TAU;
        let at = |i: usize, f: f64| {
            let a = angle(i);
            Point::new(cen.x + r * f.clamp(0.0, 1.2) * a.cos(), cen.y + r * f.clamp(0.0, 1.2) * a.sin())
        };
        // Web: a polygon per tick, a spoke per category, the value labels up the first spoke.
        let ticks: Vec<f64> = sc.ticks().collect();
        for v in &ticks {
            let f = sc.frac(*v);
            let mut web = BezPath::new();
            for i in 0..ncat {
                if i == 0 { web.move_to(at(i, f)) } else { web.line_to(at(i, f)) }
            }
            web.close_path();
            if c.gridlines || (*v - sc.max).abs() < 1e-9 {
                let g = self.grid;
                self.stroke(&web, g, 0.75);
            }
            let q = at(0, f);
            let col = self.ink;
            self.text(
                &fmt_num(*v),
                Rect::new(q.x - base * 3.2, q.y - base * 0.7, q.x - base * 0.3, q.y + base * 0.7),
                base * 0.85,
                Align::Right,
                Anchor::Middle,
                col,
            );
        }
        let mut spokes = BezPath::new();
        for i in 0..ncat {
            spokes.move_to(cen);
            spokes.line_to(at(i, 1.0));
        }
        let g = self.grid;
        self.stroke(&spokes, g, 0.75);
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
            let col = self.ink;
            self.text(name, rect, base, align, Anchor::Middle, col);
        }
        for (si, s) in c.series.iter().enumerate() {
            let pts: Vec<Point> = (0..ncat).filter_map(|i| s.values.get(i).copied().flatten().map(|v| at(i, sc.frac(v)))).collect();
            if pts.is_empty() {
                continue;
            }
            let mut shape = line_path(&pts, false, false);
            shape.close_path();
            let col = self.series_col(si, s);
            if c.kind == ChartType::FilledRadar {
                self.fill(&shape, col, 0.8);
            } else if !c.kind.points_only() {
                let (lc, lw) = self.series_line(si, s);
                self.stroke(&shape, lc, lw);
            }
            if c.kind.markers() {
                self.markers(s, &pts, col);
            }
            if c.data_labels {
                for (q, v) in pts.iter().zip(s.values.iter().flatten()) {
                    self.value_label(*v, *q);
                }
            }
        }
    }

    /// Scatter and bubble charts: values along both axes.
    fn xy(&mut self, area: Rect) {
        let c = self.c;
        let base = self.base;
        let xs_of = |s: &Series| -> Vec<Option<f64>> {
            if s.x_values.is_empty() { (1..=s.values.len()).map(|v| Some(v as f64)).collect() } else { s.x_values.clone() }
        };
        let finite = |v: &Option<f64>| v.filter(|v| v.is_finite());
        let all_x: Vec<f64> = c.series.iter().flat_map(|s| xs_of(s).iter().filter_map(finite).collect::<Vec<_>>()).collect();
        let all_y: Vec<f64> = c.series.iter().flat_map(|s| s.values.iter().filter_map(finite).collect::<Vec<_>>()).collect();
        let range = |v: &[f64]| v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(*x), b.max(*x)));
        let (xl, xh) = range(&all_x);
        let (yl, yh) = range(&all_y);
        let xs = Scale::new(xl, xh, false, (None, None));
        let ys = Scale::new(yl, yh, c.kind == ChartType::Bubble, (c.y_min, c.y_max));
        let plot = Rect::new(area.x0 + base * 3.0, area.y0, area.x1 - base, area.y1 - base * 1.8);
        if let Some(f) = &c.plot_fill {
            crate::paint::fill_path(
                self.ctx,
                self.rctx,
                f,
                None,
                &plot.to_path(0.1),
                plot,
                self.m,
                deckcraft_geom::preset::FillMode::Norm,
                1.0,
                self.rctx.pres,
            );
        }
        let px = |v: f64| plot.x0 + xs.frac(v) * plot.width();
        let py = |v: f64| plot.y1 - ys.frac(v) * plot.height();
        for v in ys.ticks().collect::<Vec<_>>() {
            let y = py(v);
            if c.gridlines {
                let mut g = BezPath::new();
                g.move_to((plot.x0, y));
                g.line_to((plot.x1, y));
                let gc = self.grid;
                self.stroke(&g, gc, 0.75);
            }
            let col = self.ink;
            self.text(&fmt_num(v), Rect::new(area.x0, y - base, plot.x0 - base * 0.4, y + base), base, Align::Right, Anchor::Middle, col);
        }
        for v in xs.ticks().collect::<Vec<_>>() {
            let x = px(v);
            let col = self.ink;
            self.text(&fmt_num(v), Rect::new(x - 30.0, plot.y1 + 3.0, x + 30.0, plot.y1 + base * 1.6), base, Align::Center, Anchor::Top, col);
        }
        let mut axes = BezPath::new();
        axes.move_to((plot.x0, plot.y0));
        axes.line_to((plot.x0, plot.y1));
        axes.line_to((plot.x1, plot.y1));
        self.stroke(&axes, Rgba::rgb(0xBF, 0xBF, 0xBF), 0.75);
        // Bubbles: the largest a quarter of the plot across, areas in proportion to the sizes.
        let max_size = c.series.iter().flat_map(|s| s.sizes.iter().filter_map(finite)).fold(0.0f64, |a, b| a.max(b.abs()));
        let max_r = plot.width().min(plot.height()) / 8.0;
        for (si, s) in c.series.iter().enumerate() {
            let col = self.series_col(si, s);
            let pts: Vec<(Point, Option<f64>, f64)> = xs_of(s)
                .iter()
                .zip(&s.values)
                .enumerate()
                .filter_map(|(i, (x, y))| {
                    let (x, y) = (finite(x)?, finite(y)?);
                    Some((Point::new(px(x), py(y)), s.sizes.get(i).copied().flatten(), y))
                })
                .collect();
            if c.kind == ChartType::Bubble {
                for (q, size, _) in &pts {
                    let size = size.unwrap_or(1.0);
                    if size.is_nan() || size <= 0.0 || max_size <= 0.0 {
                        continue;
                    }
                    let r = (size / max_size).sqrt() * max_r;
                    let b = kurbo::Circle::new(*q, r.max(1.0)).to_path(0.1);
                    self.fill(&b, col, 0.75);
                    self.stroke(&b, col, 0.75);
                }
            } else {
                let line: Vec<Point> = pts.iter().map(|p| p.0).collect();
                if !c.kind.points_only() && line.len() > 1 {
                    let (lc, lw) = self.series_line(si, s);
                    let path = line_path(&line, s.smooth, c.variant.stepped);
                    self.stroke(&path, lc, lw);
                }
                if c.kind.markers() {
                    self.markers(s, &line, col);
                }
            }
            if c.data_labels {
                for (q, _, v) in &pts {
                    self.value_label(*v, *q);
                }
            }
        }
    }

    /// Column, bar, line, area, stock and combo charts: categories along one axis, values along
    /// the other, a secondary value axis on the right when series use it.
    fn category(&mut self, area: Rect) {
        let c = self.c;
        let base = self.base;
        let ncat = c.categories.len().max(c.series.iter().map(|s| s.values.len()).max().unwrap_or(0)).max(1);
        let stock = c.kind.is_stock();
        // (series index, series, drawn as, on the secondary axis)
        let list: Vec<(usize, &Series, ChartType, bool)> = c
            .series
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if stock {
                    let volume = c.kind.has_volume() && i == 0;
                    (i, s, if volume { ChartType::Column } else { c.kind }, c.kind.has_volume() && !volume)
                } else {
                    (i, s, series_kind(c, s), s.secondary_axis)
                }
            })
            .collect();
        let has_secondary = list.iter().any(|l| l.3) && list.iter().any(|l| !l.3);
        let on = |sec: bool| list.iter().filter(move |l| l.3 == sec && has_secondary || !has_secondary && !sec);
        let scale_for = |sec: bool| -> (Scale, bool) {
            let series: Vec<&(usize, &Series, ChartType, bool)> = on(sec).collect();
            if series.iter().any(|l| l.2.percent()) {
                return (Scale::percent(), true);
            }
            let prices = series.iter().any(|l| l.2.is_stock());
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for i in 0..ncat {
                let mut stacks: [(f64, f64); 3] = [(0.0, 0.0); 3];
                for (_, s, k, _) in &series {
                    let Some(v) = s.values.get(i).copied().flatten().filter(|v| v.is_finite()) else { continue };
                    if k.stacked() {
                        let st = &mut stacks[class_of(*k) as usize];
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
            let fixed = if sec { (None, None) } else { (c.y_min, c.y_max) };
            (Scale::new(lo, hi, !prices, fixed), false)
        };
        let (primary, pct1) = scale_for(false);
        let secondary = has_secondary.then(|| scale_for(true));
        let horiz = c.kind.horizontal();
        let gutter = base * 3.0;
        let right = if secondary.is_some() { gutter } else { 0.0 };
        let plot = if horiz {
            Rect::new(area.x0 + gutter * 1.5, area.y0, area.x1, area.y1 - base * 1.6)
        } else {
            Rect::new(area.x0 + gutter, area.y0, area.x1 - right, area.y1 - base * 1.8)
        };
        if let Some(f) = &c.plot_fill {
            crate::paint::fill_path(
                self.ctx,
                self.rctx,
                f,
                None,
                &plot.to_path(0.1),
                plot,
                self.m,
                deckcraft_geom::preset::FillMode::Norm,
                1.0,
                self.rctx.pres,
            );
        }
        let to_v = |sc: &Scale, v: f64| if horiz { plot.x0 + sc.frac(v) * plot.width() } else { plot.y1 - sc.frac(v) * plot.height() };
        // Gridlines and value labels.
        for v in primary.ticks().collect::<Vec<_>>() {
            let pos = to_v(&primary, v);
            let mut g = BezPath::new();
            let t = primary.label(v, pct1);
            let col = self.ink;
            if horiz {
                g.move_to((pos, plot.y0));
                g.line_to((pos, plot.y1));
                self.text(&t, Rect::new(pos - 30.0, plot.y1 + 2.0, pos + 30.0, plot.y1 + base * 1.5), base, Align::Center, Anchor::Top, col);
            } else {
                g.move_to((plot.x0, pos));
                g.line_to((plot.x1, pos));
                self.text(&t, Rect::new(area.x0, pos - base, plot.x0 - base * 0.4, pos + base), base, Align::Right, Anchor::Middle, col);
            }
            if c.gridlines {
                let gc = self.grid;
                self.stroke(&g, gc, 0.75);
            }
        }
        if let Some((sec, pct2)) = &secondary {
            for v in sec.ticks().collect::<Vec<_>>() {
                let pos = to_v(sec, v);
                let t = sec.label(v, *pct2);
                let col = self.ink;
                if horiz {
                    self.text(&t, Rect::new(pos - 30.0, plot.y0 - base * 1.5, pos + 30.0, plot.y0 - 2.0), base, Align::Center, Anchor::Bottom, col);
                } else {
                    self.text(&t, Rect::new(plot.x1 + base * 0.4, pos - base, area.x1, pos + base), base, Align::Left, Anchor::Middle, col);
                }
            }
        }
        let band = if horiz { plot.height() / ncat as f64 } else { plot.width() / ncat as f64 };
        // Area charts put their points on the category ticks, from edge to edge; bars run from
        // the bottom up, the first category at the bottom.
        let edges = !horiz && ncat > 1 && list.iter().all(|l| class_of(l.2) == Class::Area);
        let cat_mid = |i: usize| {
            if horiz {
                plot.y1 - band * (i as f64 + 0.5)
            } else if edges {
                plot.x0 + plot.width() * i as f64 / (ncat - 1) as f64
            } else {
                plot.x0 + band * (i as f64 + 0.5)
            }
        };
        // Category labels.
        for (i, name) in c.categories.iter().enumerate().take(ncat) {
            let mid = cat_mid(i);
            let col = self.ink;
            if horiz {
                self.text(name, Rect::new(area.x0, mid - base, plot.x0 - base * 0.4, mid + base), base, Align::Right, Anchor::Middle, col);
            } else {
                self.text(
                    name,
                    Rect::new(mid - band / 2.0, plot.y1 + 3.0, mid + band / 2.0, plot.y1 + base * 1.6),
                    base,
                    Align::Center,
                    Anchor::Top,
                    col,
                );
            }
        }
        let scale_of = |sec: bool| -> Scale { if sec { secondary.map_or(primary, |s| s.0) } else { primary } };
        let pt = |sc: &Scale, i: usize, v: f64| {
            if horiz { Point::new(to_v(sc, v), cat_mid(i)) } else { Point::new(cat_mid(i), to_v(sc, v)) }
        };
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
            let sc = scale_of(*sec);
            let mut top = vec![];
            let mut bottom = vec![];
            for i in 0..ncat {
                let v = s.values.get(i).copied().flatten().filter(|v| v.is_finite()).unwrap_or(0.0);
                let v = if k.percent() { v / total(*sec, Class::Area, i) } else { v };
                let b = if k.stacked() { acc.get(i).map_or(0.0, |a| a[usize::from(*sec)]) } else { 0.0 };
                if k.stacked()
                    && let Some(a) = acc.get_mut(i)
                {
                    a[usize::from(*sec)] = b + v;
                }
                top.push(pt(&sc, i, b + v));
                bottom.push(pt(&sc, i, b));
            }
            let mut shape = line_path(&top, false, false);
            for q in bottom.iter().rev() {
                shape.line_to(*q);
            }
            shape.close_path();
            let col = self.series_col(*si, s);
            self.fill(&shape, col, 0.85);
            if c.data_labels {
                for (i, q) in top.iter().enumerate() {
                    self.value_label(s.values.get(i).copied().flatten().unwrap_or(0.0), *q);
                }
            }
        }
        self.bars(&list, plot, horiz, band, ncat, &scale_of, &total);
        let mut acc: Vec<[f64; 2]> = vec![[0.0; 2]; ncat];
        for (si, s, k, sec) in list.iter().filter(|l| class_of(l.2) == Class::Line) {
            let sc = scale_of(*sec);
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
                pts.push(pt(&sc, i, shown));
                vals.push(v);
            }
            let col = self.series_col(*si, s);
            if !k.points_only() && pts.len() > 1 {
                let (lc, lw) = self.series_line(*si, s);
                let path = line_path(&pts, s.smooth, c.variant.stepped);
                self.stroke(&path, lc, lw);
            }
            if k.markers() {
                self.markers(s, &pts, col);
            }
            if c.data_labels {
                for (q, v) in pts.iter().zip(&vals) {
                    self.value_label(*v, *q);
                }
            }
        }
        if stock {
            self.stock(band, ncat, &|i, v| pt(&scale_of(c.kind.has_volume()), i, v));
        }
        // Axis line.
        let zero = to_v(&primary, 0.0f64.clamp(primary.min, primary.max));
        let mut ax = BezPath::new();
        if horiz {
            ax.move_to((zero, plot.y0));
            ax.line_to((zero, plot.y1));
        } else {
            ax.move_to((plot.x0, zero));
            ax.line_to((plot.x1, zero));
        }
        self.stroke(&ax, Rgba::rgb(0xBF, 0xBF, 0xBF), 0.75);
    }

    /// The bars of a category chart: clustered side by side, or stacked per axis.
    #[allow(clippy::type_complexity)]
    fn bars(
        &mut self,
        list: &[(usize, &Series, ChartType, bool)],
        plot: Rect,
        horiz: bool,
        band: f64,
        ncat: usize,
        scale_of: &dyn Fn(bool) -> Scale,
        total: &dyn Fn(bool, Class, usize) -> f64,
    ) {
        let c = self.c;
        let base = self.base;
        let bars: Vec<&(usize, &Series, ChartType, bool)> = list.iter().filter(|l| class_of(l.2) == Class::Bar && !l.2.is_stock()).collect();
        if bars.is_empty() {
            return;
        }
        // Slots side by side in a category: one per series, or one per stack.
        let mut slots: Vec<(bool, Option<usize>)> = vec![];
        let slot_of = |l: &(usize, &Series, ChartType, bool)| if l.2.stacked() { (l.3, None) } else { (l.3, Some(l.0)) };
        for l in &bars {
            let key = slot_of(l);
            if !slots.contains(&key) {
                slots.push(key);
            }
        }
        let gap = c.gap_width.clamp(0.0, 5.0);
        let group = band / (1.0 + gap);
        let bar_w = group / slots.len().max(1) as f64;
        let three_d = c.variant.three_d.filter(|_| c.kind.is_column_or_bar());
        for i in 0..ncat.min(100_000) {
            let mut stacks: Vec<(bool, f64, f64)> = vec![];
            for l in &bars {
                let (si, s, k, sec) = **l;
                let sc = scale_of(sec);
                let raw = s.values.get(i).copied().flatten().filter(|v| v.is_finite()).unwrap_or(0.0);
                let v = if k.percent() { raw / total(sec, Class::Bar, i) } else { raw };
                let slot = slots.iter().position(|x| *x == slot_of(l)).unwrap_or(0);
                let start = band * i as f64 + (band - group) / 2.0 + bar_w * slot as f64;
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
                    (0.0f64.min(v), 0.0f64.max(v))
                };
                let to = |v: f64| if horiz { plot.x0 + sc.frac(v) * plot.width() } else { plot.y1 - sc.frac(v) * plot.height() };
                let r = if horiz {
                    Rect::new(to(a), plot.y1 - start - bar_w, to(b), plot.y1 - start)
                } else {
                    Rect::new(plot.x0 + start, to(b), plot.x0 + start + bar_w, to(a))
                };
                let col = self.series_col(si, s);
                match three_d {
                    Some(t) => self.bar_3d(r.abs(), col, t.shape, horiz),
                    None => {
                        self.ctx.set_transform(self.m);
                        self.ctx.set_paint(color(col, 1.0));
                        self.ctx.fill_rect(&r.abs());
                    }
                }
                if c.data_labels {
                    let t = fmt_num(raw);
                    let lr = if horiz {
                        Rect::new(r.x1 + 2.0, r.y0, r.x1 + 40.0, r.y1)
                    } else {
                        Rect::new(r.x0 - 10.0, r.y0 - base * 1.4, r.x1 + 10.0, r.y0)
                    };
                    let tc = self.ink;
                    self.text(
                        &t,
                        lr,
                        base * 0.9,
                        if horiz { Align::Left } else { Align::Center },
                        if horiz { Anchor::Middle } else { Anchor::Bottom },
                        tc,
                    );
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
    fn stock(&mut self, band: f64, ncat: usize, at: &dyn Fn(usize, f64) -> Point) {
        let c = self.c;
        let roles = c.stock_series();
        let ink = Rgba::rgb(0x40, 0x40, 0x40);
        let get = |s: Option<&Series>, i: usize| s.and_then(|s| s.values.get(i).copied().flatten()).filter(|v| v.is_finite());
        for i in 0..ncat.min(100_000) {
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
            self.stroke(&hl, ink, 1.25);
            let half = band * 0.2;
            match (o, cl) {
                (Some(o), Some(cl)) => {
                    let (a, b) = (at(i, o), at(i, cl));
                    let body = Rect::new(a.x - half, a.y.min(b.y), a.x + half, a.y.max(b.y).max(a.y.min(b.y) + 1.0)).to_path(0.1);
                    self.fill(&body, if cl >= o { Rgba::WHITE } else { ink }, 1.0);
                    self.stroke(&body, ink, 1.0);
                }
                (None, Some(cl)) => {
                    let q = at(i, cl);
                    let mut tick = BezPath::new();
                    tick.move_to(q);
                    tick.line_to((q.x + half, q.y));
                    self.stroke(&tick, ink, 1.5);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use deckcraft_model::chart::{ChartPreset, ChartVariant};
    use deckcraft_model::{Presentation, ShapeId, ShapeKind, Xfrm};

    use super::*;
    use crate::{Image, RenderOpts, render_slide};

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
            assert!(n > 500, "{}: {n} pixels of ink", p.id);
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
        let first_col = series_color_rgb(0);
        let count = |img: &Image| img.pixels.chunks(4).filter(|px| px.iter().take(3).zip(first_col.iter()).all(|(a, b)| a.abs_diff(*b) < 30)).count();
        // The first band blends with white at 85 %: compare with the same band drawn alone.
        assert!(count(&all) * 10 > count(&first) * 8, "{} vs {}", count(&all), count(&first));
    }

    /// The first accent of the default theme, blended at the area opacity over white.
    fn series_color_rgb(i: usize) -> [u8; 3] {
        let p = Presentation::default();
        let ctx = Ctx::for_master(&p, &p.masters[0]);
        let c = series_color(&ctx, &Chart::default(), i);
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
        ];
        for p in ChartPreset::GALLERY {
            for f in &odd {
                let mut c = preset(p.id);
                f(&mut c);
                let _ = render(&c);
            }
        }
    }
}
