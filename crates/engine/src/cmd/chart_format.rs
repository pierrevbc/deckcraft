//! Chart formatting: the Format pane's chart elements (chart and plot area, title, legend, axes,
//! gridlines, data labels, series and points), series options, and Chart Design's Quick Layout,
//! Change Colors, Chart Styles and Reset to Match Style.
//!
//! Fill params: `"auto"`/`null` (as the chart style draws it), `"none"`, a colour (`"#4472c4"`,
//! `"accent2"`, `{"scheme":"accent1","lumMod":75000}`), or `{"gradient": {stops, angle?, kind?}}`
//! (as `shape.fill`). Line params: `"auto"`/`null`, `"none"`, or `{color?, width? (pt), dash?}`
//! merged into the current line. Font params: `{size?, bold?, italic?, color?, font?}`; `null`
//! clears a field.

use deckcraft_color::{ColorTransform, SchemeSlot};
use deckcraft_model::chart::{Chart, ChartType};
use deckcraft_model::chart_format::{
    AxisFormat, ChartFormat, ChartText, Crosses, DataLabels, LabelPos, MARKERS, PALETTES, SeriesFormat, TickLabelPos, TickMark, palette_color,
};
use deckcraft_model::style::{ColorRef, Dash, Fill, Gradient, GradientShape, GradientStop, Line};
use deckcraft_model::{Shape, ShapeKind};
use serde_json::Value;

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "chart.format.element",
            "Format Selection",
            ["Chart Format", "Current Selection"],
            None,
            "{element: chartArea|plotArea|title|legend|dataLabels|categoryAxis|valueAxis|categoryAxisTitle|valueAxisTitle|majorGridlines|minorGridlines|categoryMajorGridlines|categoryMinorGridlines, fill?, line?, font?, overlay?: bool (title, legend), roundedCorners?: bool (chartArea), reset?: bool, id?}",
            has_selection,
            element
        ),
        cmd!(
            "chart.format.legend",
            "Legend",
            ["Chart Design", "Chart Layouts", "Add Chart Element"],
            None,
            "{position?: top|bottom|left|right|topRight|none, overlay?: bool, id?}",
            has_selection,
            legend
        ),
        cmd!(
            "chart.format.axis",
            "Format Axis",
            ["Chart Format", "Current Selection"],
            None,
            "{axis: category|value, visible?, min?: number|\"auto\", max?: number|\"auto\" (value axis), majorUnit?, minorUnit?: number|\"auto\", logBase?: number|null, reverse?, numFmt?: string|null, majorTick?/minorTick?: none|in|out|cross, labelPosition?: nextTo|low|high|none, crosses?: auto|min|max|number, rotation?: deg|null, majorGridlines?, minorGridlines?, title?: string|null, id?}",
            has_selection,
            axis
        ),
        cmd!(
            "chart.format.labels",
            "Data Labels",
            ["Chart Design", "Chart Layouts", "Add Chart Element"],
            None,
            "{show?: bool, series?: index (that series only), value?, category?, seriesName?, percent?, legendKey?: bool, position?: center|insideEnd|insideBase|outsideEnd|left|right|above|below|bestFit|null, numFmt?: string|null, separator?: string|null, id?}",
            has_selection,
            labels
        ),
        cmd!(
            "chart.format.series",
            "Format Data Series",
            ["Chart Format", "Current Selection"],
            None,
            "{series: index, fill?, line?, marker?: none|circle|square|diamond|triangle|x|star|dash|dot|plus|auto, markerSize?: 2-72, markerFill?, markerLine?, smooth?, invertIfNegative?, explosion?: percent, secondaryAxis?, reset?, id?}",
            has_selection,
            series
        ),
        cmd!(
            "chart.format.point",
            "Format Data Point",
            ["Chart Format", "Current Selection"],
            None,
            "{series?: index (0), point: index, fill?, explosion?: percent, reset?, id?}",
            has_selection,
            point
        ),
        cmd!(
            "chart.format.options",
            "Series Options",
            ["Chart Format", "Current Selection"],
            None,
            "{gapWidth?: 0-500 %, overlap?: -100-100 %, holeSize?: 10-90 %, firstSliceAngle?: 0-360 deg, varyColors?: bool, id?}",
            has_selection,
            options
        ),
        cmd!(
            "chart.colors",
            "Change Colors",
            ["Chart Design", "Chart Styles"],
            None,
            "{palette: colorful1-4|monochrome1-7, id?}",
            has_selection,
            colors
        ),
        cmd!("chart.style", "Chart Styles", ["Chart Design", "Chart Styles"], None, "{style: 1-8, id?}", has_selection, style),
        cmd!("chart.quickLayout", "Quick Layout", ["Chart Design", "Chart Layouts"], None, "{layout: 1-8, id?}", has_selection, quick_layout),
        cmd!("chart.resetStyle", "Reset to Match Style", ["Chart Format", "Current Selection"], None, "{id?}", has_selection, reset_style),
    ]
}

/// Number of Chart Styles ([`apply_style`]) and Quick Layouts ([`apply_layout`]).
pub const STYLES: usize = 8;
pub const LAYOUTS: usize = 8;

fn with_chart(s: &mut Session, p: &Value, cmd: &str, f: impl Fn(&mut Chart) -> Result<()>) -> Result<Value> {
    edit_shapes(s, p, cmd, |sh: &mut Shape| match &mut sh.kind {
        ShapeKind::Chart(c) => {
            f(c)?;
            c.raw = None;
            Ok(())
        }
        _ => Err(bad(cmd, "not a chart")),
    })
}

