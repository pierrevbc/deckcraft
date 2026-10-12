//! Chart parts: a `c:chartSpace` with cached categories and values, and the workbook that holds
//! the chart data (what PowerPoint opens for Edit Data). An unchanged chart keeps its original
//! part and workbook; otherwise both are written from the model, the workbook laid out where the
//! part's formulas point.

use std::collections::BTreeMap;

use deckcraft_model::chart::{Chart, ChartType, Series};
use deckcraft_model::style::Fill;

use crate::opc::{NS_A, NS_C, NS_R, PackageOut, RelsOut};
use crate::xml::{A, El, W};

/// Content type of an embedded `.xlsx` workbook.
pub const CT_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const NS_SSML: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT_WORKBOOK: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
const CT_WORKSHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const CT_STYLES: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
/// Rows a worksheet can hold, header row excluded.
const MAX_ROWS: usize = 1_048_575;

/// Column letter for a 0-based index (A, B, … Z, AA…).
fn col(i: usize) -> String {
    let mut n = i + 1;
    let mut s = String::new();
    while n > 0 && s.len() < 4 {
        let r = (n - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
}

/// Columns (0-based) of one series in the data sheet. Data starts on row 2; the series name sits
/// on row 1 above its values.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Cols {
    /// Categories, or X values for scatter and bubble charts.
    x: usize,
    /// Values.
    y: usize,
    /// Bubble sizes.
    size: Option<usize>,
}

/// X values of a scatter or bubble series (1, 2, 3… when it has none).
fn xs(s: &Series) -> Vec<Option<f64>> {
    if s.x_values.is_empty() { (1..=s.values.len()).map(|v| Some(v as f64)).collect() } else { s.x_values.clone() }
}

/// Bubble sizes (all 1 when the series has none).
fn sizes(s: &Series) -> Vec<Option<f64>> {
    if s.sizes.is_empty() { s.values.iter().map(|_| Some(1.0)).collect() } else { s.sizes.clone() }
}

/// Where each series sits in the data sheet: categories (or shared X values) in column A and one
/// column per series, the layout of a new chart's sheet in PowerPoint. Scatter series with
/// different X values, and bubble series, get columns of their own so no two series share cells.
fn layout(c: &Chart) -> Vec<Cols> {
    let n = c.series.len();
    let shared_x = c.series.windows(2).all(|w| match w {
        [a, b] => xs(a) == xs(b),
        _ => true,
    });
    match c.kind {
        ChartType::Scatter if shared_x => (0..n).map(|i| Cols { x: 0, y: i + 1, size: None }).collect(),
        ChartType::Scatter => (0..n).map(|i| Cols { x: 2 * i, y: 2 * i + 1, size: None }).collect(),
        ChartType::Bubble if shared_x => (0..n).map(|i| Cols { x: 0, y: 2 * i + 1, size: Some(2 * i + 2) }).collect(),
        ChartType::Bubble => (0..n).map(|i| Cols { x: 3 * i, y: 3 * i + 1, size: Some(3 * i + 2) }).collect(),
        _ => (0..n).map(|i| Cols { x: 0, y: i + 1, size: None }).collect(),
    }
}

/// `Sheet1!$B$2:$B$5`: rows 2 to `last` of column `c`.
fn range(c: usize, last: usize) -> String {
    format!("Sheet1!${0}$2:${0}${last}", col(c))
}

fn simple_fill(w: &mut W, f: &Fill) {
    match f {
        Fill::None => w.empty0("a:noFill"),
        Fill::Solid { color } => super::dml::solid(w, color),
        Fill::Gradient(_) | Fill::Picture(_) | Fill::Pattern(_) | Fill::Group | Fill::Background => {
            if let Fill::Gradient(g) = f
                && let Some(s) = g.stops.first()
            {
                super::dml::solid(w, &s.color);
            }
        }
    }
}

fn sp_pr(w: &mut W, fill: Option<&Fill>, line: Option<&deckcraft_model::Line>) {
    if fill.is_none() && line.is_none() {
        return;
    }
    w.open0("c:spPr");
    if let Some(f) = fill {
        simple_fill(w, f);
    }
    if let Some(l) = line {
        w.open("a:ln", A::new().o("w", l.width.map(super::dml::emu_pos)));
        if let Some(f) = &l.fill {
            simple_fill(w, f);
        }
        w.close("a:ln");
    }
    w.close("c:spPr");
}

fn rich(w: &mut W, text: &str) {
    w.open0("c:tx");
    w.open0("c:rich");
    w.empty0("a:bodyPr");
    w.empty0("a:lstStyle");
    for line in text.split('\n') {
        w.open0("a:p");
        w.open0("a:r");
        w.elt("a:t", line);
        w.close("a:r");
        w.close("a:p");
    }
    w.close("c:rich");
    w.close("c:tx");
}

fn title(w: &mut W, text: &str) {
    w.open0("c:title");
    rich(w, text);
    w.val("c:overlay", 0);
    w.close("c:title");
}

fn str_cache(w: &mut W, f: &str, vals: &[String]) {
    w.open0("c:strRef");
    w.elt("c:f", f);
    w.open0("c:strCache");
    w.val("c:ptCount", vals.len());
    for (i, v) in vals.iter().enumerate() {
        w.open("c:pt", A::new().a("idx", i));
        w.elt("c:v", v);
        w.close("c:pt");
    }
    w.close("c:strCache");
    w.close("c:strRef");
}

fn num_cache(w: &mut W, f: &str, vals: &[Option<f64>]) {
    w.open0("c:numRef");
    w.elt("c:f", f);
    w.open0("c:numCache");
    w.elt("c:formatCode", "General");
    w.val("c:ptCount", vals.len());
    for (i, v) in vals.iter().enumerate() {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            w.open("c:pt", A::new().a("idx", i));
            w.elt("c:v", &fmt_num(v));
            w.close("c:pt");
        }
    }
    w.close("c:numCache");
    w.close("c:numRef");
}

