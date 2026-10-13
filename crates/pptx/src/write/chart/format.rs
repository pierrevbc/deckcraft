//! Chart formatting → chart part XML: element boxes (`c:spPr` with gradients and dashes), fonts
//! (`c:txPr`, rich title runs), titles, the legend, axes, data labels, markers and data points.
//! Elements follow the order of the ECMA-376 chart schema, which PowerPoint enforces.

use deckcraft_model::chart::{Chart, Series};
use deckcraft_model::chart_format::{AxisFormat, ChartText, Crosses, DataLabels, LabelPos, MARKERS};
use deckcraft_model::style::{Dash, Fill, GradientShape, Line};

use super::{Group, fmt_num};
use crate::write::dml;
use crate::xml::{A, W};

/// A colour fill (pictures and patterns are not written into charts: a solid of their first
/// colour stands in).
pub(super) fn fill(w: &mut W, f: &Fill) {
    match f {
        Fill::None => w.empty0("a:noFill"),
        Fill::Solid { color } => dml::solid(w, color),
        Fill::Gradient(g) if g.stops.len() >= 2 => {
            w.open("a:gradFill", A::new().a("rotWithShape", if g.rotate_with_shape { "1" } else { "0" }));
            w.open0("a:gsLst");
            for s in g.stops.iter().take(64) {
                w.open("a:gs", A::new().a("pos", dml::pct(s.pos.clamp(0.0, 1.0))));
                dml::color(w, &s.color);
                w.close("a:gs");
            }
            w.close("a:gsLst");
            match &g.shape {
                GradientShape::Linear { angle, scaled } => {
                    w.empty("a:lin", A::new().a("ang", dml::ang(*angle)).a("scaled", if *scaled { "1" } else { "0" }))
                }
                GradientShape::Path { path, focus } => {
                    let p = if matches!(path.as_str(), "rect" | "shape" | "circle") { path.as_str() } else { "circle" };
                    w.open("a:path", A::new().a("path", p));
                    let pc = |v: f64| dml::pct(v.clamp(0.0, 1.0));
                    w.empty("a:fillToRect", A::new().a("l", pc(focus[0])).a("t", pc(focus[1])).a("r", pc(focus[2])).a("b", pc(focus[3])));
                    w.close("a:path");
                }
            }
            w.close("a:gradFill");
        }
        Fill::Gradient(g) => {
            if let Some(s) = g.stops.first() {
                dml::solid(w, &s.color);
            }
        }
        Fill::Pattern(p) => dml::solid(w, &p.fg),
        Fill::Picture(_) | Fill::Group | Fill::Background => {}
    }
}

/// An outline: `a:ln` with width, colour and dash.
fn ln(w: &mut W, l: &Line) {
    w.open("a:ln", A::new().o("w", l.width.filter(|v| v.is_finite()).map(|v| dml::emu_pos(v.clamp(0.0, 1584.0)))));
    if let Some(f) = &l.fill {
        fill(w, f);
    }
    if let Some(Dash::Preset { name }) = &l.dash {
        let n = if Dash::PRESETS.contains(&name.as_str()) { name.as_str() } else { "solid" };
        w.val("a:prstDash", n);
    }
    w.close("a:ln");
}

/// `c:spPr` with a fill and an outline, when either is set.
pub(super) fn sp_pr(w: &mut W, f: Option<&Fill>, l: Option<&Line>) {
    if f.is_none() && l.is_none() {
        return;
    }
    w.open0("c:spPr");
    if let Some(f) = f {
        fill(w, f);
    }
    if let Some(l) = l {
        ln(w, l);
    }
    w.close("c:spPr");
}

