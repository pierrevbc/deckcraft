//! Chart parts: a `c:chartSpace` with cached categories and values, and the workbook that holds
//! the chart data (what PowerPoint opens for Edit Data).
//!
//! - An unchanged chart keeps its original part and workbook.
//! - An imported chart whose data can be written where its workbook holds it ([`source`]) is
//!   written from the model with its original formulas, or their ranges grown, shrunk or repeated
//!   for added series. Its workbook is kept byte for byte when only the presentation changed
//!   (title, type, legend…); when the data changed, only the cells that changed are rewritten in
//!   it.
//! - Otherwise (a new chart, a category chart turned into a scatter chart…) both are written from
//!   the model, the workbook laid out where the part's formulas point.

use std::collections::BTreeMap;

use deckcraft_model::chart::{Chart, ChartType, Series, ThreeD};
use deckcraft_model::chart_format::{AxisFormat, Crosses};
use deckcraft_model::style::Fill;

use super::xlsx::{AreaMove, Edit, Range, Value, parse_range};
use crate::opc::{NS_A, NS_C, NS_R, PackageOut, RelsOut};
use crate::xml::{A, El, W};

mod format;

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
        k if k.is_scatter() && shared_x => (0..n).map(|i| Cols { x: 0, y: i + 1, size: None }).collect(),
        k if k.is_scatter() => (0..n).map(|i| Cols { x: 2 * i, y: 2 * i + 1, size: None }).collect(),
        ChartType::Bubble if shared_x => (0..n).map(|i| Cols { x: 0, y: 2 * i + 1, size: Some(2 * i + 2) }).collect(),
        ChartType::Bubble => (0..n).map(|i| Cols { x: 3 * i, y: 3 * i + 1, size: Some(3 * i + 2) }).collect(),
        _ => (0..n).map(|i| Cols { x: 0, y: i + 1, size: None }).collect(),
    }
}

/// `Sheet1!$B$2:$B$5`: rows 2 to `last` of column `c`.
fn range(c: usize, last: usize) -> String {
    format!("Sheet1!${0}$2:${0}${last}", col(c))
}

/// The workbook formulas of one series in the chart part.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Refs {
    name: String,
    /// Categories, or X values.
    x: String,
    y: String,
    size: String,
}

/// Formulas into the workbook [`workbook_xlsx`] writes.
fn generated_refs(c: &Chart) -> Vec<Refs> {
    c.series
        .iter()
        .zip(layout(c))
        .map(|(s, k)| {
            let last = c.categories.len().max(s.values.len()) + 1;
            Refs { name: format!("Sheet1!${}$1", col(k.y)), x: range(k.x, last), y: range(k.y, last), size: range(k.size.unwrap_or(k.y + 1), last) }
        })
        .collect()
}

/// An edited chart whose data can still be written where its workbook holds it: the formulas of
/// the chart part (the imported ones, or their ranges grown, shrunk or repeated for a new series),
/// the cells to update for the data that changed since (none when only the title, type, legend
/// or other options changed), and how the block of chart data moved (a table over it follows).
pub struct Source {
    pub refs: Vec<Refs>,
    pub edits: Vec<Edit>,
    pub area: Option<AreaMove>,
}

/// The cells of one series in the workbook.
#[derive(Clone, PartialEq)]
struct Place {
    name: Range,
    /// Categories, or X values.
    x: Range,
    y: Range,
    size: Option<Range>,
    /// The values run down a column (series side by side), not along a row.
    vertical: bool,
}

/// A cell: sheet (lower case, as Excel matches names), column, row.
type Key = (String, u32, u32);

/// Cells by key, with the sheet name as written.
#[derive(Default)]
struct CellMap {
    cells: BTreeMap<Key, Value>,
    sheets: BTreeMap<String, String>,
}

impl CellMap {
    /// Cell `i` of `range` holds `v`. `None` when another series already put something else there.
    fn set(&mut self, range: &Range, i: usize, v: Value) -> Option<()> {
        let (col, row) = range.cell(i)?;
        let sheet = range.sheet.to_lowercase();
        self.sheets.entry(sheet.clone()).or_insert_with(|| range.sheet.clone());
        match self.cells.get(&(sheet.clone(), col, row)) {
            Some(was) if *was != v => None,
            Some(_) => Some(()),
            None => {
                self.cells.insert((sheet, col, row), v);
                Some(())
            }
        }
    }

    fn formula(&self, key: &Key) -> String {
        let sheet = self.sheets.get(&key.0).cloned().unwrap_or_else(|| key.0.clone());
        Range { sheet, from: (key.1, key.2), to: (key.1, key.2) }.formula()
    }

    /// The sheet and corners of the block the cells fill, when they are all on one sheet.
    fn block(&self) -> Option<(String, ((u32, u32), (u32, u32)))> {
        let first = self.cells.keys().next()?;
        if self.cells.keys().any(|k| k.0 != first.0) {
            return None;
        }
        let cols = self.cells.keys().map(|k| k.1);
        let rows = self.cells.keys().map(|k| k.2);
        let corners = ((cols.clone().min()?, rows.clone().min()?), (cols.max()?, rows.max()?));
        Some((self.sheets.get(&first.0)?.clone(), corners))
    }
}