#[derive(Clone, Copy, PartialEq)]
enum Group {
    Bar,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
    Radar,
    Bubble,
}

fn group_of(k: ChartType) -> Group {
    match k {
        ChartType::Line | ChartType::LineMarkers | ChartType::StackedLine | ChartType::Stock => Group::Line,
        ChartType::Area | ChartType::StackedArea | ChartType::Surface => Group::Area,
        ChartType::Pie => Group::Pie,
        ChartType::Doughnut | ChartType::Sunburst => Group::Doughnut,
        ChartType::Scatter => Group::Scatter,
        ChartType::Bubble => Group::Bubble,
        ChartType::Radar | ChartType::FilledRadar => Group::Radar,
        _ => Group::Bar,
    }
}

fn series(w: &mut W, c: &Chart, s: &Series, i: usize, k: Cols, kind: ChartType, g: Group) {
    w.open0("c:ser");
    w.val("c:idx", i);
    w.val("c:order", i);
    w.open0("c:tx");
    str_cache(w, &format!("Sheet1!${}$1", col(k.y)), std::slice::from_ref(&s.name));
    w.close("c:tx");
    sp_pr(w, s.fill.as_ref(), s.line.as_ref());
    if g == Group::Bar {
        w.val("c:invertIfNegative", 0);
    }
    if matches!(g, Group::Line | Group::Scatter | Group::Radar) {
        let sym = match (&s.marker, kind) {
            (Some(m), _) if !m.is_empty() => Some(m.as_str()),
            (_, ChartType::LineMarkers) => None,
            (_, ChartType::Scatter) => None,
            _ => Some("none"),
        };
        if let Some(sym) = sym.filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric())) {
            w.open0("c:marker");
            w.val("c:symbol", sym);
            w.close("c:marker");
        }
    }
    if g == Group::Pie || g == Group::Doughnut || g == Group::Bar {
        for (idx, f) in s.point_fills.iter().take(1000) {
            w.open0("c:dPt");
            w.val("c:idx", idx);
            if g == Group::Bar {
                w.val("c:invertIfNegative", 0);
            }
            if g != Group::Bar {
                w.val("c:bubble3D", 0);
            }
            sp_pr(w, Some(f), None);
            w.close("c:dPt");
        }
    }
    if let Some(d) = s.data_labels {
        data_labels(w, d);
    }
    let n = c.categories.len().max(s.values.len());
    let last = n + 1;
    if matches!(g, Group::Scatter | Group::Bubble) {
        w.open0("c:xVal");
        num_cache(w, &range(k.x, last), &xs(s));
        w.close("c:xVal");
        w.open0("c:yVal");
        num_cache(w, &range(k.y, last), &s.values);
        w.close("c:yVal");
        if g == Group::Bubble {
            w.open0("c:bubbleSize");
            num_cache(w, &range(k.size.unwrap_or(k.y + 1), last), &sizes(s));
            w.close("c:bubbleSize");
            w.val("c:bubble3D", 0);
        } else {
            w.val("c:smooth", if s.smooth { 1 } else { 0 });
        }
    } else {
        w.open0("c:cat");
        str_cache(w, &range(k.x, last), &c.categories);
        w.close("c:cat");
        w.open0("c:val");
        num_cache(w, &range(k.y, last), &s.values);
        w.close("c:val");
        if g == Group::Line {
            w.val("c:smooth", if s.smooth { 1 } else { 0 });
        }
    }
    w.close("c:ser");
}