/// Run properties of chart text (`a:defRPr`, `a:rPr`).
fn run_props(w: &mut W, tag: &str, t: &ChartText, lang: bool) {
    let a = A::new()
        .o("lang", lang.then_some("en-US"))
        .o("sz", t.size.filter(|s| s.is_finite()).map(|s| (s.clamp(1.0, 400.0) * 100.0).round() as i64))
        .b("b", t.bold)
        .b("i", t.italic);
    if t.color.is_none() && t.font.is_none() {
        w.empty(tag, a);
        return;
    }
    w.open(tag, a);
    if let Some(c) = &t.color {
        dml::solid(w, c);
    }
    if let Some(f) = t.font.as_deref().filter(|f| !f.is_empty()) {
        w.empty("a:latin", A::new().a("typeface", f));
    }
    w.close(tag);
}

/// `c:txPr`: a font, and a rotation (degrees) for tick labels. Nothing when both are unset.
pub(super) fn tx_pr(w: &mut W, t: &ChartText, rot: Option<f64>) {
    let rot = rot.filter(|r| r.is_finite() && *r != 0.0);
    if t.is_empty() && rot.is_none() {
        return;
    }
    w.open0("c:txPr");
    match rot {
        Some(r) => w.empty("a:bodyPr", A::new().a("rot", (r.clamp(-90.0, 90.0) * 60_000.0).round() as i64).a("vert", "horz")),
        None => w.empty0("a:bodyPr"),
    }
    w.empty0("a:lstStyle");
    w.open0("a:p");
    w.open0("a:pPr");
    run_props(w, "a:defRPr", t, false);
    w.close("a:pPr");
    w.empty("a:endParaRPr", A::new().a("lang", "en-US"));
    w.close("a:p");
    w.close("c:txPr");
}

/// A chart or axis title: its text in runs carrying the font, overlay and box.
pub(super) fn title(w: &mut W, text: &str, t: &ChartText, overlay: bool, f: Option<&Fill>, l: Option<&Line>) {
    w.open0("c:title");
    w.open0("c:tx");
    w.open0("c:rich");
    w.empty0("a:bodyPr");
    w.empty0("a:lstStyle");
    for line in text.split('\n') {
        w.open0("a:p");
        if !t.is_empty() {
            w.open0("a:pPr");
            run_props(w, "a:defRPr", t, false);
            w.close("a:pPr");
        }
        w.open0("a:r");
        if !t.is_empty() {
            run_props(w, "a:rPr", t, true);
        }
        w.elt("a:t", line);
        w.close("a:r");
        w.close("a:p");
    }
    w.close("c:rich");
    w.close("c:tx");
    w.val("c:overlay", if overlay { 1 } else { 0 });
    sp_pr(w, f, l);
    w.close("c:title");
}

/// The legend, when the chart shows one.
pub(super) fn legend(w: &mut W, c: &Chart) {
    let Some(pos) = &c.legend else { return };
    let p = match pos.as_str() {
        "b" | "t" | "l" | "r" | "tr" => pos.as_str(),
        _ => "r",
    };
    let f = &c.format;
    w.open0("c:legend");
    w.val("c:legendPos", p);
    w.val("c:overlay", u8::from(c.legend_overlay));
    sp_pr(w, f.legend_fill.as_ref(), f.legend_line.as_ref());
    tx_pr(w, &f.legend_text, None);
    w.close("c:legend");
}

/// Label positions a plot group accepts (PowerPoint refuses the others).
fn positions(g: Group, stacked: bool) -> &'static [LabelPos] {
    use LabelPos::*;
    match g {
        Group::Bar if stacked => &[Center, InsideEnd, InsideBase],
        Group::Bar => &[Center, InsideEnd, InsideBase, OutsideEnd],
        Group::Line | Group::Scatter | Group::Bubble | Group::Stock => &[Center, Left, Right, Above, Below],
        Group::Pie | Group::OfPie => &[Center, InsideEnd, OutsideEnd, BestFit],
        Group::Area | Group::Doughnut | Group::Radar => &[],
    }
}