/// A number param in `lo..=hi`; `Err` when it's there but not a number.
fn num(p: &Value, key: &str, cmd: &str, lo: f64, hi: f64) -> Result<Option<f64>> {
    match p.get(key) {
        None => Ok(None),
        Some(v) => v.as_f64().filter(|v| v.is_finite()).map(|v| Some(v.clamp(lo, hi))).ok_or_else(|| bad(cmd, format!("`{key}` must be a number"))),
    }
}

/// `number | "auto" | null` → `Some(Some(n))`, `Some(None)` (automatic), or `None` (absent).
fn auto_num(p: &Value, key: &str, cmd: &str) -> Result<Option<Option<f64>>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) if s == "auto" => Ok(Some(None)),
        Some(v) => {
            v.as_f64().filter(|v| v.is_finite()).map(|v| Some(Some(v))).ok_or_else(|| bad(cmd, format!("`{key}` must be a number or \"auto\"")))
        }
    }
}

/// A fill param (see the module docs). `Ok(None)`: automatic.
pub(crate) fn fill_value(v: &Value, cmd: &str) -> Result<Option<Fill>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if s == "auto" || s == "automatic" => Ok(None),
        Value::String(s) if s == "none" => Ok(Some(Fill::None)),
        Value::Object(o) if o.get("none").and_then(Value::as_bool) == Some(true) => Ok(Some(Fill::None)),
        Value::Object(o) if o.contains_key("gradient") => o
            .get("gradient")
            .and_then(super::shape::gradient_param)
            .map(|g| Some(Fill::Gradient(g)))
            .ok_or_else(|| bad(cmd, "gradient needs at least two `stops`: [[0, \"#fff\"], [1, \"#000\"]]")),
        Value::Object(o) if o.contains_key("color") => {
            o.get("color").and_then(color_value).map(|c| Some(Fill::solid(c))).ok_or_else(|| bad(cmd, "unknown colour"))
        }
        v => color_value(v).map(|c| Some(Fill::solid(c))).ok_or_else(|| bad(cmd, "fill: \"auto\", \"none\", a colour or {gradient}")),
    }
}

/// A line param merged into `cur` (see the module docs). `Ok(None)`: automatic.
pub(crate) fn line_value(v: &Value, cur: Option<&Line>, cmd: &str) -> Result<Option<Line>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if s == "auto" || s == "automatic" => Ok(None),
        Value::String(s) if s == "none" => Ok(Some(Line::none())),
        Value::Object(o) => {
            let mut l = cur.cloned().unwrap_or_default();
            if o.get("none").and_then(Value::as_bool) == Some(true) {
                return Ok(Some(Line::none()));
            }
            if let Some(c) = o.get("color") {
                l.fill = Some(Fill::solid(color_value(c).ok_or_else(|| bad(cmd, "unknown line colour"))?));
            }
            if let Some(w) = o.get("width") {
                let w = w.as_f64().filter(|w| w.is_finite()).ok_or_else(|| bad(cmd, "line `width` must be a number (pt)"))?;
                l.width = Some(w.clamp(0.0, 1584.0));
                // A width on a hidden line shows it again, in its automatic colour.
                if matches!(l.fill, Some(Fill::None)) {
                    l.fill = None;
                }
            }
            if let Some(d) = o.get("dash") {
                l.dash = match d.as_str() {
                    None | Some("solid") => None,
                    Some(n) if Dash::PRESETS.contains(&n) => Some(Dash::Preset { name: n.to_string() }),
                    Some(n) => return Err(bad(cmd, format!("unknown dash `{n}`: {}", Dash::PRESETS.join(", ")))),
                };
            }
            Ok(Some(l))
        }
        v => color_value(v).map(|c| Some(Line { fill: Some(Fill::solid(c)), ..Default::default() })).ok_or_else(|| bad(cmd, "unknown line")),
    }
}

/// A font param applied to `t`.
pub(crate) fn apply_font(t: &mut ChartText, v: &Value, cmd: &str) -> Result<()> {
    let Value::Object(o) = v else { return Err(bad(cmd, "`font` must be {size?, bold?, italic?, color?, font?}")) };
    if let Some(v) = o.get("size") {
        t.size = match v {
            Value::Null => None,
            v => Some(v.as_f64().filter(|v| v.is_finite()).ok_or_else(|| bad(cmd, "font `size` must be a number"))?.clamp(1.0, 400.0)),
        };
    }
    if let Some(v) = o.get("bold") {
        t.bold = v.as_bool();
    }
    if let Some(v) = o.get("italic") {
        t.italic = v.as_bool();
    }
    if let Some(v) = o.get("color") {
        t.color = match v {
            Value::Null => None,
            v => Some(color_value(v).ok_or_else(|| bad(cmd, "unknown font colour"))?),
        };
    }
    if let Some(v) = o.get("font").or_else(|| o.get("name")) {
        t.font = v.as_str().filter(|f| !f.is_empty()).map(String::from);
    }
    Ok(())
}

fn axis_of<'a>(f: &'a mut ChartFormat, name: &str, cmd: &str) -> Result<&'a mut AxisFormat> {
    match name {
        "category" | "cat" | "horizontal" | "x" => Ok(&mut f.cat_axis),
        "value" | "val" | "vertical" | "y" => Ok(&mut f.val_axis),
        _ => Err(bad(cmd, "`axis`: category or value")),
    }
}

