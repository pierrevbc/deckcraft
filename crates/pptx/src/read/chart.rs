//! Chart parts (`c:chartSpace`) → [`Chart`]. The original XML is kept in `Chart.raw`, the data
//! workbook it points to in `Chart.workbook`, and where each series reads its data in that
//! workbook in `Chart.source`.

use deckcraft_model::MediaId;
use deckcraft_model::chart::{BarShape, Chart, ChartSource, ChartType, Series, SeriesSource, ThreeD};
use deckcraft_model::style::Fill;

use super::Imp;
use crate::xml::El;

mod format;

/// Fills of chart parts never reference images we keep; read colour fills only.
fn simple_fill(sp: Option<&El>) -> Option<Fill> {
    format::fill_of(sp)
}

fn simple_line(sp: Option<&El>) -> Option<deckcraft_model::Line> {
    format::line_of(sp)
}

fn rich_text(t: &El) -> Option<String> {
    let tx = t.child("tx")?;
    if let Some(r) = tx.child("rich") {
        let s = r.children_named("p").map(|p| p.text()).collect::<Vec<_>>().join("\n");
        return Some(s);
    }
    tx.find("v").map(|v| v.text())
}

/// String values from `c:cat`/`c:tx` (str or num refs, caches or literals).
fn strings(e: &El) -> Vec<String> {
    let cache = e.find("strCache").or_else(|| e.find("numCache")).or_else(|| e.find("strLit")).or_else(|| e.find("numLit"));
    if let Some(c) = cache {
        let n = c.child("ptCount").and_then(|p| p.u32("val")).unwrap_or(0).min(100_000) as usize;
        let mut out = vec![String::new(); n];
        for pt in c.children_named("pt").take(100_000) {
            let i = pt.u32("idx").unwrap_or(0) as usize;
            let v = pt.child("v").map(|v| v.text()).unwrap_or_default();
            if i >= out.len() {
                if i >= 100_000 {
                    continue;
                }
                out.resize(i + 1, String::new());
            }
            if let Some(s) = out.get_mut(i) {
                *s = v;
            }
        }
        return out;
    }
    if let Some(ml) = e.find("multiLvlStrCache") {
        // Use the innermost level.
        if let Some(l) = ml.child("lvl") {
            return l.children_named("pt").map(|p| p.child("v").map(|v| v.text()).unwrap_or_default()).collect();
        }
    }
    if let Some(v) = e.child("v") {
        return vec![v.text()];
    }
    vec![]
}

/// The workbook formula of `c:tx`, `c:cat`, `c:val`… (`Sheet1!$B$2:$B$5`). Multi-level category
/// references span several columns: they are left out.
fn formula(e: Option<&El>) -> Option<String> {
    let r = e?.elements().find(|r| r.is("strRef") || r.is("numRef"))?;
    let f = r.child("f")?.text();
    let f = f.trim();
    (!f.is_empty()).then(|| f.to_string())
}