fn num(v: Option<&Option<f64>>) -> Value {
    v.copied().flatten().filter(|v| v.is_finite()).map_or(Value::Blank, Value::Number)
}

fn text(v: Option<&String>) -> Value {
    v.filter(|t| !t.is_empty()).map_or(Value::Blank, |t| Value::Text(t.clone()))
}

/// `r` with `n` cells, unchanged when it has them.
fn fit(r: &Range, n: usize, vertical: bool) -> Option<Range> {
    if r.len() == n { Some(r.clone()) } else { r.resized(n, vertical) }
}

/// The position of a series range across the series: its column for series side by side.
fn across(r: &Range, vertical: bool) -> u32 {
    if vertical { r.from.0 } else { r.from.1 }
}

/// [`Source`] of a chart read from a PPTX. `None` when its data can't be written where the
/// workbook holds it: a category chart turned into a scatter or bubble chart (or back), data the
/// chart part held as literals rather than formulas, ranges that aren't one row or column, two
/// series that would need different values in the same cell. The chart then gets a workbook of
/// its own, as a new chart does.
///
/// Series and points added or removed keep the workbook: ranges grow or shrink from their first
/// cell, a new series goes next to the last one (the same distance from it as that one from the
/// one before), and the cells the chart no longer reads are cleared. Cells it reads for the first
/// time must be empty in the workbook ([`Edit::fresh`]).
pub fn source(c: &Chart) -> Option<Source> {
    let src = c.source.as_ref()?;
    let xy = c.kind.is_xy();
    let bubble = c.kind == ChartType::Bubble;
    if src.xy != xy || src.series.is_empty() || c.series.is_empty() {
        return None;
    }
    // Where the series were read from, and what those cells held.
    let mut old: Vec<Place> = vec![];
    let mut known = CellMap::default();
    for o in &src.series {
        let name = parse_range(o.name_ref.as_deref()?)?;
        let x = parse_range(o.categories_ref.as_deref()?)?;
        let y = parse_range(o.values_ref.as_deref()?)?;
        let size = if bubble { Some(parse_range(o.sizes_ref.as_deref()?)?) } else { None };
        let vertical = y.vertical().or(x.vertical()).unwrap_or(name.from.0 == y.from.0 || name.from.1 != y.from.1);
        let along = |r: &Range| r.vertical().is_none_or(|v| v == vertical);
        if name.len() != 1 || !along(&x) || !along(&y) || !size.as_ref().is_none_or(along) {
            return None;
        }
        known.set(&name, 0, text(Some(&o.name)))?;
        for i in 0..y.len() {
            known.set(&y, i, num(o.values.get(i)))?;
        }
        for i in 0..x.len() {
            known.set(&x, i, if xy { num(o.x_values.get(i)) } else { text(src.categories.get(i)) })?;
        }
        if let Some(z) = &size {
            for i in 0..z.len() {
                known.set(z, i, num(o.sizes.get(i)))?;
            }
        }
        old.push(Place { name, x, y, size, vertical });
    }

    // Where the series go now.
    let n = c.point_count();
    let last = old.last()?.clone();
    let v = last.vertical;
    let x_shared = old.iter().all(|p| p.x == last.x);
    let stride = match old.len().checked_sub(2).and_then(|i| old.get(i)) {
        Some(prev) => across(&last.y, v).checked_sub(across(&prev.y, v)).filter(|d| *d > 0)?,
        None => 1 + u32::from(bubble) + u32::from(!x_shared),
    };
    let mut places = vec![];
    for i in 0..c.series.len() {
        let p = match old.get(i) {
            Some(p) => Place {
                name: p.name.clone(),
                x: fit(&p.x, n, p.vertical)?,
                y: fit(&p.y, n, p.vertical)?,
                size: match &p.size {
                    Some(z) => Some(fit(z, n, p.vertical)?),
                    None => None,
                },
                vertical: p.vertical,
            },
            None => {
                let by = stride.checked_mul(u32::try_from(i + 1 - old.len()).ok()?)?;
                Place {
                    name: last.name.shifted(by, v)?,
                    x: if x_shared { fit(&last.x, n, v)? } else { fit(&last.x.shifted(by, v)?, n, v)? },
                    y: fit(&last.y.shifted(by, v)?, n, v)?,
                    size: match &last.size {
                        Some(z) => Some(fit(&z.shifted(by, v)?, n, v)?),
                        None => None,
                    },
                    vertical: v,
                }
            }
        };
        places.push(p);
    }
    let mut target = CellMap::default();
    let mut refs = vec![];
    for (i, (s, p)) in c.series.iter().zip(&places).enumerate() {
        target.set(&p.name, 0, text(Some(&s.name)))?;
        let xs = c.x_values_of(i);
        for j in 0..n {
            target.set(&p.y, j, num(s.values.get(j)))?;
            target.set(&p.x, j, if xy { num(xs.get(j)) } else { text(c.categories.get(j)) })?;
            if let Some(z) = &p.size {
                target.set(z, j, num(s.sizes.get(j)))?;
            }
        }
        // The formulas as they were when the ranges didn't change.
        let o = old.get(i).zip(src.series.get(i));
        let keep = |now: &Range, was: Option<&Range>, f: Option<&String>| match (was, f) {
            (Some(w), Some(f)) if w == now => f.clone(),
            _ => now.formula(),
        };
        refs.push(Refs {
            name: keep(&p.name, o.map(|(w, _)| &w.name), o.and_then(|(_, f)| f.name_ref.as_ref())),
            x: keep(&p.x, o.map(|(w, _)| &w.x), o.and_then(|(_, f)| f.categories_ref.as_ref())),
            y: keep(&p.y, o.map(|(w, _)| &w.y), o.and_then(|(_, f)| f.values_ref.as_ref())),
            size: match &p.size {
                Some(z) => keep(z, o.and_then(|(w, _)| w.size.as_ref()), o.and_then(|(_, f)| f.sizes_ref.as_ref())),
                None => o.and_then(|(_, f)| f.sizes_ref.clone()).unwrap_or_default(),
            },
        });
    }

    // The cells that change, those read for the first time, and those no longer read.
    let mut edits = vec![];
    for (key, value) in &target.cells {
        let fresh = match known.cells.get(key) {
            Some(was) if was == value => continue,
            Some(_) => false,
            None => true,
        };
        edits.push(Edit { formula: target.formula(key), index: 0, value: value.clone(), fresh });
    }
    for (key, was) in &known.cells {
        if !target.cells.contains_key(key) && *was != Value::Blank {
            edits.push(Edit { formula: known.formula(key), index: 0, value: Value::Blank, fresh: false });
        }
    }
    let area = match (known.block(), target.block()) {
        (Some((sheet, from)), Some((to_sheet, to))) if from != to && sheet.eq_ignore_ascii_case(&to_sheet) => Some(AreaMove { sheet, from, to }),
        _ => None,
    };
    Some(Source { refs, edits, area })
}