fn element(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.format.element";
    let el = str_param(p, "element").ok_or_else(|| bad(CMD, "missing `element`"))?.to_string();
    let fill = p.get("fill").map(|v| fill_value(v, CMD)).transpose()?;
    let reset = bool_or(p, "reset", false);
    with_chart(s, p, CMD, |c| {
        let line = |cur: Option<&Line>| p.get("line").map(|v| line_value(v, cur, CMD)).transpose();
        let font = |t: &mut ChartText| -> Result<()> {
            if reset {
                *t = ChartText::default();
            }
            match p.get("font") {
                Some(v) => apply_font(t, v, CMD),
                None => Ok(()),
            }
        };
        let f = &mut c.format;
        // (fill, line) slots of the element, and its font.
        let (fill_slot, line_slot): (Option<&mut Option<Fill>>, Option<&mut Option<Line>>) = match el.as_str() {
            "chartArea" | "chart" => {
                font(&mut f.text)?;
                if reset {
                    c.text_color = None;
                }
                if let Some(v) = bool_param(p, "roundedCorners") {
                    f.rounded_corners = v;
                }
                (Some(&mut c.chart_fill), Some(&mut f.chart_line))
            }
            "plotArea" | "plot" => (Some(&mut c.plot_fill), Some(&mut f.plot_line)),
            "title" | "chartTitle" => {
                font(&mut f.title_text)?;
                if let Some(v) = bool_param(p, "overlay") {
                    f.title_overlay = v;
                }
                (Some(&mut f.title_fill), Some(&mut f.title_line))
            }
            "legend" => {
                font(&mut f.legend_text)?;
                if let Some(v) = bool_param(p, "overlay") {
                    f.legend_overlay = v;
                }
                (Some(&mut f.legend_fill), Some(&mut f.legend_line))
            }
            "dataLabels" | "labels" => {
                font(&mut f.labels.text)?;
                (Some(&mut f.labels.fill), Some(&mut f.labels.line))
            }
            "categoryAxis" | "valueAxis" => {
                let a = if el == "categoryAxis" { &mut f.cat_axis } else { &mut f.val_axis };
                font(&mut a.text)?;
                (None, Some(&mut a.line))
            }
            "categoryAxisTitle" | "valueAxisTitle" => {
                let a = if el == "categoryAxisTitle" { &mut f.cat_axis } else { &mut f.val_axis };
                font(&mut a.title_text)?;
                (None, None)
            }
            "majorGridlines" => (None, Some(&mut f.val_axis.major_grid_line)),
            "minorGridlines" => (None, Some(&mut f.val_axis.minor_grid_line)),
            "categoryMajorGridlines" => (None, Some(&mut f.cat_axis.major_grid_line)),
            "categoryMinorGridlines" => (None, Some(&mut f.cat_axis.minor_grid_line)),
            other => return Err(bad(CMD, format!("unknown element `{other}`"))),
        };
        match fill_slot {
            Some(slot) => {
                if reset {
                    *slot = None;
                }
                if let Some(v) = &fill {
                    slot.clone_from(v);
                }
            }
            None if fill.is_some() => return Err(bad(CMD, format!("`{el}` has no fill"))),
            None => {}
        }
        match line_slot {
            Some(slot) => {
                if reset {
                    *slot = None;
                }
                if let Some(l) = line(slot.as_ref())? {
                    *slot = l;
                }
            }
            None if p.get("line").is_some() => return Err(bad(CMD, format!("`{el}` has no line"))),
            None => {}
        }
        Ok(())
    })
}

/// `top|bottom|left|right|topRight|none` (or the chart part's `t|b|l|r|tr`) → legend position.
fn legend_pos(v: &str) -> Option<Option<String>> {
    Some(match v {
        "none" => None,
        "top" | "t" => Some("t".into()),
        "bottom" | "b" => Some("b".into()),
        "left" | "l" => Some("l".into()),
        "right" | "r" => Some("r".into()),
        "topRight" | "tr" => Some("tr".into()),
        _ => return None,
    })
}

fn legend(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.format.legend";
    let pos = match p.get("position") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(v.as_str().and_then(legend_pos).ok_or_else(|| bad(CMD, "`position`: top|bottom|left|right|topRight|none"))?),
    };
    let overlay = bool_param(p, "overlay");
    with_chart(s, p, CMD, |c| {
        if let Some(pos) = &pos {
            c.legend.clone_from(pos);
        }
        if let Some(o) = overlay {
            c.format.legend_overlay = o;
        }
        Ok(())
    })
}