fn numbers(e: &El) -> Vec<Option<f64>> {
    strings(e).into_iter().map(|s| s.trim().parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}

/// Parse a chart part. Returns `None` for something that isn't a chart we can read.
pub fn read_chart_xml(bytes: &[u8]) -> Option<Chart> {
    let doc = crate::xml::parse(bytes).ok()?;
    let mut root = doc.root;
    super::resolve_mc(&mut root, 0);
    if !root.is("chartSpace") {
        return None;
    }
    let mut c = chart_from(&root)?;
    c.raw = Some(String::from_utf8_lossy(bytes).into_owned());
    Some(c)
}

/// Parse the chart part `name` and keep the workbook that holds its data.
pub fn read_chart_part(imp: &mut Imp, name: &str, bytes: &[u8]) -> Option<Chart> {
    let mut c = read_chart_xml(bytes)?;
    c.workbook = workbook(imp, name, bytes);
    Some(c)
}

/// The target of `c:externalData`: an embedded workbook (`package`, or an older `oleObject`)
/// read as a media item, or a link to an outside file.
fn workbook(imp: &mut Imp, name: &str, bytes: &[u8]) -> Option<MediaId> {
    let doc = crate::xml::parse(bytes).ok()?;
    let rid = doc.root.child("externalData")?.attr("r:id")?.to_string();
    let rels = imp.pkg.rels(name);
    let rel = rels.get(&rid)?;
    if rel.external {
        return Some(imp.media_link(&rel.target));
    }
    if !matches!(rel.kind.as_str(), "package" | "oleObject") {
        return None;
    }
    imp.media_for(&rel.target)
}

fn is_true(v: &str) -> bool {
    v == "1" || v == "true"
}

/// The series draws no line (`c:spPr/a:ln/a:noFill`).
fn no_line(s: &El) -> bool {
    s.path(&["spPr", "ln"]).is_some_and(|ln| ln.child("noFill").is_some())
}

/// The series draws no markers (`c:marker/c:symbol val="none"`).
fn no_marker(s: &El) -> bool {
    s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")) == Some("none")
}

/// The chart type of plot group `g` (`c:barChart`, `c:lineChart`…).
fn group_kind(g: &El) -> Option<ChartType> {
    let name = g.local();
    let grouping = g.child_val("grouping").unwrap_or(if name.starts_with("bar") { "clustered" } else { "standard" });
    let series: Vec<&El> = g.children_named("ser").collect();
    let all = |f: fn(&El) -> bool| !series.is_empty() && series.iter().all(|s| f(s));
    Some(match name {
        "barChart" | "bar3DChart" => {
            let bar = g.child_val("barDir") == Some("bar");
            match (bar, grouping) {
                (false, "stacked") => ChartType::StackedColumn,
                (false, "percentStacked") => ChartType::PercentColumn,
                (false, _) => ChartType::Column,
                (true, "stacked") => ChartType::StackedBar,
                (true, "percentStacked") => ChartType::PercentBar,
                (true, _) => ChartType::Bar,
            }
        }
        "lineChart" | "line3DChart" => {
            let markers = g.child_val("marker").is_some_and(is_true) && series.iter().all(|s| !no_marker(s));
            let lines = !all(no_line);
            match (grouping, markers) {
                ("stacked", false) => ChartType::StackedLine,
                ("stacked", true) => ChartType::StackedLineMarkers,
                ("percentStacked", false) => ChartType::PercentLine,
                ("percentStacked", true) => ChartType::PercentLineMarkers,
                (_, true) if !lines => ChartType::LinePoints,
                (_, true) => ChartType::LineMarkers,
                (_, false) => ChartType::Line,
            }
        }
        "areaChart" | "area3DChart" => match grouping {
            "stacked" => ChartType::StackedArea,
            "percentStacked" => ChartType::PercentArea,
            _ => ChartType::Area,
        },
        "pieChart" | "pie3DChart" => {
            if series.iter().any(|s| explosion(s).is_some_and(|e| e > 0)) {
                ChartType::ExplodedPie
            } else {
                ChartType::Pie
            }
        }
        "ofPieChart" => {
            if g.child_val("ofPieType") == Some("bar") {
                ChartType::BarOfPie
            } else {
                ChartType::PieOfPie
            }
        }
        "doughnutChart" => {
            if series.iter().any(|s| explosion(s).is_some_and(|e| e > 0)) {
                ChartType::ExplodedDoughnut
            } else {
                ChartType::Doughnut
            }
        }
        "scatterChart" => {
            let style = g.child_val("scatterStyle").unwrap_or("marker");
            let lines = !all(no_line) && !matches!(style, "marker" | "none");
            let markers = !all(no_marker) && !matches!(style, "line" | "smooth");
            match (lines, markers) {
                (true, true) => ChartType::ScatterLinesMarkers,
                (true, false) => ChartType::ScatterLines,
                _ => ChartType::Scatter,
            }
        }
        "bubbleChart" => ChartType::Bubble,
        "radarChart" => {
            let style = g.child_val("radarStyle").unwrap_or("standard");
            let markers = style == "marker" && !all(no_marker);
            match (style, markers, !all(no_line)) {
                ("filled", _, _) => ChartType::FilledRadar,
                (_, true, true) => ChartType::RadarMarkers,
                (_, true, false) => ChartType::RadarPoints,
                _ => ChartType::Radar,
            }
        }
        "stockChart" => ChartType::Stock,
        "surfaceChart" | "surface3DChart" => ChartType::Surface,
        _ => return None,
    })
}

/// How far a pie series' slices move out (`c:explosion`), % of the radius.
fn explosion(s: &El) -> Option<u32> {
    s.child("explosion").and_then(|e| e.u32("val")).map(|v| v.min(400))
}

/// The 3-D view of a chart with 3-D plot group `g`.
fn three_d(ch: &El, g: &El, kind: ChartType) -> ThreeD {
    let mut t = ThreeD::default();
    if let Some(v) = ch.child("view3D") {
        let num = |n: &str| v.child(n).and_then(|e| e.i64("val"));
        t.rot_x = num("rotX").map_or(if kind.is_pie() { 30 } else { t.rot_x }, |v| v.clamp(-90, 90) as i32);
        t.rot_y = num("rotY").map_or(if kind.is_pie() { 0 } else { t.rot_y }, |v| v.clamp(0, 360) as u32);
        t.perspective = num("perspective").map_or(t.perspective, |v| v.clamp(0, 240) as u32);
        t.depth_percent = num("depthPercent").map_or(t.depth_percent, |v| v.clamp(20, 2000) as u32);
        t.right_angle_axes = v.child("rAngAx").and_then(|e| e.attr("val")).map_or(!kind.is_pie(), is_true);
    } else if kind.is_pie() {
        (t.rot_x, t.rot_y, t.right_angle_axes) = (30, 0, false);
    }
    let shape = g.child_val("shape").or_else(|| g.children_named("ser").find_map(|s| s.child_val("shape")));
    t.shape = shape.map_or(BarShape::Box, BarShape::from_ooxml);
    let grouping = g.child_val("grouping").unwrap_or("standard");
    t.deep = match g.local() {
        "line3DChart" => true,
        "bar3DChart" | "area3DChart" => grouping == "standard",
        _ => false,
    };
    t
}

/// Our extension says the lines are stepped.
fn stepped(root: &El) -> bool {
    root.child("extLst")
        .into_iter()
        .flat_map(|l| l.children_named("ext"))
        .filter(|e| e.attr("uri") == Some(crate::CHART_EXT_URI))
        .any(|e| e.child("stepped").and_then(|s| s.attr("val")).is_some_and(is_true))
}

pub(crate) fn chart_from(root: &El) -> Option<Chart> {
    let ch = root.child("chart")?;
    let pa = ch.child("plotArea")?;
    let mut c = Chart { categories: vec![], series: vec![], title: None, legend: None, gridlines: false, ..Default::default() };
    c.style = root.child_val("style").and_then(|v| v.parse().ok()).unwrap_or(2);
    if let Some(t) = ch.child("title") {
        c.title = Some(rich_text(t).unwrap_or_else(|| "Chart Title".into()));
    }
    if ch.child("autoTitleDeleted").and_then(|a| a.bool("val")) == Some(true) {
        c.title = None;
    }
    c.legend = ch.child("legend").map(|l| l.child_val("legendPos").unwrap_or("r").to_string());
    c.legend_overlay = ch.child("legend").and_then(|l| l.child("overlay")).and_then(|o| o.bool("val")).unwrap_or(false);
    c.chart_fill = simple_fill(root.child("spPr"));
    c.plot_fill = simple_fill(pa.child("spPr"));

    let mut kinds: Vec<ChartType> = vec![];
    // The value axis the first plot group uses is the primary one; a group on another value axis
    // (a combo chart's line on the right) is on the secondary axis.
    let val_ids: Vec<(&str, bool)> =
        pa.children_named("valAx").filter_map(|a| Some((a.child_val("axId")?, matches!(a.child_val("axPos"), Some("l" | "r"))))).collect();
    // A group's value axis: its vertical one when it has two (scatter), else its one.
    let group_val_axis = |g: &El| {
        let ids: Vec<(&str, bool)> =
            g.children_named("axId").filter_map(|a| a.attr("val")).filter_map(|id| val_ids.iter().find(|(v, _)| *v == id).copied()).collect();
        ids.iter().find(|(_, vertical)| *vertical).or(ids.first()).map(|(id, _)| id.to_string())
    };
    // The primary value axis is the one on the left (bottom for bar charts); the secondary one
    // sits on the right or top, or crosses the category axis at its maximum.
    let secondary_placed = |a: &El| a.child_val("crosses") == Some("max") || matches!(a.child_val("axPos"), Some("r" | "t"));
    let used: Vec<String> = pa.elements().filter(|g| g.local().ends_with("Chart")).filter_map(group_val_axis).collect();
    let primary_val: Option<String> = used
        .iter()
        .find(|id| pa.children_named("valAx").find(|a| a.child_val("axId") == Some(id.as_str())).is_some_and(|a| !secondary_placed(a)))
        .or(used.first())
        .cloned();
    let mut sources: Vec<SeriesSource> = vec![];
    // Per plot group: its element name and its series (a range of `c.series`).
    let mut groups: Vec<(&str, std::ops::Range<usize>)> = vec![];
    for g in pa.elements() {
        let name = g.local();
        if !name.ends_with("Chart") {
            continue;
        }
        let Some(kind) = group_kind(g) else { continue };
        if name.ends_with("3DChart") {
            c.variant.three_d = Some(three_d(ch, g, kind));
        }
        match name {
            "pieChart" | "pie3DChart" | "doughnutChart" if kind.exploded() => {
                c.variant.explosion = g.children_named("ser").filter_map(explosion).max().unwrap_or(0);
            }
            "ofPieChart" => {
                if g.child_val("splitType") == Some("pos") {
                    c.variant.split_pos =
                        g.child("splitPos").and_then(|v| v.f64("val")).filter(|v| v.is_finite()).map_or(0, |v| v.round().clamp(0.0, 1000.0) as u32);
                }
                c.variant.second_size = g.child("secondPieSize").and_then(|v| v.u32("val")).filter(|v| *v != 75).map_or(0, |v| v.clamp(5, 200));
            }
            _ => {}
        }
        let first_series = c.series.len();
        if let Some(v) = g.child_val("varyColors") {
            c.vary_colors = v == "1" || v == "true";
        }
        // The gap of a pie of pie is between its two plots: not the bars' gap width.
        if let Some(v) = g.child_val("gapWidth").filter(|_| name != "ofPieChart").and_then(|v| v.parse::<f64>().ok()) {
            c.gap_width = (v / 100.0).clamp(0.0, 5.0);
        }
        if let Some(v) = g.child_val("overlap").and_then(|v| v.parse::<f64>().ok()) {
            c.overlap = (v / 100.0).clamp(-1.0, 1.0);
        }
        if let Some(v) = g.child_val("holeSize").and_then(|v| v.parse::<f64>().ok()) {
            c.hole_size = (v / 100.0).clamp(0.0, 0.9);
        }
        if g.path(&["dLbls", "showVal"]).and_then(|v| v.bool("val")) == Some(true) {
            c.data_labels = true;
        }
        kinds.push(kind);
        let val_axis = group_val_axis(g);
        let secondary = val_axis.is_some() && val_axis != primary_val;
        for s in g.children_named("ser").take(1000) {
            let mut ser = Series {
                name: s.child("tx").map(strings).and_then(|v| v.into_iter().next()).unwrap_or_default(),
                fill: simple_fill(s.child("spPr")),
                line: simple_line(s.child("spPr")),
                smooth: s.child("smooth").and_then(|v| v.bool("val")).unwrap_or(false),
                marker: s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")).filter(|m| *m != "none").map(String::from),
                kind: Some(kind),
                secondary_axis: secondary,
                ..Default::default()
            };
            if let Some(d) = s.child("dLbls").and_then(|d| d.child("showVal")).and_then(|v| v.bool("val")) {
                ser.data_labels = Some(d);
            }
            // Data labels' number format: the series' labels', the group's, else the source data's.
            let label_fmt = |d: Option<&El>| {
                d.and_then(|d| d.child("numFmt"))
                    .filter(|n| n.attr("sourceLinked") != Some("1"))
                    .and_then(|n| n.attr("formatCode"))
                    .map(str::to_string)
            };
            let source =
                s.child("val").or_else(|| s.child("yVal")).and_then(|v| v.find("numCache")).and_then(|c| c.child("formatCode")).map(|f| f.text());
            ser.number_format = label_fmt(s.child("dLbls")).or_else(|| label_fmt(g.child("dLbls"))).or(source).filter(|f| !f.is_empty());
            for dp in s.children_named("dPt") {
                if let (Some(i), Some(f)) = (dp.child("idx").and_then(|i| i.u32("val")), simple_fill(dp.child("spPr"))) {
                    ser.point_fills.push((i as usize, f));
                }
            }
            if kind.points_only() || kind.is_stock() {
                // The type says there are no lines (stock prices: the group's high-low lines and
                // up-down bars stand for them): keep only what else the line holds.
                if let Some(l) = &mut ser.line
                    && l.fill == Some(Fill::None)
                {
                    l.fill = None;
                }
                if ser.line.as_ref().is_some_and(|l| *l == deckcraft_model::Line::default()) {
                    ser.line = None;
                }
            }
            format::read_series_format(s, &mut ser);
            if let Some(cat) = s.child("cat").or_else(|| s.child("xVal")) {
                let cats = strings(cat);
                if kind.is_xy() {
                    ser.x_values = cats.iter().map(|v| v.trim().parse::<f64>().ok()).collect();
                }
                if c.categories.is_empty() {
                    c.categories = cats;
                }
            }
            if let Some(v) = s.child("val").or_else(|| s.child("yVal")) {
                ser.values = numbers(v);
            }
            if let Some(b) = s.child("bubbleSize") {
                ser.sizes = numbers(b);
            }
            sources.push(SeriesSource {
                name: ser.name.clone(),
                values: ser.values.clone(),
                x_values: ser.x_values.clone(),
                sizes: ser.sizes.clone(),
                name_ref: formula(s.child("tx")),
                categories_ref: formula(s.child("cat").or_else(|| s.child("xVal"))),
                values_ref: formula(s.child("val").or_else(|| s.child("yVal"))),
                sizes_ref: formula(s.child("bubbleSize")),
            });
            c.series.push(ser);
        }
        groups.push((name, first_series..c.series.len()));
    }
    let first = *kinds.first()?;
    c.kind = if kinds.iter().any(|k| *k != first) { ChartType::Combo } else { first };
    if let Some(stock) = groups.iter().find(|g| g.0 == "stockChart") {
        // Stock charts: the prices of the stock group, after the volume of a column group.
        let prices = stock.1.len();
        let volume = groups.iter().find(|g| matches!(g.0, "barChart" | "bar3DChart")).map(|g| g.1.clone());
        c.kind = match (volume.is_some(), prices >= 4) {
            (false, false) => ChartType::Stock,
            (false, true) => ChartType::StockOhlc,
            (true, false) => ChartType::StockVhlc,
            (true, true) => ChartType::StockVohlc,
        };
        if let Some(v) = volume.filter(|v| !v.is_empty() && v.start > 0) {
            // Volume first, as PowerPoint lays the data out.
            let moved: Vec<Series> = c.series.drain(v.clone()).collect();
            let moved_src: Vec<SeriesSource> = sources.drain(v).collect();
            c.series.splice(0..0, moved);
            sources.splice(0..0, moved_src);
        }
    }
    if c.kind != ChartType::Combo {
        for s in &mut c.series {
            s.kind = None;
            s.secondary_axis = false;
        }
    }
    c.variant.stepped = c.kind.has_lines() && stepped(root);
    if c.kind.exploded() {
        // The type's explosion stands for the series that have it.
        for s in &mut c.series {
            if s.format.explosion == Some(c.variant.explosion) {
                s.format.explosion = None;
            }
        }
    }
    if sources.iter().any(|s| s.name_ref.is_some() || s.categories_ref.is_some() || s.values_ref.is_some()) {
        let xy = c.kind.is_xy();
        c.source = Some(ChartSource { xy, categories: c.categories.clone(), series: sources });
    }
    // Axes.
    c.lang = root.child_val("lang").map(str::to_string);
    let vertical = |a: &&El| matches!(a.child_val("axPos"), Some("l" | "r"));
    let by_id = |id: &Option<String>| pa.children_named("valAx").find(|a| id.is_some() && a.child_val("axId") == id.as_deref());
    let val_ax = by_id(&primary_val).or_else(|| pa.children_named("valAx").find(vertical)).or_else(|| pa.children_named("valAx").last());
    let num = |a: &El, k: &str| a.child_val(k).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
    let format_code = |a: &El| a.child("numFmt").and_then(|n| n.attr("formatCode")).map(str::to_string);
    let deleted = |a: &El| a.child("delete").and_then(|d| d.bool("val")) == Some(true);
    if c.kind == ChartType::Combo && c.series.iter().any(|s| s.secondary_axis) {
        let primary_id = val_ax.and_then(|a| a.child_val("axId"));
        // The other vertical value axis is the secondary one.
        if let Some(a) = pa.children_named("valAx").find(|a| a.child_val("axId") != primary_id && vertical(a)) {
            let sc = a.child("scaling");
            c.y2_min = sc.and_then(|s| num(s, "min"));
            c.y2_max = sc.and_then(|s| num(s, "max"));
            c.y2_major = num(a, "majorUnit");
            c.y2_format = format_code(a);
            c.y2_hidden = deleted(a);
        }
    }
    let cat_ax = pa.child("catAx").or_else(|| pa.child("dateAx")).or_else(|| pa.children_named("valAx").find(|a| !vertical(a)));
    if let Some(v) = val_ax {
        c.gridlines = v.child("majorGridlines").is_some();
        let sc = v.child("scaling");
        c.y_min = sc.and_then(|s| s.child_val("min")).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite());
        c.y_max = sc.and_then(|s| s.child_val("max")).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite());
        c.axis_titles.1 = v.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into()));
        c.y_major = num(v, "majorUnit");
        c.y_format = format_code(v);
        c.y_hidden = deleted(v);
    }
    if let Some(a) = cat_ax {
        c.axis_titles.0 = a.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into()));
    }
    // Text sizes as the file sets them (PowerPoint draws chart text at these sizes, not scaled to
    // the chart): the axes' and legend's, else the whole chart's; the title's own.
    let chart_size = text_size(root.child("txPr"));
    c.text_size = [val_ax, cat_ax, ch.child("legend")].into_iter().flatten().find_map(|e| text_size(e.child("txPr"))).or(chart_size);
    c.title_size = ch.child("title").and_then(|t| text_size(t.child("tx").and_then(|x| x.child("rich"))).or_else(|| text_size(t.child("txPr"))));
    format::read_format(root, ch, pa, &mut c, val_ax, cat_ax);
    Some(c)
}