fn sp_pr(w: &mut W, fill: Option<&Fill>, line: Option<&deckcraft_model::Line>) {
    format::sp_pr(w, fill, line);
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
    OfPie,
    Scatter,
    Radar,
    Bubble,
    Stock,
}

fn group_of(k: ChartType) -> Group {
    match k {
        k if k.is_line() => Group::Line,
        k if k.is_area() => Group::Area,
        ChartType::Surface => Group::Area,
        ChartType::Pie | ChartType::ExplodedPie => Group::Pie,
        ChartType::Doughnut | ChartType::ExplodedDoughnut | ChartType::Sunburst => Group::Doughnut,
        ChartType::PieOfPie | ChartType::BarOfPie => Group::OfPie,
        k if k.is_scatter() => Group::Scatter,
        ChartType::Bubble => Group::Bubble,
        k if k.is_radar() => Group::Radar,
        k if k.is_stock() => Group::Stock,
        _ => Group::Bar,
    }
}

/// The ids of the axes a plot group uses: category (or X), value (or Y), and the depth axis of a
/// 3-D chart that has one.
#[derive(Clone, Copy)]
struct AxIds {
    cat: u32,
    val: u32,
    ser: Option<u32>,
}

const PRIMARY: AxIds = AxIds { cat: AX_CAT, val: AX_VAL, ser: None };
const SECONDARY: AxIds = AxIds { cat: AX_CAT2, val: AX_VAL2, ser: None };

/// The XML element of a plot group, 2-D or 3-D.
fn group_tag(g: Group, three_d: bool) -> &'static str {
    match (g, three_d) {
        (Group::Bar, false) => "c:barChart",
        (Group::Bar, true) => "c:bar3DChart",
        (Group::Line, false) => "c:lineChart",
        (Group::Line, true) => "c:line3DChart",
        (Group::Area, false) => "c:areaChart",
        (Group::Area, true) => "c:area3DChart",
        (Group::Pie, false) => "c:pieChart",
        (Group::Pie, true) => "c:pie3DChart",
        (Group::Doughnut, _) => "c:doughnutChart",
        (Group::OfPie, _) => "c:ofPieChart",
        (Group::Scatter, _) => "c:scatterChart",
        (Group::Radar, _) => "c:radarChart",
        (Group::Bubble, _) => "c:bubbleChart",
        (Group::Stock, _) => "c:stockChart",
    }
}

/// Whether `g` exists as a 3-D plot group.
fn has_3d(g: Group) -> bool {
    matches!(g, Group::Bar | Group::Line | Group::Area | Group::Pie)
}

fn marker(w: &mut W, symbol: &str, size: Option<u32>) {
    if !symbol.chars().all(|c| c.is_ascii_alphanumeric()) {
        return;
    }
    w.open0("c:marker");
    w.val("c:symbol", symbol);
    if let Some(n) = size {
        w.val("c:size", n);
    }
    w.close("c:marker");
}

