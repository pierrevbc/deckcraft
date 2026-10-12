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

/// The id of the value axis plot group `g` uses: the vertical one for a scatter chart.
fn value_axis(pa: &El, g: &El) -> Option<u32> {
    let ids: Vec<u32> = g.children_named("axId").filter_map(|a| a.u32("val")).collect();
    let axes: Vec<&El> =
        pa.children_named("valAx").filter(|a| a.child("axId").and_then(|i| i.u32("val")).is_some_and(|i| ids.contains(&i))).collect();
    let pick = if axes.len() > 1 { axes.iter().find(|a| matches!(a.child_val("axPos"), Some("l" | "r"))).or(axes.first()) } else { axes.first() };
    pick.and_then(|a| a.child("axId")).and_then(|i| i.u32("val"))
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
    c.chart_fill = simple_fill(root.child("spPr"));
    c.plot_fill = simple_fill(pa.child("spPr"));

    let mut kinds: Vec<ChartType> = vec![];
    let mut sources: Vec<SeriesSource> = vec![];
    // Per plot group: its element name, its series (a range of `c.series`) and its value axis.
    let mut groups: Vec<(&str, std::ops::Range<usize>, Option<u32>)> = vec![];
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
        for s in g.children_named("ser").take(1000) {
            let mut ser = Series {
                name: s.child("tx").map(strings).and_then(|v| v.into_iter().next()).unwrap_or_default(),
                fill: simple_fill(s.child("spPr")),
                line: simple_line(s.child("spPr")),
                smooth: s.child("smooth").and_then(|v| v.bool("val")).unwrap_or(false),
                marker: s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")).filter(|m| *m != "none").map(String::from),
                kind: Some(kind),
                ..Default::default()
            };
            if let Some(d) = s.child("dLbls").and_then(|d| d.child("showVal")).and_then(|v| v.bool("val")) {
                ser.data_labels = Some(d);
            }
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
        groups.push((name, first_series..c.series.len(), value_axis(pa, g)));
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
    } else {
        // Series whose group plots on another value axis than the first group: secondary axis.
        let primary = groups.iter().find_map(|g| g.2);
        for (_, range, axis) in &groups {
            if axis.is_some() && *axis != primary {
                for s in c.series.get_mut(range.clone()).into_iter().flatten() {
                    s.secondary_axis = true;
                }
            }
        }
    }
    if c.kind != ChartType::Combo {
        for s in &mut c.series {
            s.kind = None;
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
    let vertical = |a: &&El| matches!(a.child_val("axPos"), Some("l" | "r"));
    let val_ax = pa.children_named("valAx").find(vertical).or_else(|| pa.children_named("valAx").last());
    let cat_ax = pa.child("catAx").or_else(|| pa.child("dateAx")).or_else(|| pa.children_named("valAx").find(|a| !vertical(a)));
    if let Some(v) = val_ax {
        c.gridlines = v.child("majorGridlines").is_some();
        let sc = v.child("scaling");
        c.y_min = sc.and_then(|s| s.child_val("min")).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite());
        c.y_max = sc.and_then(|s| s.child_val("max")).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite());
        c.axis_titles.1 = v.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into()));
    }
    if let Some(a) = cat_ax {
        c.axis_titles.0 = a.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into()));
    }
    format::read_format(root, ch, pa, &mut c, val_ax, cat_ax);
    Some(c)
}