fn axis(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.format.axis";
    let name = str_param(p, "axis").ok_or_else(|| bad(CMD, "missing `axis`: category or value"))?.to_string();
    let min = auto_num(p, "min", CMD)?;
    let max = auto_num(p, "max", CMD)?;
    let major = auto_num(p, "majorUnit", CMD)?;
    let minor = auto_num(p, "minorUnit", CMD)?;
    let log = auto_num(p, "logBase", CMD)?;
    let rotation = auto_num(p, "rotation", CMD)?;
    let tick = |key: &str| -> Result<Option<Option<TickMark>>> {
        match p.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(v) => v.as_str().and_then(TickMark::from_xml).map(|t| Some(Some(t))).ok_or_else(|| bad(CMD, format!("`{key}`: none|in|out|cross"))),
        }
    };
    let (major_tick, minor_tick) = (tick("majorTick")?, tick("minorTick")?);
    let label_pos = match p.get("labelPosition") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(v.as_str().and_then(TickLabelPos::from_xml).ok_or_else(|| bad(CMD, "`labelPosition`: nextTo|low|high|none"))?)),
    };
    let crosses = match p.get("crosses") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(match s.as_str() {
            "auto" | "autoZero" => Crosses::AutoZero,
            "min" => Crosses::Min,
            "max" => Crosses::Max,
            _ => return Err(bad(CMD, "`crosses`: auto|min|max|number")),
        })),
        Some(v) => Some(Some(Crosses::At { value: v.as_f64().filter(|v| v.is_finite()).ok_or_else(|| bad(CMD, "`crosses`: auto|min|max|number"))? })),
    };
    let num_fmt =
        p.get("numFmt").map(|v| v.as_str().map(str::trim).filter(|f| !f.is_empty() && !f.eq_ignore_ascii_case("general")).map(String::from));
    let title = p.get("title").map(|v| v.as_str().map(String::from));
    let value_axis = matches!(name.as_str(), "value" | "val" | "vertical" | "y");
    if !value_axis && (min.is_some() || max.is_some()) {
        return Err(bad(CMD, "only the value axis has bounds"));
    }
    with_chart(s, p, CMD, |c| {
        if let Some(v) = min {
            c.y_min = v;
        }
        if let Some(v) = max {
            c.y_max = v;
        }
        if let (Some(a), Some(b)) = (c.y_min, c.y_max)
            && a >= b
        {
            return Err(bad(CMD, "minimum must be below maximum"));
        }
        if let Some(t) = &title {
            if value_axis {
                c.axis_titles.1.clone_from(t);
            } else {
                c.axis_titles.0.clone_from(t);
            }
        }
        if value_axis && let Some(v) = bool_param(p, "majorGridlines") {
            c.gridlines = v;
        }
        let a = axis_of(&mut c.format, &name, CMD)?;
        if let Some(v) = bool_param(p, "visible") {
            a.hidden = !v;
        }
        if let Some(v) = bool_param(p, "reverse") {
            a.reverse = v;
        }
        if let Some(v) = major {
            a.major_unit = v.filter(|u| *u > 0.0);
        }
        if let Some(v) = minor {
            a.minor_unit = v.filter(|u| *u > 0.0);
        }
        if let Some(v) = log {
            a.log_base = v.map(|b| b.clamp(2.0, 1000.0));
        }
        if let Some(v) = rotation {
            a.rotation = v.map(|r| r.clamp(-90.0, 90.0)).filter(|r| *r != 0.0);
        }
        if let Some(v) = major_tick {
            a.major_tick = v;
        }
        if let Some(v) = minor_tick {
            a.minor_tick = v;
        }
        if let Some(v) = label_pos {
            a.label_pos = v;
        }
        if let Some(v) = crosses {
            a.crosses = v;
        }
        if let Some(v) = &num_fmt {
            a.num_fmt.clone_from(v);
        }
        if !value_axis && let Some(v) = bool_param(p, "majorGridlines") {
            a.major_gridlines = v;
        }
        if let Some(v) = bool_param(p, "minorGridlines") {
            a.minor_gridlines = v;
        }
        Ok(())
    })
}

fn labels(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.format.labels";
    let position = match p.get("position") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(
            v.as_str()
                .and_then(LabelPos::from_xml)
                .ok_or_else(|| bad(CMD, "`position`: center|insideEnd|insideBase|outsideEnd|left|right|above|below|bestFit"))?,
        )),
    };
    let num_fmt =
        p.get("numFmt").map(|v| v.as_str().map(str::trim).filter(|f| !f.is_empty() && !f.eq_ignore_ascii_case("general")).map(String::from));
    let separator = p.get("separator").map(|v| v.as_str().map(String::from));
    let only = usize_param(p, "series");
    with_chart(s, p, CMD, |c| {
        if let Some(show) = bool_param(p, "show") {
            match only {
                Some(i) => c.series.get_mut(i).ok_or_else(|| bad(CMD, format!("no series {i}")))?.data_labels = Some(show),
                None => {
                    c.data_labels = show;
                    for s in &mut c.series {
                        s.data_labels = None;
                    }
                }
            }
        }
        let l = &mut c.format.labels;
        for (key, slot) in [
            ("value", &mut l.value),
            ("category", &mut l.category),
            ("seriesName", &mut l.series),
            ("percent", &mut l.percent),
            ("legendKey", &mut l.legend_key),
        ] {
            if let Some(v) = bool_param(p, key) {
                *slot = v;
            }
        }
        if let Some(v) = position {
            l.position = v;
        }
        if let Some(v) = &num_fmt {
            l.num_fmt.clone_from(v);
        }
        if let Some(v) = &separator {
            l.separator.clone_from(v);
        }
        Ok(())
    })
}

fn series_index(c: &Chart, p: &Value, cmd: &str) -> Result<usize> {
    let i = usize_param(p, "series").unwrap_or(0);
    if i >= c.series.len() {
        return Err(bad(cmd, format!("no series {i} (the chart has {})", c.series.len())));
    }
    Ok(i)
}

fn series(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.format.series";
    if p.get("series").is_none() {
        return Err(bad(CMD, "missing `series` (index)"));
    }
    let fill = p.get("fill").map(|v| fill_value(v, CMD)).transpose()?;
    let marker_fill = p.get("markerFill").map(|v| fill_value(v, CMD)).transpose()?;
    let marker = match p.get("marker") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => {
            let m = v.as_str().filter(|m| MARKERS.contains(m)).ok_or_else(|| bad(CMD, format!("`marker`: {}", MARKERS.join("|"))))?;
            Some(if m == "auto" { None } else { Some(m.to_string()) })
        }
    };
    let size = num(p, "markerSize", CMD, 2.0, 72.0)?;
    let explosion = num(p, "explosion", CMD, 0.0, 400.0)?;
    with_chart(s, p, CMD, |c| {
        let i = series_index(c, p, CMD)?;
        let Some(se) = c.series.get_mut(i) else { return Err(bad(CMD, "no such series")) };
        if bool_or(p, "reset", false) {
            se.fill = None;
            se.line = None;
            se.marker = None;
            se.format = SeriesFormat::default();
            se.point_fills.clear();
        }
        if let Some(f) = &fill {
            se.fill.clone_from(f);
        }
        if let Some(v) = p.get("line") {
            se.line = line_value(v, se.line.as_ref(), CMD)?;
        }
        if let Some(m) = &marker {
            se.marker.clone_from(m);
        }
        if let Some(v) = size {
            se.format.marker_size = Some(v.round() as u32);
        }
        if let Some(f) = &marker_fill {
            se.format.marker_fill.clone_from(f);
        }
        if let Some(v) = p.get("markerLine") {
            se.format.marker_line = line_value(v, se.format.marker_line.as_ref(), CMD)?;
        }
        if let Some(v) = bool_param(p, "smooth") {
            se.smooth = v;
        }
        if let Some(v) = bool_param(p, "invertIfNegative") {
            se.format.invert_if_negative = v;
        }
        if let Some(v) = explosion {
            se.format.explosion = Some(v.round() as u32).filter(|x| *x > 0);
        }
        if let Some(v) = bool_param(p, "secondaryAxis") {
            se.secondary_axis = v;
        }
        Ok(())
    })
}