fn data_labels(w: &mut W, show: bool) {
    w.open0("c:dLbls");
    for (n, v) in [
        ("c:showLegendKey", false),
        ("c:showVal", show),
        ("c:showCatName", false),
        ("c:showSerName", false),
        ("c:showPercent", false),
        ("c:showBubbleSize", false),
    ] {
        w.val(n, if v { 1 } else { 0 });
    }
    w.close("c:dLbls");
}

const AX_CAT: u32 = 50_001;
const AX_VAL: u32 = 50_002;

fn axes(w: &mut W, c: &Chart, g: Group) {
    let horizontal = c.kind.horizontal() && g == Group::Bar;
    let (cat_pos, val_pos) = if horizontal { ("l", "b") } else { ("b", "l") };
    if g == Group::Scatter || g == Group::Bubble {
        w.open0("c:valAx");
        w.val("c:axId", AX_CAT);
        w.open0("c:scaling");
        w.val("c:orientation", "minMax");
        w.close("c:scaling");
        w.val("c:delete", 0);
        w.val("c:axPos", "b");
        if let Some(t) = &c.axis_titles.0 {
            title(w, t);
        }
        w.empty("c:numFmt", A::new().a("formatCode", "General").a("sourceLinked", 1));
        w.val("c:majorTickMark", "out");
        w.val("c:minorTickMark", "none");
        w.val("c:tickLblPos", "nextTo");
        w.val("c:crossAx", AX_VAL);
        w.val("c:crosses", "autoZero");
        w.val("c:crossBetween", "midCat");
        w.close("c:valAx");
    } else {
        w.open0("c:catAx");
        w.val("c:axId", AX_CAT);
        w.open0("c:scaling");
        w.val("c:orientation", "minMax");
        w.close("c:scaling");
        w.val("c:delete", 0);
        w.val("c:axPos", cat_pos);
        if let Some(t) = &c.axis_titles.0 {
            title(w, t);
        }
        w.empty("c:numFmt", A::new().a("formatCode", "General").a("sourceLinked", 1));
        w.val("c:majorTickMark", "none");
        w.val("c:minorTickMark", "none");
        w.val("c:tickLblPos", "nextTo");
        w.val("c:crossAx", AX_VAL);
        w.val("c:crosses", "autoZero");
        w.val("c:auto", 1);
        w.val("c:lblAlgn", "ctr");
        w.val("c:lblOffset", 100);
        w.val("c:noMultiLvlLbl", 0);
        w.close("c:catAx");
    }
    w.open0("c:valAx");
    w.val("c:axId", AX_VAL);
    w.open0("c:scaling");
    w.val("c:orientation", "minMax");
    if let Some(m) = c.y_max.filter(|v| v.is_finite()) {
        w.val("c:max", fmt_num(m));
    }
    if let Some(m) = c.y_min.filter(|v| v.is_finite()) {
        w.val("c:min", fmt_num(m));
    }
    w.close("c:scaling");
    w.val("c:delete", 0);
    w.val("c:axPos", val_pos);
    if c.gridlines {
        w.empty0("c:majorGridlines");
    }
    if let Some(t) = &c.axis_titles.1 {
        title(w, t);
    }
    w.empty("c:numFmt", A::new().a("formatCode", if c.kind.percent() { "0%" } else { "General" }).a("sourceLinked", 1));
    w.val("c:majorTickMark", "none");
    w.val("c:minorTickMark", "none");
    w.val("c:tickLblPos", "nextTo");
    w.val("c:crossAx", AX_CAT);
    w.val("c:crosses", "autoZero");
    w.val("c:crossBetween", if g == Group::Area || g == Group::Scatter { "midCat" } else { "between" });
    w.close("c:valAx");
}