#[allow(clippy::too_many_arguments)]
fn series(w: &mut W, c: &Chart, s: &Series, i: usize, r: &Refs, kind: ChartType, g: Group, close: bool) {
    w.open0("c:ser");
    w.val("c:idx", i);
    w.val("c:order", i);
    w.open0("c:tx");
    str_cache(w, &r.name, std::slice::from_ref(&s.name));
    w.close("c:tx");
    // A palette other than the default one writes its colours, which nothing else records.
    let palette = c.palette != deckcraft_model::chart_format::DEFAULT_PALETTE;
    let n = c.series.len();
    let pie = matches!(g, Group::Pie | Group::Doughnut | Group::OfPie);
    let vary_points = pie && (c.vary_colors || g == Group::Doughnut);
    let colour = || Fill::solid(deckcraft_model::chart_format::palette_color(&c.palette, i, n));
    // Lines take the colour on their outline (and markers), the others on their fill.
    let lines = matches!(g, Group::Line | Group::Scatter) || (g == Group::Radar && kind != ChartType::FilledRadar);
    let own = (palette && s.fill.is_none() && !vary_points && !lines).then(colour);
    let own_line = (palette && lines && s.line.as_ref().and_then(|l| l.fill.as_ref()).is_none())
        .then(|| deckcraft_model::Line { fill: Some(colour()), ..s.line.clone().unwrap_or_default() });
    let line = own_line.as_ref().or(s.line.as_ref());
    if kind.points_only() || g == Group::Stock {
        // No line at all for the types that draw none (points only, stock prices: their high-low
        // lines and up-down bars are the group's).
        let none = deckcraft_model::Line { fill: Some(Fill::None), width: line.and_then(|l| l.width), ..Default::default() };
        sp_pr(w, s.fill.as_ref().or(own.as_ref()), Some(&none));
    } else {
        sp_pr(w, s.fill.as_ref().or(own.as_ref()), line);
    }
    if g == Group::Bar {
        w.val("c:invertIfNegative", if s.format.invert_if_negative { 1 } else { 0 });
    }
    if matches!(g, Group::Pie | Group::Doughnut) {
        let x = s.format.explosion.or(kind.exploded().then(|| c.variant.explosion_or_default()));
        if let Some(x) = x.filter(|x| *x > 0) {
            w.val("c:explosion", x.min(400));
        }
    }
    match g {
        Group::Stock if close && !kind.has_open() => marker(w, "dash", Some(7)),
        Group::Stock => marker(w, "none", None),
        Group::Line | Group::Scatter | Group::Radar => {
            // The type says whether markers show, the series which symbol (the type's default
            // when it picks none).
            let sym = match s.marker.as_deref() {
                _ if !kind.markers() => Some("none"),
                Some(m) if !m.is_empty() && m != "auto" => Some(m),
                _ => None,
            };
            // Markers without a line: their colour is spelled out, as applications that take a
            // marker's colour from the series line would draw them all alike.
            let auto = if kind.points_only() { Some(s.fill.clone().unwrap_or_else(colour)) } else { own_line.as_ref().and_then(|l| l.fill.clone()) };
            format::marker(w, sym, s, auto.as_ref());
        }
        _ => {}
    }
    let points = s.values.len();
    let vary = move |k: usize| Fill::solid(deckcraft_model::chart_format::palette_color(&c.palette, k, points));
    format::points(w, s, g, (palette && vary_points).then_some(&vary as &dyn Fn(usize) -> Fill));
    if let Some(d) = s.data_labels {
        format::dlbls(w, &c.format.labels, d, g, kind.stacked());
    }
    if matches!(g, Group::Scatter | Group::Bubble) {
        w.open0("c:xVal");
        num_cache(w, &r.x, &xs(s));
        w.close("c:xVal");
        w.open0("c:yVal");
        num_cache(w, &r.y, &s.values);
        w.close("c:yVal");
        if g == Group::Bubble {
            w.open0("c:bubbleSize");
            num_cache(w, &r.size, &sizes(s));
            w.close("c:bubbleSize");
            w.val("c:bubble3D", 0);
        } else {
            w.val("c:smooth", if s.smooth { 1 } else { 0 });
        }
    } else {
        w.open0("c:cat");
        str_cache(w, &r.x, &c.categories);
        w.close("c:cat");
        w.open0("c:val");
        num_cache(w, &r.y, &s.values);
        w.close("c:val");
        if matches!(g, Group::Line | Group::Stock) {
            w.val("c:smooth", if s.smooth && g == Group::Line { 1 } else { 0 });
        }
    }
    w.close("c:ser");
}

const AX_CAT: u32 = 50_001;
const AX_VAL: u32 = 50_002;
const AX_CAT2: u32 = 50_003;
const AX_VAL2: u32 = 50_004;
const AX_SER: u32 = 50_005;