fn point(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.format.point";
    let k = usize_param(p, "point").ok_or_else(|| bad(CMD, "missing `point` (index)"))?;
    let fill = p.get("fill").map(|v| fill_value(v, CMD)).transpose()?;
    let explosion = num(p, "explosion", CMD, 0.0, 400.0)?;
    with_chart(s, p, CMD, |c| {
        let i = series_index(c, p, CMD)?;
        let n = c.categories.len().max(c.series.get(i).map(|s| s.values.len()).unwrap_or(0));
        if k >= n {
            return Err(bad(CMD, format!("no point {k} (the series has {n})")));
        }
        let Some(se) = c.series.get_mut(i) else { return Err(bad(CMD, "no such series")) };
        if bool_or(p, "reset", false) {
            se.point_fills.retain(|(j, _)| *j != k);
            se.format.point_explosion.retain(|(j, _)| *j != k);
        }
        if let Some(f) = &fill {
            se.point_fills.retain(|(j, _)| *j != k);
            if let Some(f) = f {
                se.point_fills.push((k, f.clone()));
                se.point_fills.sort_by_key(|(j, _)| *j);
            }
        }
        if let Some(x) = explosion {
            se.format.point_explosion.retain(|(j, _)| *j != k);
            let x = x.round() as u32;
            if x > 0 {
                se.format.point_explosion.push((k, x));
                se.format.point_explosion.sort_by_key(|(j, _)| *j);
            }
        }
        Ok(())
    })
}

fn options(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.format.options";
    let gap = num(p, "gapWidth", CMD, 0.0, 500.0)?;
    let overlap = num(p, "overlap", CMD, -100.0, 100.0)?;
    let hole = num(p, "holeSize", CMD, 10.0, 90.0)?;
    let angle = num(p, "firstSliceAngle", CMD, 0.0, 360.0)?;
    let vary = bool_param(p, "varyColors");
    with_chart(s, p, CMD, |c| {
        if let Some(v) = gap {
            c.gap_width = v / 100.0;
        }
        if let Some(v) = overlap {
            c.overlap = v / 100.0;
        }
        if let Some(v) = hole {
            c.hole_size = v / 100.0;
        }
        if let Some(v) = angle {
            c.format.first_slice_angle = v.rem_euclid(360.0);
        }
        if let Some(v) = vary {
            c.vary_colors = v;
        }
        Ok(())
    })
}

fn colors(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.colors";
    let id = str_param(p, "palette").ok_or_else(|| bad(CMD, "missing `palette`"))?;
    let palette = PALETTES
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(k, _)| *k)
        .ok_or_else(|| bad(CMD, format!("unknown palette `{id}`: {}", PALETTES.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", "))))?;
    with_chart(s, p, CMD, |c| {
        c.palette = palette.to_string();
        Ok(())
    })
}

/// Reset to Match Style: drop the chart's own formatting (fills, lines, fonts, markers, point
/// colours), keeping its elements, options and palette.
pub fn reset_format(c: &mut Chart) {
    let keep_angle = c.format.first_slice_angle;
    let labels = DataLabels { fill: None, line: None, text: ChartText::default(), ..c.format.labels.clone() };
    let keep_axis = |a: &AxisFormat| AxisFormat {
        text: ChartText::default(),
        title_text: ChartText::default(),
        line: None,
        major_grid_line: None,
        minor_grid_line: None,
        ..a.clone()
    };
    let (cat, val) = (keep_axis(&c.format.cat_axis), keep_axis(&c.format.val_axis));
    let overlays = (c.format.title_overlay, c.format.legend_overlay);
    c.format = ChartFormat { cat_axis: cat, val_axis: val, labels, first_slice_angle: keep_angle, ..Default::default() };
    (c.format.title_overlay, c.format.legend_overlay) = overlays;
    c.chart_fill = None;
    c.plot_fill = None;
    c.text_color = None;
    for s in &mut c.series {
        s.fill = None;
        s.line = None;
        s.marker = None;
        s.point_fills.clear();
        let explosion = (s.format.explosion, std::mem::take(&mut s.format.point_explosion), s.format.invert_if_negative);
        s.format = SeriesFormat { explosion: explosion.0, point_explosion: explosion.1, invert_if_negative: explosion.2, ..Default::default() };
    }
}

fn reset_style(s: &mut Session, p: &Value) -> Result<Value> {
    with_chart(s, p, "chart.resetStyle", |c| {
        reset_format(c);
        Ok(())
    })
}

fn tx(lum_mod: i32, lum_off: i32) -> ColorRef {
    ColorRef::scheme(SchemeSlot::Tx1).with(ColorTransform::LumMod(lum_mod)).with(ColorTransform::LumOff(lum_off))
}