fn plot_group(w: &mut W, c: &Chart, kind: ChartType, list: &[(usize, &Series)], cols: &[Cols]) {
    let g = group_of(kind);
    let grouping = if kind.percent() {
        "percentStacked"
    } else if kind.stacked() {
        "stacked"
    } else if matches!(g, Group::Bar) {
        "clustered"
    } else {
        "standard"
    };
    let vary = if c.vary_colors { 1 } else { 0 };
    let tag = match g {
        Group::Bar => "c:barChart",
        Group::Line => "c:lineChart",
        Group::Area => "c:areaChart",
        Group::Pie => "c:pieChart",
        Group::Doughnut => "c:doughnutChart",
        Group::Scatter => "c:scatterChart",
        Group::Radar => "c:radarChart",
        Group::Bubble => "c:bubbleChart",
    };
    w.open0(tag);
    match g {
        Group::Bar => {
            w.val("c:barDir", if kind.horizontal() { "bar" } else { "col" });
            w.val("c:grouping", grouping);
        }
        Group::Line | Group::Area => w.val("c:grouping", grouping),
        Group::Scatter => w.val("c:scatterStyle", "lineMarker"),
        Group::Radar => w.val("c:radarStyle", if kind == ChartType::FilledRadar { "filled" } else { "marker" }),
        _ => {}
    }
    w.val("c:varyColors", vary);
    for (i, s) in list {
        series(w, c, s, *i, cols.get(*i).copied().unwrap_or_default(), kind, g);
    }
    if c.data_labels {
        data_labels(w, true);
    }
    match g {
        Group::Bar => {
            w.val("c:gapWidth", (c.gap_width.clamp(0.0, 5.0) * 100.0).round() as i64);
            let ov = if kind.stacked() { 100 } else { (c.overlap.clamp(-1.0, 1.0) * 100.0).round() as i64 };
            if ov != 0 {
                w.val("c:overlap", ov);
            }
        }
        Group::Line => w.val("c:marker", 1),
        Group::Pie => w.val("c:firstSliceAng", 0),
        Group::Doughnut => {
            w.val("c:firstSliceAng", 0);
            w.val("c:holeSize", (c.hole_size.clamp(0.1, 0.9) * 100.0).round() as i64);
        }
        _ => {}
    }
    if !matches!(g, Group::Pie | Group::Doughnut) {
        w.val("c:axId", AX_CAT);
        w.val("c:axId", AX_VAL);
    }
    w.close(tag);
}

/// The chart's original part when the chart is unchanged since it was read: its formulas,
/// unmodelled options and workbook all still apply.
pub fn unchanged(c: &Chart) -> Option<El> {
    let raw = c.raw.as_ref()?;
    let doc = crate::xml::parse(raw.as_bytes()).ok()?;
    if !doc.root.is("chartSpace") {
        return None;
    }
    let mut root = doc.root.clone();
    crate::read::resolve_mc(&mut root, 0);
    let back = crate::read::chart_from_el(&root)?;
    let mut me = c.clone();
    me.raw = None;
    me.workbook = None;
    (back == me).then_some(doc.root)
}

/// Point `c:externalData` at relationship `rid`, whatever prefix its `r:id` uses.
fn set_rid(e: &mut El, rid: &str) {
    let key = e
        .attrs
        .iter()
        .find(|(k, _)| !k.starts_with("xmlns") && k.rsplit_once(':').is_some_and(|(_, l)| l == "id"))
        .map(|(k, _)| k.clone())
        .unwrap_or_else(|| "r:id".into());
    e.set_attr(&key, rid);
}