/// The size in points of the first `a:defRPr` or `a:rPr` with one under `e` (`sz` in hundredths).
fn text_size(e: Option<&El>) -> Option<f64> {
    fn rec(e: &El, depth: usize) -> Option<f64> {
        if depth > 8 {
            return None;
        }
        for c in e.elements() {
            if (c.is("defRPr") || c.is("rPr"))
                && let Some(v) = c.attr("sz").and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0)
            {
                return Some((v / 100.0).clamp(1.0, 400.0));
            }
            if let Some(v) = rec(c, depth + 1) {
                return Some(v);
            }
        }
        None
    }
    rec(e?, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legend_position_and_overlay_are_read() {
        let legend = r#"<c:legend><c:legendPos val="r"/><c:overlay val="1"/></c:legend>"#;
        let xml = format!(
            r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/></c:barChart></c:plotArea>{legend}</c:chart></c:chartSpace>"#
        );
        let c = read_chart_xml(xml.as_bytes()).unwrap();
        assert_eq!(c.legend.as_deref(), Some("r"));
        assert!(c.legend_overlay);
        let xml = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/></c:barChart></c:plotArea><c:legend/></c:chart></c:chartSpace>"#;
        let c = read_chart_xml(xml).unwrap();
        assert_eq!(c.legend.as_deref(), Some("r"));
        assert!(!c.legend_overlay);
    }

    #[test]
    fn combo_chart_with_a_secondary_axis() {
        // Columns on axes 1/2, a line on axes 3/4 whose value axis is on the right.
        let xml = br##"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:lang val="ru-RU"/><c:chart><c:plotArea>
<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:tx><c:v>Spend</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>W1</c:v></c:pt><c:pt idx="1"><c:v>W2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>200000</c:v></c:pt><c:pt idx="1"><c:v>310000</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart>
<c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="1"/><c:tx><c:v>Clicks</c:v></c:tx><c:spPr><a:ln w="28575"><a:solidFill><a:srgbClr val="00B050"/></a:solidFill></a:ln></c:spPr><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>6000</c:v></c:pt><c:pt idx="1"><c:v>7400</c:v></c:pt></c:numLit></c:val></c:ser><c:marker val="1"/><c:axId val="3"/><c:axId val="4"/></c:lineChart>
<c:catAx><c:axId val="1"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:axPos val="l"/><c:numFmt formatCode="#,##0" sourceLinked="1"/><c:crossAx val="1"/><c:majorUnit val="100000"/></c:valAx>
<c:valAx><c:axId val="4"/><c:scaling><c:max val="8000"/></c:scaling><c:axPos val="r"/><c:numFmt formatCode="0.0" sourceLinked="0"/><c:crossAx val="3"/></c:valAx><c:catAx><c:axId val="3"/><c:delete val="1"/><c:axPos val="b"/><c:crossAx val="4"/></c:catAx>
</c:plotArea></c:chart></c:chartSpace>"##;
        let c = read_chart_xml(xml).unwrap();
        assert_eq!(c.kind, ChartType::Combo);
        assert_eq!(c.series.len(), 2);
        assert!(!c.series[0].secondary_axis);
        assert!(c.series[1].secondary_axis);
        assert_eq!(c.series[1].kind, Some(ChartType::LineMarkers));
        assert!(c.series[1].line.as_ref().is_some_and(|l| l.width.is_some_and(|w| (w - 2.25).abs() < 0.01)));
        // Each value axis' number format, bounds and unit; the chart's language.
        assert_eq!(c.y_format.as_deref(), Some("#,##0"));
        assert_eq!((c.y_major, c.y2_max, c.y2_format.as_deref()), (Some(100000.0), Some(8000.0), Some("0.0")));
        assert_eq!(c.lang.as_deref(), Some("ru-RU"));
        // Group order doesn't decide: columns on the right-hand axis are the secondary series even
        // when their group comes first, and the line on the left is primary.
        let swapped = String::from_utf8_lossy(xml)
            .replace(
                "<c:axPos val=\"l\"/><c:numFmt formatCode=\"#,##0\"",
                "<c:axPos val=\"r\"/><c:crosses val=\"max\"/><c:numFmt formatCode=\"#,##0\"",
            )
            .replace("<c:axPos val=\"r\"/><c:numFmt formatCode=\"0.0\"", "<c:axPos val=\"l\"/><c:numFmt formatCode=\"0.0\"");
        let c2 = read_chart_xml(swapped.as_bytes()).unwrap();
        assert!(c2.series[0].secondary_axis && !c2.series[1].secondary_axis);
        assert_eq!((c2.y_format.as_deref(), c2.y2_format.as_deref()), (Some("0.0"), Some("#,##0")));
        // A chart of one type on one axis has no secondary series.
        let single = String::from_utf8_lossy(xml)
            .replace("<c:axId val=\"3\"/><c:axId val=\"4\"/></c:lineChart>", "<c:axId val=\"1\"/><c:axId val=\"2\"/></c:lineChart>");
        let c = read_chart_xml(single.as_bytes()).unwrap();
        assert!(c.series.iter().all(|s| !s.secondary_axis));
    }

    #[test]
    fn text_sizes_are_read_from_the_file() {
        // PowerPoint's new charts set the axes' and legend's text to 11.97 pt and the title's to
        // 18.62 pt; a chart that only sets its overall text size uses it everywhere.
        let ns =
            r#"xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;
        let tx = |sz: u32| format!(r#"<c:txPr><a:bodyPr/><a:p><a:pPr><a:defRPr sz="{sz}"/></a:pPr></a:p></c:txPr>"#);
        let xml = format!(
            r#"<c:chartSpace {ns}><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:pPr><a:defRPr sz="1862"/></a:pPr><a:r><a:t>T</a:t></a:r></a:p></c:rich></c:tx></c:title><c:plotArea><c:barChart><c:barDir val="col"/><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/>{}</c:catAx><c:valAx><c:axId val="2"/>{}</c:valAx></c:plotArea></c:chart></c:chartSpace>"#,
            tx(1197),
            tx(1197)
        );
        let c = read_chart_xml(xml.as_bytes()).unwrap();
        assert_eq!((c.text_size, c.title_size), (Some(11.97), Some(18.62)));
        let xml = format!(
            r#"<c:chartSpace {ns}><c:chart><c:plotArea><c:barChart><c:barDir val="col"/></c:barChart></c:plotArea></c:chart>{}</c:chartSpace>"#,
            tx(1400)
        );
        let c = read_chart_xml(xml.as_bytes()).unwrap();
        assert_eq!((c.text_size, c.title_size), (Some(14.0), None));
        let xml = format!(
            r#"<c:chartSpace {ns}><c:chart><c:plotArea><c:barChart><c:barDir val="col"/></c:barChart></c:plotArea></c:chart></c:chartSpace>"#
        );
        assert_eq!(read_chart_xml(xml.as_bytes()).unwrap().text_size, None, "no sizes: the renderer picks one");
    }
}