/// Our Chart Styles: formatting presets over a reset chart.
pub fn apply_style(c: &mut Chart, style: usize) {
    reset_format(c);
    let line_like = matches!(c.kind, ChartType::Line | ChartType::LineMarkers | ChartType::StackedLine | ChartType::Scatter | ChartType::Radar);
    let pie = matches!(c.kind, ChartType::Pie | ChartType::Doughnut);
    let n = c.series.len();
    match style {
        // Clean: labels on the bars, no gridlines nor value axis.
        2 => {
            c.data_labels = true;
            c.gridlines = false;
            c.format.val_axis.hidden = !pie;
            c.format.labels.position = Some(if pie {
                LabelPos::OutsideEnd
            } else if line_like {
                LabelPos::Above
            } else if c.kind.stacked() {
                LabelPos::Center
            } else {
                LabelPos::OutsideEnd
            });
        }
        // Shaded plot area with light gridlines.
        3 => {
            c.plot_fill = Some(Fill::solid(tx(5_000, 95_000)));
            c.gridlines = true;
            c.format.val_axis.major_grid_line = Some(Line::solid(ColorRef::scheme(SchemeSlot::Bg1), 1.0));
        }
        // Dark background.
        4 => {
            c.chart_fill = Some(Fill::solid(tx(85_000, 15_000)));
            c.format.text.color = Some(ColorRef::scheme(SchemeSlot::Bg1));
            c.format.val_axis.major_grid_line = Some(Line::solid(tx(75_000, 25_000), 0.75));
            for s in &mut c.series {
                s.line = if pie { Some(Line::solid(tx(85_000, 15_000), 1.0)) } else { None };
            }
        }
        // Gradient series.
        5 => {
            for (i, s) in c.series.iter_mut().enumerate() {
                let base = palette_color(&c.palette, i, n);
                s.fill = Some(Fill::Gradient(Gradient {
                    stops: vec![
                        GradientStop { pos: 0.0, color: base.clone().with(ColorTransform::Tint(66_000)) },
                        GradientStop { pos: 1.0, color: base },
                    ],
                    shape: GradientShape::Linear { angle: 90.0, scaled: false },
                    rotate_with_shape: true,
                    tile: [0.0; 4],
                }));
            }
        }
        // Bold title, legend on top, minor gridlines.
        6 => {
            c.format.title_text = ChartText { size: Some(18.0), bold: Some(true), ..Default::default() };
            if c.legend.is_some() {
                c.legend = Some("t".into());
            }
            c.gridlines = true;
            c.format.val_axis.minor_gridlines = !pie;
            c.format.val_axis.minor_grid_line = Some(Line::solid(tx(5_000, 95_000), 0.5));
            c.format.cat_axis.line = Some(Line::solid(tx(65_000, 35_000), 1.0));
        }
        // Outlined: white outlines, a border round the chart.
        7 => {
            for s in &mut c.series {
                s.line = Some(Line::solid(ColorRef::scheme(SchemeSlot::Bg1), 1.5));
            }
            c.format.chart_line = Some(Line::solid(tx(25_000, 75_000), 1.0));
            c.format.rounded_corners = true;
        }
        // Bold lines and markers.
        8 => {
            for s in &mut c.series {
                if line_like {
                    s.line = Some(Line { width: Some(3.5), ..Default::default() });
                    s.marker = Some("circle".into());
                    s.format.marker_size = Some(7);
                } else {
                    s.line = Some(Line::solid(tx(85_000, 15_000), 0.75));
                }
            }
            c.format.text = ChartText { bold: Some(true), ..Default::default() };
        }
        _ => {}
    }
}

fn style(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.style";
    let n = usize_param(p, "style").filter(|n| (1..=STYLES).contains(n)).ok_or_else(|| bad(CMD, format!("`style`: 1-{STYLES}")))?;
    with_chart(s, p, CMD, |c| {
        apply_style(c, n);
        Ok(())
    })
}

/// Our Quick Layouts: which elements show and where.
pub fn apply_layout(c: &mut Chart, layout: usize) {
    let pie = matches!(c.kind, ChartType::Pie | ChartType::Doughnut);
    let t = Some(c_title(c));
    let (title, legend, labels, grid, axis_titles, val_hidden, minor) = match layout {
        1 => (t, Some("r"), false, true, false, false, false),
        2 => (t, Some("t"), true, false, false, !pie, false),
        3 => (t, Some("b"), false, true, false, false, false),
        4 => (None, Some("b"), true, false, false, false, false),
        5 => (t, None, false, true, true, false, false),
        6 => (t, Some("r"), false, true, true, false, false),
        7 => (t, Some("r"), false, true, true, false, true),
        _ => (t, None, true, false, false, !pie, false),
    };
    c.title = title;
    c.legend = legend.map(String::from);
    c.data_labels = labels;
    for s in &mut c.series {
        s.data_labels = None;
    }
    c.gridlines = grid;
    c.format.val_axis.minor_gridlines = minor;
    c.format.val_axis.hidden = val_hidden;
    if layout == 8 || (labels && pie) {
        c.format.labels.category = pie;
        c.format.labels.percent = pie && layout == 8;
        c.format.labels.value = !(pie && layout == 8);
    }
    if axis_titles && !pie {
        if c.axis_titles.1.is_none() {
            c.axis_titles.1 = Some("Axis Title".into());
        }
        if layout != 6 && c.axis_titles.0.is_none() {
            c.axis_titles.0 = Some("Axis Title".into());
        }
    } else {
        c.axis_titles = (None, None);
    }
}

fn c_title(c: &Chart) -> String {
    c.title.clone().unwrap_or_else(|| "Chart Title".into())
}