/// The chart part XML. `kept` is the original part from [`unchanged`]; `workbook` is the id of
/// the relationship to the data workbook, when one is written.
pub fn chart_xml(c: &Chart, kept: Option<El>, workbook: Option<&str>) -> Vec<u8> {
    if let Some(mut keep) = kept {
        keep.remove_all(&["userShapes"]);
        match workbook {
            Some(rid) => {
                for e in keep.elements_mut().filter(|e| e.is("externalData")) {
                    set_rid(e, rid);
                }
            }
            None => keep.remove_all(&["externalData"]),
        }
        let mut w = W::new();
        w.raw(&keep.to_xml());
        return w.finish();
    }
    let cols = layout(c);
    let mut w = W::new();
    w.open("c:chartSpace", A::new().a("xmlns:c", NS_C).a("xmlns:a", NS_A).a("xmlns:r", NS_R));
    w.val("c:date1904", 0);
    w.val("c:roundedCorners", 0);
    w.val("c:style", c.style.clamp(1, 48));
    w.open0("c:chart");
    match &c.title {
        Some(t) => {
            title(&mut w, t);
            w.val("c:autoTitleDeleted", 0);
        }
        None => w.val("c:autoTitleDeleted", 1),
    }
    w.open0("c:plotArea");
    w.empty0("c:layout");
    let indexed: Vec<(usize, &Series)> = c.series.iter().enumerate().collect();
    let main = match c.kind {
        ChartType::Combo => c.series.first().and_then(|s| s.kind).unwrap_or(ChartType::Column),
        k => k,
    };
    if c.kind == ChartType::Combo {
        // One plot group per series type, sharing the axes.
        let mut kinds: Vec<ChartType> = vec![];
        for s in &c.series {
            let k = s.kind.unwrap_or(main);
            let k = if matches!(group_of(k), Group::Pie | Group::Doughnut | Group::Scatter | Group::Bubble) { ChartType::Column } else { k };
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        for k in kinds {
            let list: Vec<(usize, &Series)> = indexed
                .iter()
                .filter(|(_, s)| {
                    let sk = s.kind.unwrap_or(main);
                    let sk =
                        if matches!(group_of(sk), Group::Pie | Group::Doughnut | Group::Scatter | Group::Bubble) { ChartType::Column } else { sk };
                    sk == k
                })
                .copied()
                .collect();
            plot_group(&mut w, c, k, &list, &cols);
        }
        axes(&mut w, c, Group::Bar);
    } else {
        if !matches!(
            c.kind,
            ChartType::Column
                | ChartType::StackedColumn
                | ChartType::PercentColumn
                | ChartType::Bar
                | ChartType::StackedBar
                | ChartType::PercentBar
                | ChartType::Line
                | ChartType::LineMarkers
                | ChartType::StackedLine
                | ChartType::Area
                | ChartType::StackedArea
                | ChartType::Pie
                | ChartType::Doughnut
                | ChartType::Scatter
                | ChartType::Bubble
                | ChartType::Radar
                | ChartType::FilledRadar
        ) {
            log::warn!("pptx: {} chart written as a column chart", c.kind.label());
        }
        plot_group(&mut w, c, main, &indexed, &cols);
        let g = group_of(main);
        if !matches!(g, Group::Pie | Group::Doughnut) {
            axes(&mut w, c, g);
        }
    }
    if let Some(f) = &c.plot_fill {
        sp_pr(&mut w, Some(f), None);
    }
    w.close("c:plotArea");
    if let Some(pos) = &c.legend {
        let p = match pos.as_str() {
            "b" | "t" | "l" | "r" | "tr" => pos.as_str(),
            _ => "r",
        };
        w.open0("c:legend");
        w.val("c:legendPos", p);
        w.val("c:overlay", 0);
        w.close("c:legend");
    }
    w.val("c:plotVisOnly", 1);
    w.val("c:dispBlanksAs", "gap");
    w.close("c:chart");
    if let Some(f) = &c.chart_fill {
        sp_pr(&mut w, Some(f), None);
    }
    if let Some(rid) = workbook {
        w.open("c:externalData", A::new().a("r:id", rid));
        w.val("c:autoUpdate", 0);
        w.close("c:externalData");
    }
    w.close("c:chartSpace");
    w.finish()
}

enum Cell {
    Text(String),
    Num(f64),
}

fn put_numbers(cells: &mut BTreeMap<(usize, usize), Cell>, column: usize, values: &[Option<f64>]) {
    for (r, v) in values.iter().enumerate().take(MAX_ROWS) {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            cells.insert((r + 2, column), Cell::Num(v));
        }
    }
}

const STYLES_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    "<fonts count=\"1\"><font><sz val=\"11\"/></font></fonts>",
    "<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>",
    "<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>",
    "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
    "<cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs>",
    "<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>",
    "</styleSheet>"
);