/// The axes of plot groups of kind `g` using `ids`, with the chart's axis formatting;
/// `secondary` ones sit on the right, their category axis hidden, unformatted.
fn axes(w: &mut W, c: &Chart, g: Group, ids: AxIds, secondary: bool) {
    let horizontal = c.kind.horizontal() && g == Group::Bar;
    let (cat_pos, val_pos) = match (horizontal, secondary) {
        (true, false) => ("l", "b"),
        (true, true) => ("r", "t"),
        (false, false) => ("b", "l"),
        (false, true) => ("t", "r"),
    };
    let xy = g == Group::Scatter || g == Group::Bubble;
    let hidden = AxisFormat { hidden: true, ..Default::default() };
    let at_max = AxisFormat { crosses: Some(Crosses::Max), ..Default::default() };
    format::axis(
        w,
        &format::AxisSpec {
            tag: if xy { "c:valAx" } else { "c:catAx" },
            id: ids.cat,
            cross: ids.val,
            pos: cat_pos,
            fmt: if secondary { &hidden } else { &c.format.cat_axis },
            major_gridlines: !secondary && c.format.cat_axis.major_gridlines,
            title: c.axis_titles.0.as_deref().filter(|_| !secondary),
            min: if xy { c.format.cat_axis.min } else { None },
            max: if xy { c.format.cat_axis.max } else { None },
            num_fmt: "General",
            // No tick marks unless the axis sets them, as it reads back.
            major_tick: "none",
            cross_between: xy.then_some("midCat"),
        },
    );
    format::axis(
        w,
        &format::AxisSpec {
            tag: "c:valAx",
            id: ids.val,
            cross: ids.cat,
            pos: val_pos,
            fmt: if secondary { &at_max } else { &c.format.val_axis },
            major_gridlines: c.gridlines && !secondary,
            title: c.axis_titles.1.as_deref().filter(|_| !secondary),
            min: if secondary { None } else { c.y_min },
            max: if secondary { None } else { c.y_max },
            num_fmt: if c.kind.percent() { "0%" } else { "General" },
            major_tick: "none",
            cross_between: Some(if g == Group::Area || g == Group::Scatter { "midCat" } else { "between" }),
        },
    );
    if let Some(ser) = ids.ser {
        w.open0("c:serAx");
        w.val("c:axId", ser);
        w.open0("c:scaling");
        w.val("c:orientation", "minMax");
        w.close("c:scaling");
        w.val("c:delete", 0);
        w.val("c:axPos", "b");
        w.val("c:majorTickMark", "none");
        w.val("c:minorTickMark", "none");
        w.val("c:tickLblPos", "nextTo");
        w.val("c:crossAx", ids.val);
        w.val("c:crosses", "autoZero");
        w.close("c:serAx");
    }
}

/// One plot group (`c:barChart`…) of `kind` holding `list`, on axes `ids`; 3-D when `three_d`.
fn plot_group(w: &mut W, c: &Chart, kind: ChartType, list: &[(usize, &Series)], refs: &[Refs], ids: AxIds, three_d: Option<&ThreeD>) {
    let g = group_of(kind);
    let three_d = three_d.filter(|_| has_3d(g));
    let deep = three_d.is_some_and(|t| t.deep || g == Group::Line);
    let grouping = if kind.percent() {
        "percentStacked"
    } else if kind.stacked() {
        "stacked"
    } else if matches!(g, Group::Bar) && !deep {
        "clustered"
    } else {
        "standard"
    };
    let vary = if c.vary_colors { 1 } else { 0 };
    let tag = group_tag(g, three_d.is_some());
    w.open0(tag);
    match g {
        Group::Bar => {
            w.val("c:barDir", if kind.horizontal() { "bar" } else { "col" });
            w.val("c:grouping", grouping);
        }
        Group::Line | Group::Area => w.val("c:grouping", grouping),
        Group::Scatter => {
            let smooth = list.iter().any(|(_, s)| s.smooth);
            w.val("c:scatterStyle", if smooth { "smoothMarker" } else { "lineMarker" });
        }
        Group::Radar => w.val("c:radarStyle", if kind == ChartType::FilledRadar { "filled" } else { "marker" }),
        Group::OfPie => w.val("c:ofPieType", if kind == ChartType::BarOfPie { "bar" } else { "pie" }),
        _ => {}
    }
    if g != Group::Stock {
        w.val("c:varyColors", vary);
    }
    let last = list.len().saturating_sub(1);
    for (k, (i, s)) in list.iter().enumerate() {
        series(w, c, s, *i, refs.get(*i).unwrap_or(&Refs::default()), kind, g, k == last);
    }
    if c.data_labels {
        format::dlbls(w, &c.format.labels, true, g, kind.stacked());
    }
    match g {
        Group::Bar => {
            w.val("c:gapWidth", (c.gap_width.clamp(0.0, 5.0) * 100.0).round() as i64);
            if let Some(t) = three_d {
                w.val("c:shape", t.shape.ooxml());
            } else {
                let ov = if kind.stacked() { 100 } else { (c.overlap.clamp(-1.0, 1.0) * 100.0).round() as i64 };
                if ov != 0 {
                    w.val("c:overlap", ov);
                }
            }
        }
        Group::Line if three_d.is_none() => w.val("c:marker", 1),
        Group::Pie if three_d.is_none() => w.val("c:firstSliceAng", first_slice(c)),
        Group::Doughnut => {
            w.val("c:firstSliceAng", first_slice(c));
            w.val("c:holeSize", (c.hole_size.clamp(0.1, 0.9) * 100.0).round() as i64);
        }
        Group::OfPie => {
            w.val("c:gapWidth", 100);
            if c.variant.split_pos == 0 {
                w.val("c:splitType", "auto");
            } else {
                w.val("c:splitType", "pos");
                w.val("c:splitPos", c.variant.split_pos);
            }
            w.val("c:secondPieSize", c.variant.second_size_or_default());
            w.empty0("c:serLines");
        }
        Group::Stock => {
            w.empty0("c:hiLowLines");
            if kind.has_open() {
                w.open0("c:upDownBars");
                w.val("c:gapWidth", 150);
                w.empty0("c:upBars");
                w.empty0("c:downBars");
                w.close("c:upDownBars");
            }
        }
        _ => {}
    }
    if !matches!(g, Group::Pie | Group::Doughnut | Group::OfPie) {
        w.val("c:axId", ids.cat);
        w.val("c:axId", ids.val);
        if let Some(ser) = ids.ser {
            w.val("c:axId", ser);
        }
    }
    w.close(tag);
}

