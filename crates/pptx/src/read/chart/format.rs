//! Chart formatting in a chart part → [`ChartFormat`] and [`SeriesFormat`]: fonts (`c:txPr`,
//! rich text run properties), element boxes (`c:spPr`), axes (scaling, units, number formats,
//! tick marks, label position and rotation, crossing, gridlines), data labels (`c:dLbls`),
//! markers, invert if negative and pie explosion.

use deckcraft_model::chart::{Chart, Series};
use deckcraft_model::chart_format::{AxisFormat, ChartText, Crosses, DataLabels, LabelPos, TickLabelPos, TickMark};
use deckcraft_model::style::{Dash, Fill, Line};

use crate::read::dml;
use crate::xml::El;

/// A `CT_Boolean` element: `val` defaults to true when absent.
fn flag(e: Option<&El>) -> Option<bool> {
    e.map(|e| e.bool("val").unwrap_or(true))
}

fn num(e: Option<&El>) -> Option<f64> {
    e.and_then(|e| e.attr("val")).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

/// A colour fill of a chart element (pictures are not kept in charts).
pub(super) fn fill_of(sp: Option<&El>) -> Option<Fill> {
    let sp = sp?;
    for e in sp.elements() {
        match e.local() {
            "noFill" => return Some(Fill::None),
            "solidFill" => return dml::color(e).map(|color| Fill::Solid { color }),
            "gradFill" => return Some(Fill::Gradient(dml::gradient(e))),
            _ => {}
        }
    }
    None
}

/// The outline (`a:ln`) of a chart element: width, colour fill and dash.
pub(super) fn line_of(sp: Option<&El>) -> Option<Line> {
    let ln = sp?.child("ln")?;
    let mut l = Line { width: dml::pt(ln, "w").map(|w| w.clamp(0.0, 1584.0)), ..Default::default() };
    l.fill = fill_of(Some(ln));
    if let Some(d) = ln.child("prstDash").and_then(|d| d.attr("val")) {
        l.dash = Some(Dash::Preset { name: d.to_string() });
    }
    Some(l)
}

/// Run properties (`a:defRPr`, `a:rPr`) → chart text, over `base`.
fn run_text(r: &El, base: ChartText) -> ChartText {
    let mut t = base;
    if let Some(s) = r.f64("sz").filter(|s| s.is_finite()) {
        t.size = Some((s / 100.0).clamp(1.0, 400.0));
    }
    if let Some(b) = r.bool("b") {
        t.bold = Some(b);
    }
    if let Some(i) = r.bool("i") {
        t.italic = Some(i);
    }
    for e in r.elements() {
        match e.local() {
            "solidFill" => t.color = dml::color(e),
            "latin" => t.font = e.attr("typeface").filter(|f| !f.is_empty() && !f.starts_with('+')).map(String::from),
            _ => {}
        }
    }
    t
}

/// The font of a text element: `c:txPr` (or `c:tx/c:rich`), its first paragraph's default run
/// properties, then its first run's.
pub(super) fn text_of(e: Option<&El>) -> ChartText {
    let Some(e) = e else { return ChartText::default() };
    let body = e.child("txPr").or_else(|| e.path(&["tx", "rich"]));
    let Some(p) = body.and_then(|b| b.child("p")) else { return ChartText::default() };
    let mut t = ChartText::default();
    if let Some(d) = p.path(&["pPr", "defRPr"]) {
        t = run_text(d, t);
    }
    if let Some(r) = p.path(&["r", "rPr"]) {
        t = run_text(r, t);
    }
    t
}

/// Rotation of the tick labels (`c:txPr/a:bodyPr@rot`, 1/60 000 degree).
fn rotation_of(e: &El) -> Option<f64> {
    let rot = e.path(&["txPr", "bodyPr"])?.f64("rot")?;
    rot.is_finite().then(|| (rot / 60_000.0).clamp(-90.0, 90.0))
}

/// `c:dLbls` → whether labels show, and which parts, where and how.
pub(super) fn labels_of(d: &El) -> (bool, DataLabels) {
    let mut l = DataLabels {
        value: flag(d.child("showVal")).unwrap_or(false),
        category: flag(d.child("showCatName")).unwrap_or(false),
        series: flag(d.child("showSerName")).unwrap_or(false),
        percent: flag(d.child("showPercent")).unwrap_or(false),
        legend_key: flag(d.child("showLegendKey")).unwrap_or(false),
        position: d.child_val("dLblPos").and_then(LabelPos::from_xml),
        num_fmt: d.child("numFmt").and_then(num_fmt),
        separator: d.child("separator").map(|s| s.text()),
        text: text_of(Some(d)),
        fill: fill_of(d.child("spPr")),
        line: line_of(d.child("spPr")),
    };
    let shown = l.value || l.category || l.series || l.percent;
    if flag(d.child("delete")) == Some(true) {
        return (false, DataLabels::default());
    }
    if !shown {
        l.value = true;
    }
    (shown, l)
}

/// A custom number format: kept unless it follows the source data or is General.
fn num_fmt(e: &El) -> Option<String> {
    let code = e.attr("formatCode")?.trim();
    if e.bool("sourceLinked") == Some(true) || code.is_empty() || code.eq_ignore_ascii_case("general") {
        return None;
    }
    Some(code.to_string())
}

/// An axis element (`c:catAx`, `c:valAx`, `c:dateAx`) → its format. The value axis' bounds and
/// major gridlines switch are read by the chart reader itself.
pub(super) fn axis_of(a: &El) -> AxisFormat {
    let sc = a.child("scaling");
    let grid = |name: &str| a.child(name);
    AxisFormat {
        hidden: flag(a.child("delete")).unwrap_or(false),
        min: num(sc.and_then(|s| s.child("min"))),
        max: num(sc.and_then(|s| s.child("max"))),
        reverse: sc.and_then(|s| s.child_val("orientation")) == Some("maxMin"),
        log_base: num(sc.and_then(|s| s.child("logBase"))).map(|b| b.clamp(2.0, 1000.0)),
        major_unit: num(a.child("majorUnit")).filter(|v| *v > 0.0),
        minor_unit: num(a.child("minorUnit")).filter(|v| *v > 0.0),
        num_fmt: a.child("numFmt").and_then(num_fmt),
        // What a new chart has (no tick marks, labels next to the axis, crossing at zero) reads
        // as unset, so charts made here come back as they were.
        major_tick: a.child_val("majorTickMark").and_then(TickMark::from_xml).filter(|t| *t != TickMark::None),
        minor_tick: a.child_val("minorTickMark").and_then(TickMark::from_xml).filter(|t| *t != TickMark::None),
        label_pos: a.child_val("tickLblPos").and_then(TickLabelPos::from_xml).filter(|p| *p != TickLabelPos::NextTo),
        crosses: match (a.child_val("crosses"), num(a.child("crossesAt"))) {
            (_, Some(v)) => Some(Crosses::At { value: v }),
            (Some("min"), _) => Some(Crosses::Min),
            (Some("max"), _) => Some(Crosses::Max),
            _ => None,
        },
        text: text_of(Some(a)),
        rotation: rotation_of(a),
        line: line_of(a.child("spPr")),
        major_gridlines: grid("majorGridlines").is_some(),
        minor_gridlines: grid("minorGridlines").is_some(),
        major_grid_line: grid("majorGridlines").and_then(|g| line_of(g.child("spPr"))),
        minor_grid_line: grid("minorGridlines").and_then(|g| line_of(g.child("spPr"))),
        title_text: text_of(a.child("title")),
    }
}

/// Chart-wide formatting: fonts, boxes, axes, data labels, first slice angle.
pub(super) fn read_format(root: &El, ch: &El, pa: &El, c: &mut Chart, val_ax: Option<&El>, cat_ax: Option<&El>) {
    let f = &mut c.format;
    f.text = text_of(Some(root));
    f.rounded_corners = flag(root.child("roundedCorners")).unwrap_or(false);
    f.chart_line = line_of(root.child("spPr"));
    f.plot_line = line_of(pa.child("spPr"));
    if let Some(t) = ch.child("title") {
        f.title_text = text_of(Some(t));
        f.title_overlay = flag(t.child("overlay")).unwrap_or(false);
        f.title_fill = fill_of(t.child("spPr"));
        f.title_line = line_of(t.child("spPr"));
    }
    if let Some(l) = ch.child("legend") {
        f.legend_text = text_of(Some(l));
        f.legend_overlay = flag(l.child("overlay")).unwrap_or(false);
        f.legend_fill = fill_of(l.child("spPr"));
        f.legend_line = line_of(l.child("spPr"));
    }
    if let Some(a) = val_ax {
        f.val_axis = axis_of(a);
        // The value axis' major gridlines and bounds are `Chart::gridlines`, `y_min`, `y_max`.
        f.val_axis.major_gridlines = false;
        (f.val_axis.min, f.val_axis.max) = (None, None);
    }
    if let Some(a) = cat_ax {
        f.cat_axis = axis_of(a);
    }
    for g in pa.elements().filter(|g| g.local().ends_with("Chart")) {
        if let Some(d) = g.child("dLbls") {
            let (shown, l) = labels_of(d);
            if shown {
                c.data_labels = true;
            }
            f.labels = l;
        }
        if let Some(a) = num(g.child("firstSliceAng")) {
            f.first_slice_angle = a.rem_euclid(360.0);
        }
    }
    if !c.data_labels
        && let Some(d) = pa.elements().filter(|g| g.local().ends_with("Chart")).flat_map(|g| g.children_named("ser")).find_map(|s| s.child("dLbls"))
    {
        // Only series labels: their options stand for the chart's.
        let (_, l) = labels_of(d);
        c.format.labels = l;
    }
}

/// Series formatting: marker size and colours, invert if negative, explosion.
pub(super) fn read_series_format(s: &El, ser: &mut Series) {
    let f = &mut ser.format;
    if let Some(m) = s.child("marker") {
        f.marker_size = num(m.child("size")).map(|v| v.clamp(2.0, 72.0) as u32);
        f.marker_fill = fill_of(m.child("spPr"));
        f.marker_line = line_of(m.child("spPr"));
    }
    f.invert_if_negative = flag(s.child("invertIfNegative")).unwrap_or(false);
    // The series' explosion also makes the chart an exploded pie or doughnut (the chart reader
    // sets `ChartVariant::explosion`).
    f.explosion = num(s.child("explosion")).map(|v| v.clamp(0.0, 400.0) as u32).filter(|v| *v > 0);
    for dp in s.children_named("dPt").take(1000) {
        if let (Some(i), Some(x)) = (dp.child("idx").and_then(|i| i.u32("val")), num(dp.child("explosion"))) {
            let x = x.clamp(0.0, 400.0) as u32;
            if x > 0 {
                f.point_explosion.push((i as usize, x));
            }
        }
    }
    if let Some(d) = s.child("dLbls") {
        let (shown, _) = labels_of(d);
        ser.data_labels = Some(shown);
    }
}