/// A workbook holding the chart data on `Sheet1`, where the formulas of [`chart_xml`] point (see
/// [`layout`]), so PowerPoint's Edit Data opens the chart's own numbers. `None` for a chart
/// without series.
pub fn workbook_xlsx(c: &Chart) -> Option<Vec<u8>> {
    if c.series.is_empty() {
        return None;
    }
    let xy = matches!(c.kind, ChartType::Scatter | ChartType::Bubble);
    let mut cells: BTreeMap<(usize, usize), Cell> = BTreeMap::new();
    for (s, k) in c.series.iter().zip(layout(c)) {
        if !s.name.is_empty() {
            cells.insert((1, k.y), Cell::Text(s.name.clone()));
        }
        put_numbers(&mut cells, k.y, &s.values);
        if xy {
            put_numbers(&mut cells, k.x, &xs(s));
        }
        if let Some(column) = k.size {
            put_numbers(&mut cells, column, &sizes(s));
        }
    }
    if !xy {
        for (r, cat) in c.categories.iter().enumerate().take(MAX_ROWS) {
            if !cat.is_empty() {
                cells.insert((r + 2, 0), Cell::Text(cat.clone()));
            }
        }
    }

    // Rows in order, cells in column order within a row, as SpreadsheetML requires.
    let mut sheet = W::new();
    sheet.open("worksheet", A::new().a("xmlns", NS_SSML));
    sheet.open0("sheetData");
    let mut row = 0;
    for ((r, column), cell) in &cells {
        if *r != row {
            if row != 0 {
                sheet.close("row");
            }
            sheet.open("row", A::new().a("r", r));
            row = *r;
        }
        let at = format!("{}{r}", col(*column));
        match cell {
            Cell::Text(t) => {
                sheet.open("c", A::new().a("r", at).a("t", "inlineStr"));
                sheet.open0("is");
                sheet.open("t", A::new().a("xml:space", "preserve"));
                sheet.text(t);
                sheet.close("t");
                sheet.close("is");
                sheet.close("c");
            }
            Cell::Num(v) => {
                sheet.open("c", A::new().a("r", at));
                sheet.elt("v", &fmt_num(*v));
                sheet.close("c");
            }
        }
    }
    if row != 0 {
        sheet.close("row");
    }
    sheet.close("sheetData");
    sheet.close("worksheet");

    let mut rels = RelsOut::default();
    let sheet_rid = rels.add("worksheet", "worksheets/sheet1.xml", false);
    rels.add("styles", "styles.xml", false);
    let mut book = W::new();
    book.open("workbook", A::new().a("xmlns", NS_SSML).a("xmlns:r", NS_R));
    book.open0("sheets");
    book.empty("sheet", A::new().a("name", "Sheet1").a("sheetId", 1).a("r:id", sheet_rid));
    book.close("sheets");
    book.close("workbook");

    let mut pkg = PackageOut::default();
    pkg.xml_part("xl/workbook.xml", CT_WORKBOOK, book.finish());
    pkg.rels("xl/workbook.xml", &rels);
    pkg.xml_part("xl/worksheets/sheet1.xml", CT_WORKSHEET, sheet.finish());
    pkg.xml_part("xl/styles.xml", CT_STYLES, STYLES_XML.as_bytes().to_vec());
    let mut root = RelsOut::default();
    root.add("officeDocument", "xl/workbook.xml", false);
    pkg.rels("", &root);
    pkg.finish().ok()
}

#[cfg(test)]
mod tests {
    use deckcraft_model::MediaId;

    use super::*;

    /// Cells of the workbook's first sheet by reference (`B2`), as written (number or text).
    fn sheet_cells(xlsx: &[u8]) -> BTreeMap<String, String> {
        let pkg = crate::opc::Package::open(xlsx).unwrap();
        let book = pkg.rels("").first_of("officeDocument").unwrap().target.clone();
        let sheet = pkg.rels(&book).first_of("worksheet").unwrap().target.clone();
        let doc = pkg.xml(&sheet).unwrap();
        let mut found = vec![];
        doc.root.find_all("c", &mut found);
        found
            .into_iter()
            .map(|c| {
                let v = c.child("v").map(|v| v.text()).or_else(|| c.find("t").map(|t| t.text())).unwrap_or_default();
                (c.attr("r").unwrap().to_string(), v)
            })
            .collect()
    }

    /// The cells a one-column formula covers: `Sheet1!$B$2:$B$5` → B2, B3, B4, B5.
    fn formula_cells(f: &str) -> Vec<String> {
        let refs = f.strip_prefix("Sheet1!").unwrap();
        let cell = |r: &str| {
            let r = r.replace('$', "");
            let split = r.find(|ch: char| ch.is_ascii_digit()).unwrap();
            (r[..split].to_string(), r[split..].parse::<usize>().unwrap())
        };
        let (a, b) = refs.split_once(':').unwrap_or((refs, refs));
        let ((ca, ra), (cb, rb)) = (cell(a), cell(b));
        assert_eq!(ca, cb, "{f}");
        (ra..=rb).map(|r| format!("{ca}{r}")).collect()
    }