/// `c:dLbls`: what labels show (nothing when `show` is off), where, their number format
/// (`num_fmt`, the series'), box and font.
pub(super) fn dlbls(w: &mut W, l: &DataLabels, num_fmt: Option<&str>, show: bool, g: Group, stacked: bool) {
    w.open0("c:dLbls");
    if let Some(code) = num_fmt.filter(|c| !c.trim().is_empty() && !c.trim().eq_ignore_ascii_case("general")) {
        w.empty("c:numFmt", A::new().a("formatCode", code).a("sourceLinked", 0));
    }
    sp_pr(w, l.fill.as_ref(), l.line.as_ref());
    tx_pr(w, &l.text, None);
    if show && let Some(p) = l.position.filter(|p| positions(g, stacked).contains(p)) {
        w.val("c:dLblPos", p.xml());
    }
    let pie = matches!(g, Group::Pie | Group::Doughnut | Group::OfPie);
    for (n, v) in [
        ("c:showLegendKey", l.legend_key),
        ("c:showVal", l.value),
        ("c:showCatName", l.category),
        ("c:showSerName", l.series),
        ("c:showPercent", l.percent && pie),
        ("c:showBubbleSize", false),
    ] {
        w.val(n, if show && v { 1 } else { 0 });
    }
    if let Some(s) = &l.separator {
        w.elt("c:separator", s);
    }
    w.close("c:dLbls");
}

/// `c:marker`: symbol (`None` leaves it to the chart type), size and colours (`auto` for those
/// the series doesn't set: a palette colour).
pub(super) fn marker(w: &mut W, symbol: Option<&str>, s: &Series, auto: Option<&Fill>) {
    let f = &s.format;
    let symbol = symbol.filter(|m| MARKERS.contains(m));
    let size = f.marker_size.map(|v| v.clamp(2, 72));
    let fill = f.marker_fill.as_ref().or(auto);
    let auto_line = auto.map(|a| Line { fill: Some(a.clone()), ..Default::default() });
    let line = f.marker_line.as_ref().or(auto_line.as_ref());
    if symbol.is_none() && size.is_none() && fill.is_none() && line.is_none() {
        return;
    }
    w.open0("c:marker");
    if let Some(sym) = symbol {
        w.val("c:symbol", sym);
    }
    if symbol != Some("none") {
        if let Some(v) = size {
            w.val("c:size", v);
        }
        sp_pr(w, fill, line);
    }
    w.close("c:marker");
}

/// `c:dPt` for every point with its own fill or explosion. `vary` gives the fill of points that
/// have none of their own (a palette other than the default, pie charts).
pub(super) fn points(w: &mut W, s: &Series, g: Group, vary: Option<&dyn Fn(usize) -> Fill>) {
    if !matches!(g, Group::Pie | Group::Doughnut | Group::OfPie | Group::Bar) {
        return;
    }
    let pie = matches!(g, Group::Pie | Group::Doughnut | Group::OfPie);
    let mut idx: Vec<usize> = s.point_fills.iter().map(|(i, _)| *i).collect();
    if pie {
        idx.extend(s.format.point_explosion.iter().map(|(i, _)| *i));
        if vary.is_some() {
            idx.extend(0..s.values.len());
        }
    }
    idx.sort_unstable();
    idx.dedup();
    for i in idx.into_iter().take(1000) {
        let own = s.point_fills.iter().find(|(k, _)| *k == i).map(|(_, f)| f.clone());
        let fill = own.or_else(|| if pie { vary.map(|v| v(i)) } else { None });
        let explosion = if pie { s.format.point_explosion.iter().find(|(k, _)| *k == i).map(|(_, x)| *x) } else { None };
        if fill.is_none() && explosion.is_none() {
            continue;
        }
        w.open0("c:dPt");
        w.val("c:idx", i);
        if g == Group::Bar {
            w.val("c:invertIfNegative", if s.format.invert_if_negative { 1 } else { 0 });
        } else {
            w.val("c:bubble3D", 0);
        }
        if let Some(x) = explosion {
            w.val("c:explosion", x.min(400));
        }
        sp_pr(w, fill.as_ref(), None);
        w.close("c:dPt");
    }
}