fn quick_layout(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "chart.quickLayout";
    let n = usize_param(p, "layout").filter(|n| (1..=LAYOUTS).contains(n)).ok_or_else(|| bad(CMD, format!("`layout`: 1-{LAYOUTS}")))?;
    with_chart(s, p, CMD, |c| {
        apply_layout(c, n);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn session_with_chart(kind: &str) -> (Session, u64) {
        let mut s = Session::with_new();
        let r = s.execute("insert.chart", &json!({"type": kind})).unwrap();
        let id = r["id"].as_u64().unwrap();
        s.execute("edit.select", &json!({"ids": [id]})).unwrap();
        (s, id)
    }

    fn chart(s: &Session, id: u64) -> Chart {
        let st = s.doc().unwrap();
        match &st.shape(deckcraft_model::ShapeId(id as u32)).unwrap().kind {
            ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("not a chart"),
        }
    }

    #[test]
    fn elements_take_fill_line_and_font() {
        let (mut s, id) = session_with_chart("column");
        s.execute("chart.format.element", &json!({"element": "chartArea", "fill": "#ffeecc", "line": {"color": "accent2", "width": 2, "dash": "dash"}, "font": {"size": 12, "color": "#333333"}})).unwrap();
        s.execute(
            "chart.format.element",
            &json!({"element": "plotArea", "fill": {"gradient": {"stops": [[0, "#ffffff"], [1, "accent1"]], "angle": 90}}}),
        )
        .unwrap();
        s.execute(
            "chart.format.element",
            &json!({"element": "title", "font": {"size": 20, "bold": true, "italic": true, "font": "Liberation Serif"}, "overlay": true}),
        )
        .unwrap();
        s.execute("chart.format.element", &json!({"element": "legend", "fill": "none", "line": "none", "font": {"size": 9}})).unwrap();
        s.execute("chart.format.element", &json!({"element": "majorGridlines", "line": {"color": "#cccccc", "width": 0.5, "dash": "sysDot"}}))
            .unwrap();
        s.execute("chart.format.element", &json!({"element": "valueAxisTitle", "font": {"color": "accent1"}})).unwrap();
        let c = chart(&s, id);
        assert!(matches!(c.chart_fill, Some(Fill::Solid { .. })));
        let l = c.format.chart_line.as_ref().unwrap();
        assert_eq!((l.width, l.dash.clone()), (Some(2.0), Some(Dash::Preset { name: "dash".into() })));
        assert_eq!(c.format.text.size, Some(12.0));
        assert!(matches!(c.plot_fill, Some(Fill::Gradient(_))));
        assert_eq!((c.format.title_text.size, c.format.title_text.bold, c.format.title_overlay), (Some(20.0), Some(true), true));
        assert_eq!(c.format.title_text.font.as_deref(), Some("Liberation Serif"));
        assert_eq!(c.format.legend_fill, Some(Fill::None));
        assert_eq!(c.format.val_axis.major_grid_line.as_ref().and_then(|l| l.width), Some(0.5));
        assert!(c.format.val_axis.title_text.color.is_some());
        // Reset one element, errors for what it doesn't have.
        s.execute("chart.format.element", &json!({"element": "chartArea", "reset": true})).unwrap();
        let c = chart(&s, id);
        assert!(c.chart_fill.is_none() && c.format.chart_line.is_none() && c.format.text.is_empty());
        assert!(s.execute("chart.format.element", &json!({"element": "valueAxis", "fill": "#ff0000"})).is_err());
        assert!(s.execute("chart.format.element", &json!({"element": "nope"})).is_err());
        assert!(s.execute("chart.format.element", &json!({"element": "title", "fill": 7})).is_err());
        // Undo restores.
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(chart(&s, id).chart_fill.is_some());
    }

    #[test]
    fn axis_options() {
        let (mut s, id) = session_with_chart("column");
        s.execute(
            "chart.format.axis",
            &json!({"axis": "value", "min": 0, "max": 10, "majorUnit": 2, "minorUnit": 0.5, "numFmt": "0.0", "reverse": true,
                    "majorTick": "out", "minorTick": "in", "labelPosition": "low", "crosses": 5, "minorGridlines": true, "title": "Revenue", "visible": true}),
        )
        .unwrap();
        s.execute("chart.format.axis", &json!({"axis": "category", "rotation": -45, "majorGridlines": true, "visible": false, "title": "Quarter"}))
            .unwrap();
        let c = chart(&s, id);
        assert_eq!((c.y_min, c.y_max), (Some(0.0), Some(10.0)));
        let v = &c.format.val_axis;
        assert_eq!((v.major_unit, v.minor_unit, v.num_fmt.as_deref(), v.reverse), (Some(2.0), Some(0.5), Some("0.0"), true));
        assert_eq!((v.major_tick, v.minor_tick, v.label_pos), (Some(TickMark::Outside), Some(TickMark::Inside), Some(TickLabelPos::Low)));
        assert_eq!(v.crosses, Some(Crosses::At { value: 5.0 }));
        assert!(v.minor_gridlines);
        assert_eq!(c.axis_titles, (Some("Quarter".into()), Some("Revenue".into())));
        assert_eq!((c.format.cat_axis.rotation, c.format.cat_axis.hidden, c.format.cat_axis.major_gridlines), (Some(-45.0), true, true));
        s.execute("chart.format.axis", &json!({"axis": "value", "min": "auto", "max": null, "logBase": 10, "numFmt": "General"})).unwrap();
        let c = chart(&s, id);
        assert_eq!((c.y_min, c.y_max, c.format.val_axis.log_base, c.format.val_axis.num_fmt.clone()), (None, None, Some(10.0), None));
        assert!(s.execute("chart.format.axis", &json!({"axis": "category", "min": 1})).is_err());
        assert!(s.execute("chart.format.axis", &json!({"axis": "value", "min": 5, "max": 1})).is_err());
        assert!(s.execute("chart.format.axis", &json!({"axis": "value", "majorTick": "sideways"})).is_err());
        assert!(s.execute("chart.format.axis", &json!({"axis": "z"})).is_err());
    }

    #[test]
    fn labels_legend_series_points_options() {
        let (mut s, id) = session_with_chart("pie");
        s.execute(
            "chart.format.labels",
            &json!({"show": true, "category": true, "percent": true, "value": false, "position": "outsideEnd", "numFmt": "0.0%", "separator": " / "}),
        )
        .unwrap();
        s.execute("chart.format.legend", &json!({"position": "left", "overlay": true})).unwrap();
        s.execute("chart.format.point", &json!({"point": 2, "fill": "#ff0000", "explosion": 20})).unwrap();
        s.execute("chart.format.series", &json!({"series": 0, "explosion": 5, "line": {"color": "#ffffff", "width": 2}})).unwrap();
        s.execute("chart.format.options", &json!({"firstSliceAngle": 90, "holeSize": 40, "varyColors": true})).unwrap();
        let c = chart(&s, id);
        assert!(c.data_labels);
        let l = &c.format.labels;
        assert_eq!(
            (l.value, l.category, l.percent, l.position, l.num_fmt.as_deref(), l.separator.as_deref()),
            (false, true, true, Some(LabelPos::OutsideEnd), Some("0.0%"), Some(" / "))
        );
        assert_eq!((c.legend.as_deref(), c.format.legend_overlay), (Some("l"), true));
        assert_eq!(c.series[0].point_fills.len(), 1);
        assert_eq!(c.series[0].format.point_explosion, vec![(2, 20)]);
        assert_eq!(c.series[0].format.explosion, Some(5));
        assert_eq!((c.format.first_slice_angle, c.hole_size), (90.0, 0.4));
        s.execute("chart.format.point", &json!({"point": 2, "reset": true})).unwrap();
        let c = chart(&s, id);
        assert!(c.series[0].point_fills.is_empty() && c.series[0].format.point_explosion.is_empty());
        assert!(s.execute("chart.format.point", &json!({"point": 99, "fill": "#ff0000"})).is_err());
        assert!(s.execute("chart.format.series", &json!({"series": 9, "fill": "#ff0000"})).is_err());
        assert!(s.execute("chart.format.legend", &json!({"position": "middle"})).is_err());
        s.execute("chart.format.legend", &json!({"position": "none"})).unwrap();
        assert!(chart(&s, id).legend.is_none());

        let (mut s, id) = session_with_chart("line");
        s.execute("chart.format.series", &json!({"series": 1, "fill": "accent6", "line": {"width": 3, "dash": "lgDash"}, "marker": "diamond", "markerSize": 9, "markerFill": "#00ff00", "smooth": true})).unwrap();
        s.execute("chart.format.labels", &json!({"show": true, "series": 0})).unwrap();
        let c = chart(&s, id);
        let se = &c.series[1];
        assert_eq!((se.marker.as_deref(), se.format.marker_size, se.smooth), (Some("diamond"), Some(9), true));
        assert_eq!(se.line.as_ref().and_then(|l| l.width), Some(3.0));
        assert_eq!(c.series[0].data_labels, Some(true));
        assert!(s.execute("chart.format.series", &json!({"series": 0, "marker": "blob"})).is_err());
        assert!(s.execute("chart.format.series", &json!({"series": 0, "line": {"dash": "zigzag"}})).is_err());

        let (mut s, id) = session_with_chart("column");
        s.execute("chart.format.options", &json!({"gapWidth": 50, "overlap": 30})).unwrap();
        s.execute("chart.format.series", &json!({"series": 0, "invertIfNegative": true})).unwrap();
        let c = chart(&s, id);
        assert_eq!((c.gap_width, c.overlap, c.series[0].format.invert_if_negative), (0.5, 0.3, true));
    }

    #[test]
    fn colors_styles_layouts_and_reset() {
        let (mut s, id) = session_with_chart("column");
        s.execute("chart.colors", &json!({"palette": "monochrome3"})).unwrap();
        assert_eq!(chart(&s, id).palette, "monochrome3");
        assert!(s.execute("chart.colors", &json!({"palette": "rainbow"})).is_err());
        for n in 1..=STYLES {
            s.execute("chart.style", &json!({"style": n})).unwrap();
        }
        s.execute("chart.style", &json!({"style": 4})).unwrap();
        assert!(chart(&s, id).chart_fill.is_some());
        assert!(s.execute("chart.style", &json!({"style": 0})).is_err());
        s.execute("chart.style", &json!({"style": 5})).unwrap();
        assert!(chart(&s, id).series.iter().all(|s| matches!(s.fill, Some(Fill::Gradient(_)))));
        s.execute("chart.resetStyle", &json!({})).unwrap();
        let c = chart(&s, id);
        assert!(c.series.iter().all(|s| s.fill.is_none()) && c.chart_fill.is_none() && c.format.text.is_empty());
        assert_eq!(c.palette, "monochrome3");
        for n in 1..=LAYOUTS {
            s.execute("chart.quickLayout", &json!({"layout": n})).unwrap();
        }
        s.execute("chart.quickLayout", &json!({"layout": 5})).unwrap();
        let c = chart(&s, id);
        assert!(c.legend.is_none() && c.axis_titles.0.is_some() && c.axis_titles.1.is_some() && c.title.is_some());
        s.execute("chart.quickLayout", &json!({"layout": 4})).unwrap();
        let c = chart(&s, id);
        assert!(c.title.is_none() && c.data_labels && c.axis_titles == (None, None));
        assert!(s.execute("chart.quickLayout", &json!({"layout": 99})).is_err());
    }

    #[test]
    fn not_a_chart() {
        let mut s = Session::with_new();
        s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 100, 100]})).unwrap();
        assert!(s.execute("chart.format.element", &json!({"element": "chartArea", "fill": "#ff0000"})).is_err());
        assert!(s.execute("chart.style", &json!({"style": 1})).is_err());
    }
}