    /// Every cached value of the chart part is in the workbook cell its formula points to.
    fn assert_formulas_match(c: &Chart) {
        let xml = chart_xml(c, None, Some("rId1"));
        let book = sheet_cells(&workbook_xlsx(c).unwrap());
        let doc = crate::xml::parse(&xml).unwrap();
        assert!(doc.root.child("externalData").and_then(|e| e.attr("r:id")) == Some("rId1"));
        let mut refs = vec![];
        doc.root.find_all("strRef", &mut refs);
        doc.root.find_all("numRef", &mut refs);
        assert!(!refs.is_empty());
        for r in refs {
            let cells = formula_cells(&r.child("f").unwrap().text());
            let mut pts = vec![];
            r.find_all("pt", &mut pts);
            for pt in pts {
                let i = pt.u32("idx").unwrap() as usize;
                let cached = pt.child("v").unwrap().text();
                let at = &cells[i];
                assert_eq!(book.get(at).map(String::as_str).unwrap_or(""), cached, "{:?}: {at}", c.kind);
            }
        }
    }

    #[test]
    fn workbook_holds_the_cached_data() {
        for kind in
            [ChartType::Column, ChartType::StackedBar, ChartType::Line, ChartType::Pie, ChartType::Scatter, ChartType::Bubble, ChartType::Radar]
        {
            assert_formulas_match(&Chart::sample(kind));
        }
        let mut combo = Chart::sample(ChartType::Column);
        combo.kind = ChartType::Combo;
        combo.series[1].kind = Some(ChartType::Line);
        assert_formulas_match(&combo);
        // Scatter series with their own X values, and several bubble series, never share cells.
        let mut scatter = Chart::sample(ChartType::Scatter);
        scatter.series[1].x_values = vec![Some(10.0), Some(20.0), Some(30.0), Some(40.0)];
        assert_formulas_match(&scatter);
        let mut bubbles = Chart::sample(ChartType::Bubble);
        bubbles.series = Chart::sample(ChartType::Column).series;
        bubbles.series[0].sizes = vec![Some(3.0), Some(1.0), Some(2.0), Some(5.0)];
        assert_formulas_match(&bubbles);
        bubbles.series[2].x_values = vec![Some(0.5), Some(1.5), Some(2.5), Some(3.5)];
        assert_formulas_match(&bubbles);
    }

    #[test]
    fn workbook_layout_is_the_familiar_one() {
        let c = Chart::sample(ChartType::Column);
        let book = sheet_cells(&workbook_xlsx(&c).unwrap());
        assert_eq!(book.get("A2").map(String::as_str), Some("Q1"));
        assert_eq!(book.get("B1").map(String::as_str), Some("Series 1"));
        assert_eq!(book.get("D5").map(String::as_str), Some("5.1"));
        assert!(!book.contains_key("A1"));
        assert!(workbook_xlsx(&Chart::default()).is_none());
    }

    #[test]
    fn unchanged_chart_keeps_its_part_and_workbook_link() {
        let first = chart_xml(&Chart::sample(ChartType::Column), None, Some("rId7"));
        // The chart as an import leaves it: read back from the part, which it keeps.
        let doc = crate::xml::parse(&first).unwrap();
        let mut read = crate::read::chart_from_el(&doc.root).unwrap();
        read.raw = Some(String::from_utf8(first.clone()).unwrap());
        read.workbook = Some(MediaId(42));
        let kept = unchanged(&read);
        assert!(kept.is_some());
        let again = String::from_utf8(chart_xml(&read, kept, Some("rId1"))).unwrap();
        assert!(again.contains(r#"<c:externalData r:id="rId1"><c:autoUpdate val="0"/></c:externalData>"#), "{again}");
        // No workbook to write: the reference goes too.
        let bare = String::from_utf8(chart_xml(&read, unchanged(&read), None)).unwrap();
        assert!(!bare.contains("externalData"));
        // Edited data: the part is rewritten from the model.
        let mut edited = read.clone();
        edited.series[0].values[0] = Some(99.0);
        assert!(unchanged(&edited).is_none());
    }
}