/// One axis to write.
pub(super) struct AxisSpec<'a> {
    /// `c:catAx` or `c:valAx`.
    pub tag: &'static str,
    pub id: u32,
    pub cross: u32,
    pub pos: &'static str,
    pub fmt: &'a AxisFormat,
    pub major_gridlines: bool,
    pub title: Option<&'a str>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Number format when the axis has none of its own.
    pub num_fmt: &'static str,
    /// Major tick marks when the axis sets none.
    pub major_tick: &'static str,
    /// Value axes: `between` or `midCat`.
    pub cross_between: Option<&'static str>,
}

/// A whole `c:catAx` / `c:valAx` in schema order.
pub(super) fn axis(w: &mut W, a: &AxisSpec<'_>) {
    let f = a.fmt;
    let cat = a.tag == "c:catAx";
    w.open0(a.tag);
    w.val("c:axId", a.id);
    w.open0("c:scaling");
    if !cat && let Some(b) = f.log_base.filter(|b| b.is_finite()) {
        w.val("c:logBase", fmt_num(b.clamp(2.0, 1000.0)));
    }
    w.val("c:orientation", if f.reverse { "maxMin" } else { "minMax" });
    if let Some(m) = a.max.filter(|v| v.is_finite()) {
        w.val("c:max", fmt_num(m));
    }
    if let Some(m) = a.min.filter(|v| v.is_finite()) {
        w.val("c:min", fmt_num(m));
    }
    w.close("c:scaling");
    w.val("c:delete", if f.hidden { 1 } else { 0 });
    w.val("c:axPos", a.pos);
    for (on, line, tag) in [(a.major_gridlines, &f.major_grid_line, "c:majorGridlines"), (f.minor_gridlines, &f.minor_grid_line, "c:minorGridlines")]
    {
        if on {
            match line {
                Some(l) => {
                    w.open0(tag);
                    sp_pr(w, None, Some(l));
                    w.close(tag);
                }
                None => w.empty0(tag),
            }
        }
    }
    if let Some(t) = a.title {
        title(w, t, &f.title_text, false, None, None);
    }
    match f.num_fmt.as_deref().filter(|c| !c.trim().is_empty()) {
        Some(code) => w.empty("c:numFmt", A::new().a("formatCode", code).a("sourceLinked", 0)),
        None => w.empty("c:numFmt", A::new().a("formatCode", a.num_fmt).a("sourceLinked", 1)),
    }
    w.val("c:majorTickMark", f.major_tick.map(|t| t.xml()).unwrap_or(a.major_tick));
    w.val("c:minorTickMark", f.minor_tick.map(|t| t.xml()).unwrap_or("none"));
    w.val("c:tickLblPos", f.label_pos.map(|p| p.xml()).unwrap_or("nextTo"));
    sp_pr(w, None, f.line.as_ref());
    let text = if f.text.is_empty() && f.rotation.is_none() { None } else { Some(&f.text) };
    if let Some(t) = text {
        tx_pr(w, t, f.rotation);
    }
    w.val("c:crossAx", a.cross);
    match f.crosses {
        Some(Crosses::At { value }) if value.is_finite() => w.val("c:crossesAt", fmt_num(value)),
        Some(Crosses::Min) => w.val("c:crosses", "min"),
        Some(Crosses::Max) => w.val("c:crosses", "max"),
        _ => w.val("c:crosses", "autoZero"),
    }
    if cat {
        w.val("c:auto", 1);
        w.val("c:lblAlgn", "ctr");
        w.val("c:lblOffset", 100);
        w.val("c:noMultiLvlLbl", 0);
    } else {
        if let Some(b) = a.cross_between {
            w.val("c:crossBetween", b);
        }
        for (v, tag) in [(f.major_unit, "c:majorUnit"), (f.minor_unit, "c:minorUnit")] {
            if let Some(u) = v.filter(|u| u.is_finite() && *u > 0.0) {
                w.val(tag, fmt_num(u));
            }
        }
    }
    w.close(a.tag);
}