/// Angle of the first pie slice, whole degrees 0–360.
fn first_slice(c: &Chart) -> i64 {
    let a = c.format.first_slice_angle;
    if a.is_finite() { (a.rem_euclid(360.0).round() as i64).clamp(0, 360) } else { 0 }
}

/// The plot groups of a chart that mixes types or axes: one per series type and axis, in the
/// order of their first series. Types a combination can't hold become columns or lines.
fn combo_groups(c: &Chart) -> Vec<(ChartType, bool, Vec<(usize, &Series)>)> {
    let main = match c.kind {
        ChartType::Combo => c.series.first().and_then(|s| s.kind).unwrap_or(ChartType::Column),
        k => k,
    };
    let mut out: Vec<(ChartType, bool, Vec<(usize, &Series)>)> = vec![];
    for (i, s) in c.series.iter().enumerate() {
        let k = if c.kind == ChartType::Combo { s.kind.unwrap_or(main) } else { main };
        let k = match k {
            k if k.is_pie() || k.is_xy() => ChartType::Column,
            k if k.is_radar() || k.is_stock() => ChartType::Line,
            k => k,
        };
        match out.iter_mut().find(|(gk, sec, _)| *gk == k && *sec == s.secondary_axis) {
            Some((_, _, list)) => list.push((i, s)),
            None => out.push((k, s.secondary_axis, vec![(i, s)])),
        }
    }
    // A secondary axis needs a primary one.
    if out.iter().all(|(_, sec, _)| *sec) {
        for g in &mut out {
            g.1 = false;
        }
    }
    out
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

fn view_3d(w: &mut W, t: &ThreeD) {
    w.open0("c:view3D");
    w.val("c:rotX", t.rot_x.clamp(-90, 90));
    w.val("c:rotY", t.rot_y.min(360));
    w.val("c:depthPercent", t.depth_percent.clamp(20, 2000));
    w.val("c:rAngAx", if t.right_angle_axes { 1 } else { 0 });
    if !t.right_angle_axes {
        w.val("c:perspective", t.perspective.min(240));
    }
    w.close("c:view3D");
}

/// The plot groups and axes of the chart.
fn plot_area(w: &mut W, c: &Chart, refs: &[Refs]) {
    let indexed: Vec<(usize, &Series)> = c.series.iter().enumerate().collect();
    let combo = c.kind == ChartType::Combo
        || (c.series.iter().any(|s| s.secondary_axis) && !c.kind.is_pie() && !c.kind.is_xy() && !c.kind.is_stock() && !c.kind.is_radar());
    if combo {
        let groups = combo_groups(c);
        for (k, sec, list) in &groups {
            plot_group(w, c, *k, list, refs, if *sec { SECONDARY } else { PRIMARY }, None);
        }
        let first = groups.first().map_or(Group::Bar, |g| group_of(g.0));
        axes(w, c, first, PRIMARY, false);
        if let Some((k, _, _)) = groups.iter().find(|g| g.1) {
            axes(w, c, group_of(*k), SECONDARY, true);
        }
        return;
    }
    if c.kind.is_stock() {
        let (volume, prices) = if c.kind.has_volume() { indexed.split_at(1.min(indexed.len())) } else { (&[][..], &indexed[..]) };
        let price_ids = if volume.is_empty() { PRIMARY } else { SECONDARY };
        if !volume.is_empty() {
            plot_group(w, c, ChartType::Column, volume, refs, PRIMARY, None);
        }
        // A stock group holds three or four series; other counts are drawn as lines.
        let kind = if (3..=4).contains(&prices.len()) { c.kind } else { ChartType::Line };
        if !prices.is_empty() {
            plot_group(w, c, kind, prices, refs, price_ids, None);
        }
        axes(w, c, Group::Stock, PRIMARY, false);
        if !volume.is_empty() && !prices.is_empty() {
            axes(w, c, Group::Stock, SECONDARY, true);
        }
        return;
    }
    if !natively_written(c.kind) {
        log::warn!("pptx: {} chart written as a column chart", c.kind.label());
    }
    let g = group_of(c.kind);
    let three_d = c.variant.three_d.as_ref().filter(|_| has_3d(g));
    let ids = match three_d {
        Some(t) if t.deep || g == Group::Line => AxIds { ser: Some(AX_SER), ..PRIMARY },
        _ => PRIMARY,
    };
    plot_group(w, c, c.kind, &indexed, refs, ids, three_d);
    if !matches!(g, Group::Pie | Group::Doughnut | Group::OfPie) {
        axes(w, c, g, ids, false);
    }
}

/// Types with a chart part element of their own (the others are written as column charts).
fn natively_written(k: ChartType) -> bool {
    !matches!(
        k,
        ChartType::Surface
            | ChartType::Histogram
            | ChartType::Pareto
            | ChartType::BoxWhisker
            | ChartType::Waterfall
            | ChartType::Funnel
            | ChartType::Treemap
            | ChartType::Sunburst
    )
}

/// The chart part XML. `kept` is the original part from [`unchanged`]; `workbook` is the id of
/// the relationship to the data workbook, when one is written; `refs` are the series formulas
/// into it, those of [`source`] or, when `None`, those of the workbook [`workbook_xlsx`] writes.
pub fn chart_xml(c: &Chart, kept: Option<El>, workbook: Option<&str>, refs: Option<&[Refs]>) -> Vec<u8> {
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
    let refs = refs.map(<[Refs]>::to_vec).unwrap_or_else(|| generated_refs(c));
    let mut w = W::new();
    w.open("c:chartSpace", A::new().a("xmlns:c", NS_C).a("xmlns:a", NS_A).a("xmlns:r", NS_R));
    w.val("c:date1904", 0);
    w.val("c:roundedCorners", if c.format.rounded_corners { 1 } else { 0 });
    w.val("c:style", c.style.clamp(1, 48));
    w.open0("c:chart");
    match &c.title {
        Some(t) => {
            let f = &c.format;
            format::title(&mut w, t, &f.title_text, f.title_overlay, f.title_fill.as_ref(), f.title_line.as_ref());
            w.val("c:autoTitleDeleted", 0);
        }
        None => w.val("c:autoTitleDeleted", 1),
    }
    if let Some(t) = &c.variant.three_d
        && c.kind != ChartType::Combo
        && has_3d(group_of(c.kind))
    {
        view_3d(&mut w, t);
    }
    w.open0("c:plotArea");
    w.empty0("c:layout");
    plot_area(&mut w, c, &refs);
    sp_pr(&mut w, c.plot_fill.as_ref(), c.format.plot_line.as_ref());
    w.close("c:plotArea");
    format::legend(&mut w, c);
    w.val("c:plotVisOnly", 1);
    w.val("c:dispBlanksAs", "gap");
    w.close("c:chart");
    sp_pr(&mut w, c.chart_fill.as_ref(), c.format.chart_line.as_ref());
    let text = c.format.text.over(&deckcraft_model::chart_format::ChartText { color: c.text_color.clone(), ..Default::default() });
    format::tx_pr(&mut w, &text, None);
    if let Some(rid) = workbook {
        w.open("c:externalData", A::new().a("r:id", rid));
        w.val("c:autoUpdate", 0);
        w.close("c:externalData");
    }
    if c.variant.stepped && c.kind.has_lines() {
        // OOXML has no stepped lines: our own extension keeps them, other applications draw
        // straight lines.
        w.open0("c:extLst");
        w.open("c:ext", A::new().a("uri", crate::CHART_EXT_URI).a("xmlns:dc", crate::NS_DECKCRAFT_CHART));
        w.empty("dc:stepped", A::new().a("val", 1));
        w.close("c:ext");
        w.close("c:extLst");
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
    let xy = c.kind.is_xy();
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
        let xml = chart_xml(c, None, Some("rId1"), None);
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

    /// A gallery entry as a new chart gets it.
    fn preset_chart(p: &deckcraft_model::chart::ChartPreset) -> Chart {
        let mut c = Chart::sample(p.kind);
        p.apply(&mut c);
        c
    }

    /// The local names of the elements of `xml`, in document order.
    fn element_names(xml: &[u8]) -> Vec<String> {
        fn walk(e: &El, out: &mut Vec<String>) {
            out.push(e.local().to_string());
            for c in e.elements() {
                walk(c, out);
            }
        }
        let doc = crate::xml::parse(xml).unwrap();
        let mut out = vec![];
        walk(&doc.root, &mut out);
        out
    }

    #[test]
    fn every_gallery_type_gets_a_workbook_matching_its_formulas() {
        for p in deckcraft_model::chart::ChartPreset::GALLERY {
            assert_formulas_match(&preset_chart(p));
        }
    }

    #[test]
    fn gallery_types_write_their_own_plot_groups() {
        let groups = |id: &str| {
            let p = deckcraft_model::chart::ChartPreset::find(id).unwrap();
            let names = element_names(&chart_xml(&preset_chart(p), None, None, None));
            names
                .into_iter()
                .filter(|n| n.ends_with("Chart") || matches!(n.as_str(), "catAx" | "valAx" | "serAx" | "dateAx" | "view3D"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(groups("column"), "barChart catAx valAx");
        assert_eq!(groups("columnCylinder"), "view3D bar3DChart catAx valAx");
        assert_eq!(groups("deepColumn3d"), "view3D bar3DChart catAx valAx serAx");
        assert_eq!(groups("line3d"), "view3D line3DChart catAx valAx serAx");
        assert_eq!(groups("area3d"), "view3D area3DChart catAx valAx serAx");
        assert_eq!(groups("stackedArea3d"), "view3D area3DChart catAx valAx");
        assert_eq!(groups("pie3d"), "view3D pie3DChart");
        assert_eq!(groups("explodedPie"), "pieChart");
        assert_eq!(groups("explodedDoughnut"), "doughnutChart");
        assert_eq!(groups("pieOfPie"), "ofPieChart");
        assert_eq!(groups("barOfPie"), "ofPieChart");
        assert_eq!(groups("scatterSmooth"), "scatterChart valAx valAx");
        assert_eq!(groups("bubble"), "bubbleChart valAx valAx");
        assert_eq!(groups("radarPoints"), "radarChart catAx valAx");
        assert_eq!(groups("stock"), "stockChart catAx valAx");
        assert_eq!(groups("stockOhlc"), "stockChart catAx valAx");
        assert_eq!(groups("stockVhlc"), "barChart stockChart catAx valAx catAx valAx");
        assert_eq!(groups("stockVohlc"), "barChart stockChart catAx valAx catAx valAx");
        assert_eq!(groups("columnLine"), "barChart lineChart catAx valAx");
        assert_eq!(groups("columnLineSecondary"), "barChart lineChart catAx valAx catAx valAx");
        // Stepped lines are kept in our extension, which other applications skip.
        let stepped =
            String::from_utf8(chart_xml(&preset_chart(deckcraft_model::chart::ChartPreset::find("lineStepped").unwrap()), None, None, None)).unwrap();
        assert!(stepped.contains(r#"<c:ext uri="urn:deckcraft:chart" xmlns:dc="urn:deckcraft:chart"><dc:stepped val="1"/></c:ext>"#), "{stepped}");
        // Points only: the series draw no line.
        let points = String::from_utf8(chart_xml(&Chart::sample(ChartType::LinePoints), None, None, None)).unwrap();
        assert_eq!(points.matches("<a:ln><a:noFill/></a:ln>").count(), 3, "{points}");
        // A 3-D bar group has a shape and no overlap; the 2-D one no shape.
        let cyl =
            String::from_utf8(chart_xml(&preset_chart(deckcraft_model::chart::ChartPreset::find("barCylinder").unwrap()), None, None, None)).unwrap();
        assert!(cyl.contains(r#"<c:shape val="cylinder"/>"#) && !cyl.contains("c:overlap"), "{cyl}");
    }

    #[test]
    fn stock_groups_hold_three_or_four_series() {
        // Two prices can't make a stock group: they are written as lines.
        let mut c = Chart::sample(ChartType::Stock);
        c.series.truncate(2);
        let names = element_names(&chart_xml(&c, None, None, None));
        assert!(names.iter().any(|n| n == "lineChart") && !names.iter().any(|n| n == "stockChart"), "{names:?}");
        let mut c = Chart::sample(ChartType::StockVohlc);
        c.series.clear();
        let _ = chart_xml(&c, None, None, None);
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
        let first = chart_xml(&Chart::sample(ChartType::Column), None, Some("rId7"), None);
        // The chart as an import leaves it: read back from the part, which it keeps.
        let doc = crate::xml::parse(&first).unwrap();
        let mut read = crate::read::chart_from_el(&doc.root).unwrap();
        read.raw = Some(String::from_utf8(first.clone()).unwrap());
        read.workbook = Some(MediaId(42));
        let kept = unchanged(&read);
        assert!(kept.is_some());
        let again = String::from_utf8(chart_xml(&read, kept, Some("rId1"), None)).unwrap();
        assert!(again.contains(r#"<c:externalData r:id="rId1"><c:autoUpdate val="0"/></c:externalData>"#), "{again}");
        // No workbook to write: the reference goes too.
        let bare = String::from_utf8(chart_xml(&read, unchanged(&read), None, None)).unwrap();
        assert!(!bare.contains("externalData"));
        // Edited data: the part is rewritten from the model.
        let mut edited = read.clone();
        edited.series[0].values[0] = Some(99.0);
        assert!(unchanged(&edited).is_none());
    }
}
