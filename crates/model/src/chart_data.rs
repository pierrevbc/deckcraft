//! Chart data as a table, the way a data sheet shows it: one row per point (category), one column
//! per value a series has (categories, X values, Y values, bubble sizes), series names on the
//! header row. The data editor, the `chart.*` data commands and tab-separated copy and paste all go
//! through this view, so each chart type gets the columns it needs:
//!
//! | Data | Chart types | Columns |
//! |---|---|---|
//! | [`DataShape::Categories`] | column, bar, line, area, pie, doughnut, radar, stock, combo… | categories, then one value column per series |
//! | [`DataShape::Xy`] | scatter | X values (shared, or one per series), then Y values |
//! | [`DataShape::Bubble`] | bubble | X values (shared, or one per series), then Y values and sizes |
//!
//! Every edit keeps the series the same length as the table (gaps are `None`), so the chart part
//! and its workbook get one range per series.

use serde::Serialize;

use crate::chart::{Chart, ChartType, Series};

/// Most points (rows) a chart's data can have, as many as a chart part read from a file.
pub const MAX_POINTS: usize = 100_000;
/// Most series a chart's data can have.
pub const MAX_SERIES: usize = 1_000;

/// What a chart's data is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataShape {
    /// Category labels and one value per category for each series.
    Categories,
    /// X and Y values for each series.
    Xy,
    /// X and Y values and a bubble size for each series.
    Bubble,
}

impl ChartType {
    /// The data this chart type plots.
    pub fn data_shape(self) -> DataShape {
        match self {
            k if k.is_scatter() => DataShape::Xy,
            ChartType::Bubble => DataShape::Bubble,
            _ => DataShape::Categories,
        }
    }
}

/// What one column of the data table holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "role")]
pub enum DataColumn {
    /// Category labels.
    Categories,
    /// X values: of every series when `series` is `None` (they all share them), or of one series.
    X {
        #[serde(skip_serializing_if = "Option::is_none")]
        series: Option<usize>,
    },
    /// Values (Y values for scatter and bubble charts). The header cell is the series name.
    Values { series: usize },
    /// Bubble sizes.
    Sizes { series: usize },
}

/// A number typed or pasted in a data cell: `Ok(None)` for an empty cell (a gap in the series),
/// `Err` for text that is not a number. Accepts a decimal comma (`3,5`), thousands separators
/// (`1 234,5`, `1,234.5`, `1.234,5`, `1'234`), a leading `+` and a trailing `%` (`12%` is 0.12).
pub fn parse_number(text: &str) -> Result<Option<f64>, String> {
    let t: String = text.chars().filter(|c| !c.is_whitespace() && !matches!(c, '\u{a0}' | '\u{202f}' | '\'')).collect();
    if t.is_empty() {
        return Ok(None);
    }
    let (t, scale) = match t.strip_suffix('%') {
        Some(rest) => (rest.to_string(), 0.01),
        None => (t, 1.0),
    };
    let t = t.strip_prefix('+').unwrap_or(&t).to_string();
    let (commas, dots) = (t.matches(',').count(), t.matches('.').count());
    let plain = match (commas, dots) {
        (0, 0) | (0, 1) => t.clone(),
        // `3,5`: a decimal comma.
        (1, 0) => t.replace(',', "."),
        // `1,234,567` or `1.234.567`: thousands only.
        (_, 0) => t.replace(',', ""),
        (0, _) => t.replace('.', ""),
        // Both: the last one is the decimal separator.
        _ => {
            let (last_comma, last_dot) = (t.rfind(',').unwrap_or(0), t.rfind('.').unwrap_or(0));
            if last_comma > last_dot { t.replace('.', "").replace(',', ".") } else { t.replace(',', "") }
        }
    };
    // Only plain decimal notation (with an exponent): no `inf`, `NaN` or hex.
    if !plain.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E' | '+')) || !plain.chars().any(|c| c.is_ascii_digit()) {
        return Err(format!("“{}” is not a number", text.trim()));
    }
    match plain.parse::<f64>() {
        Ok(v) if (v * scale).is_finite() => Ok(Some(v * scale)),
        _ => Err(format!("“{}” is not a number", text.trim())),
    }
}

/// A value as the data table shows it: `4` rather than `4.0`, the shortest exact form otherwise.
pub fn format_number(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if v == v.trunc() && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
}

fn fmt_cell(v: Option<f64>) -> String {
    v.map(format_number).unwrap_or_default()
}

/// Tab-separated text (what spreadsheets put on the clipboard) as rows of cells. A cell may be
/// quoted (`"a<TAB>b"`, `""` for a quote) to hold tabs or line breaks. The last line break is
/// not a row.
pub fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = vec![];
    let mut row: Vec<String> = vec![];
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut at_start = true;
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start => {
                quoted = true;
                at_start = false;
            }
            '\t' => {
                row.push(std::mem::take(&mut cell));
                at_start = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                at_start = true;
            }
            _ => {
                cell.push(c);
                at_start = false;
            }
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// One cell for tab-separated text, quoted when it holds a tab, a line break or a quote.
fn tsv_cell(s: &str) -> String {
    if s.contains(['\t', '\n', '\r', '"']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// The role names of stock chart series, by number of series: high-low-close, open-high-low-close
/// and volume-open-high-low-close, the order the series of a stock chart come in.
pub fn stock_roles(series: usize) -> &'static [&'static str] {
    match series {
        3 => &["High", "Low", "Close"],
        4 => &["Open", "High", "Low", "Close"],
        5 => &["Volume", "Open", "High", "Low", "Close"],
        _ => &[],
    }
}

/// Roles of the series of a stock chart of type `kind`: the variant fixes them (`StockVhlc` is
/// Volume, High, Low, Close); the plain stock chart goes by the number of series.
pub fn stock_roles_for(kind: ChartType, series: usize) -> &'static [&'static str] {
    match (kind.has_volume(), kind.has_open()) {
        (true, true) => &["Volume", "Open", "High", "Low", "Close"],
        (true, false) => &["Volume", "High", "Low", "Close"],
        (false, true) => &["Open", "High", "Low", "Close"],
        (false, false) => stock_roles(series),
    }
}

/// Point indices of per-point options (`point_fills`, point explosions) after the points were
/// moved: `map` gives the new index of an old one (`None` when the point went).
fn remap_points<T>(fills: &mut Vec<(usize, T)>, map: impl Fn(usize) -> Option<usize>) {
    let old = std::mem::take(fills);
    *fills = old.into_iter().filter_map(|(i, f)| map(i).map(|j| (j, f))).collect();
}

/// [`remap_points`] on everything a series keeps per point.
fn remap_series_points(s: &mut crate::chart::Series, map: impl Fn(usize) -> Option<usize>) {
    remap_points(&mut s.point_fills, &map);
    remap_points(&mut s.format.point_explosion, &map);
}

/// `v` with `count` items `fill` inserted at `at` (when `v` reaches that far).
fn insert_n<T: Clone>(v: &mut Vec<T>, at: usize, count: usize, fill: T) {
    if at <= v.len() {
        v.splice(at..at, std::iter::repeat_n(fill, count));
    }
}

fn remove_n<T>(v: &mut Vec<T>, at: usize, count: usize) {
    if at < v.len() {
        let end = at.saturating_add(count).min(v.len());
        v.drain(at..end);
    }
}

fn move_item<T>(v: &mut Vec<T>, from: usize, to: usize) {
    if from < v.len() && to < v.len() && from != to {
        let x = v.remove(from);
        v.insert(to, x);
    }
}

/// Where item `i` goes when the item at `from` moves to `to`.
fn moved(i: usize, from: usize, to: usize) -> usize {
    if i == from {
        to
    } else if from < to && i > from && i <= to {
        i - 1
    } else if to < from && i >= to && i < from {
        i + 1
    } else {
        i
    }
}

impl Chart {
    /// Number of points (rows of the data table): the longest of the series, and of the
    /// categories for a chart that plots them (scatter and bubble charts plot X values).
    pub fn point_count(&self) -> usize {
        let mut n = if self.kind.data_shape() == DataShape::Categories { self.categories.len() } else { 0 };
        for s in &self.series {
            n = n.max(s.values.len()).max(s.x_values.len()).max(s.sizes.len());
        }
        n.min(MAX_POINTS)
    }

    /// The X values of series `s` as plotted, one per point: its own, or 1, 2, 3… when it has none.
    pub fn x_values_of(&self, s: usize) -> Vec<Option<f64>> {
        let n = self.point_count();
        match self.series.get(s) {
            Some(ser) if !ser.x_values.is_empty() => (0..n).map(|i| ser.x_values.get(i).copied().flatten()).collect(),
            _ => (1..=n).map(|i| Some(i as f64)).collect(),
        }
    }

    /// Do all series have the same X values (so the table can show them in one column)?
    pub fn shares_x(&self) -> bool {
        let first = self.x_values_of(0);
        (1..self.series.len()).all(|s| self.x_values_of(s) == first)
    }

    /// The columns of the data table. `shared_x` asks for one X column for all series (scatter and
    /// bubble charts), which only applies when they do share their X values.
    pub fn data_columns(&self, shared_x: bool) -> Vec<DataColumn> {
        let n = self.series.len();
        let shape = self.kind.data_shape();
        if shape != DataShape::Categories && (n == 0 || (shared_x && self.shares_x())) {
            let mut cols = vec![DataColumn::X { series: None }];
            for s in 0..n {
                cols.push(DataColumn::Values { series: s });
                if shape == DataShape::Bubble {
                    cols.push(DataColumn::Sizes { series: s });
                }
            }
            return cols;
        }
        match shape {
            DataShape::Categories => std::iter::once(DataColumn::Categories).chain((0..n).map(|s| DataColumn::Values { series: s })).collect(),
            DataShape::Xy => (0..n).flat_map(|s| [DataColumn::X { series: Some(s) }, DataColumn::Values { series: s }]).collect(),
            DataShape::Bubble => {
                (0..n).flat_map(|s| [DataColumn::X { series: Some(s) }, DataColumn::Values { series: s }, DataColumn::Sizes { series: s }]).collect()
            }
        }
    }

    /// What a column holds, for the line above the table: `Categories`, `X values`, `Y values`,
    /// `Size`, or the role of a stock chart series (`Open`, `High`…). `None` for plain values.
    pub fn column_role(&self, col: DataColumn) -> Option<String> {
        match col {
            DataColumn::Categories => Some("Categories".into()),
            DataColumn::X { .. } => Some("X values".into()),
            DataColumn::Sizes { .. } => Some("Size".into()),
            DataColumn::Values { series } => match self.kind {
                k if k.is_xy() => Some("Y values".into()),
                k if k.is_stock() => stock_roles_for(k, self.series.len()).get(series).map(|r| r.to_string()),
                ChartType::Combo => self.series.get(series).and_then(|s| s.kind).map(|k| k.label().to_string()),
                _ => None,
            },
        }
    }

    /// The text of a cell: row 0 is the header row (series names), rows 1… are the points.
    pub fn cell_text(&self, row: usize, col: DataColumn) -> String {
        if row == 0 {
            return match col {
                DataColumn::Values { series } => self.series.get(series).map(|s| s.name.clone()).unwrap_or_default(),
                DataColumn::Categories => String::new(),
                DataColumn::X { series } => match series.and_then(|s| self.series.get(s)) {
                    Some(s) => format!("{} X", s.name),
                    None => "X values".into(),
                },
                DataColumn::Sizes { series } => self.series.get(series).map(|s| format!("{} size", s.name)).unwrap_or_default(),
            };
        }
        let i = row - 1;
        match col {
            DataColumn::Categories => self.categories.get(i).cloned().unwrap_or_default(),
            DataColumn::X { series } => fmt_cell(self.x_values_of(series.unwrap_or(0)).get(i).copied().flatten()),
            DataColumn::Values { series } => fmt_cell(self.series.get(series).and_then(|s| s.values.get(i)).copied().flatten()),
            DataColumn::Sizes { series } => fmt_cell(self.series.get(series).and_then(|s| s.sizes.get(i)).copied().flatten()),
        }
    }

    /// Can the user type in this cell? Header cells other than series names are labels.
    pub fn cell_editable(row: usize, col: DataColumn) -> bool {
        row > 0 || matches!(col, DataColumn::Values { .. })
    }

    /// The whole table as text, header row first.
    pub fn data_grid(&self, shared_x: bool) -> Vec<Vec<String>> {
        let cols = self.data_columns(shared_x);
        (0..=self.point_count()).map(|r| cols.iter().map(|c| self.cell_text(r, *c)).collect()).collect()
    }

    /// The table as tab-separated text, ready for a spreadsheet.
    pub fn data_tsv(&self, shared_x: bool) -> String {
        let mut out = String::new();
        for row in self.data_grid(shared_x) {
            out.push_str(&row.iter().map(|c| tsv_cell(c)).collect::<Vec<_>>().join("\t"));
            out.push('\n');
        }
        out
    }

    /// Make every series as long as the table (gaps are `None`), and the categories too for a
    /// chart that plots them. X values and sizes a series doesn't have stay empty.
    pub fn fit_data(&mut self) {
        let n = self.point_count();
        if self.kind.data_shape() == DataShape::Categories || !self.categories.is_empty() {
            self.categories.resize(n, String::new());
        }
        for s in &mut self.series {
            s.values.resize(n, None);
            if !s.x_values.is_empty() {
                s.x_values.resize(n, None);
            }
            if !s.sizes.is_empty() || self.kind == ChartType::Bubble {
                s.sizes.resize(n, None);
            }
        }
    }

    /// Give series `s` X values of its own (1, 2, 3… when it had none) before one of them changes.
    fn own_x(&mut self, s: usize) {
        let xs = self.x_values_of(s);
        if let Some(ser) = self.series.get_mut(s) {
            ser.x_values = xs;
        }
    }

    fn grow_to(&mut self, points: usize) -> Result<(), String> {
        if points > MAX_POINTS {
            return Err(format!("a chart holds at most {MAX_POINTS} points"));
        }
        let n = self.point_count();
        if points > n {
            self.insert_points(n, points - n)?;
        }
        Ok(())
    }

    /// Set a cell from text. `row` 0 is the header row (only series names can change there); a row
    /// just past the last point adds a point. Numbers accept a decimal comma; an empty cell is a
    /// gap. With `lenient`, text that isn't a number makes a gap instead of an error (pasting).
    pub fn set_cell(&mut self, row: usize, col: usize, shared_x: bool, text: &str, lenient: bool) -> Result<(), String> {
        let cols = self.data_columns(shared_x);
        let column = *cols.get(col).ok_or_else(|| format!("no column {col}"))?;
        if row == 0 {
            return match column {
                DataColumn::Values { series } => {
                    let s = self.series.get_mut(series).ok_or("no such series")?;
                    s.name = text.trim().to_string();
                    Ok(())
                }
                _ if lenient => Ok(()),
                _ => Err("this header is a label: type series names above their values".into()),
            };
        }
        let i = row - 1;
        let number = || match parse_number(text) {
            Ok(v) => Ok(v),
            Err(_) if lenient => Ok(None),
            Err(e) => Err(e),
        };
        let v = if column == DataColumn::Categories { None } else { number()? };
        self.grow_to(row)?;
        match column {
            DataColumn::Categories => {
                if let Some(c) = self.categories.get_mut(i) {
                    *c = text.trim().to_string();
                }
            }
            DataColumn::X { series: None } => {
                for s in 0..self.series.len() {
                    self.own_x(s);
                    if let Some(x) = self.series.get_mut(s).and_then(|ser| ser.x_values.get_mut(i)) {
                        *x = v;
                    }
                }
            }
            DataColumn::X { series: Some(s) } => {
                self.own_x(s);
                if let Some(x) = self.series.get_mut(s).and_then(|ser| ser.x_values.get_mut(i)) {
                    *x = v;
                }
            }
            DataColumn::Values { series } => {
                if let Some(y) = self.series.get_mut(series).and_then(|s| s.values.get_mut(i)) {
                    *y = v;
                }
            }
            DataColumn::Sizes { series } => {
                let n = self.point_count();
                if let Some(ser) = self.series.get_mut(series) {
                    ser.sizes.resize(n, None);
                    if let Some(z) = ser.sizes.get_mut(i) {
                        *z = v;
                    }
                }
            }
        }
        self.fit_data();
        Ok(())
    }

    /// Paste tab-separated text with its top-left cell at (`row`, `col`). Series and points are
    /// added when the block reaches past the table; text that isn't a number leaves a gap.
    /// Returns the size of the block (rows, columns).
    pub fn paste_tsv(&mut self, row: usize, col: usize, shared_x: bool, text: &str) -> Result<(usize, usize), String> {
        let block = parse_tsv(text);
        let height = block.len();
        let width = block.iter().map(Vec::len).max().unwrap_or(0);
        if height == 0 || width == 0 {
            return Ok((0, 0));
        }
        if row.saturating_add(height) > MAX_POINTS + 1 {
            return Err(format!("a chart holds at most {MAX_POINTS} points"));
        }
        let shared = shared_x && self.shares_x();
        let need = col.saturating_add(width);
        while self.data_columns(shared).len() < need {
            if self.series.len() >= MAX_SERIES {
                return Err(format!("a chart holds at most {MAX_SERIES} series"));
            }
            self.insert_series(self.series.len(), None)?;
        }
        self.grow_to((row + height).saturating_sub(1))?;
        for (r, cells) in block.iter().enumerate() {
            for (c, text) in cells.iter().enumerate() {
                self.set_cell(row + r, col + c, shared, text, true)?;
            }
        }
        Ok((height, width))
    }

    /// Insert `count` points before point `at` (at the end when `at` is past it). New categories
    /// are named `Category 5`…; new values are gaps.
    pub fn insert_points(&mut self, at: usize, count: usize) -> Result<(), String> {
        let n = self.point_count();
        if n.saturating_add(count) > MAX_POINTS {
            return Err(format!("a chart holds at most {MAX_POINTS} points"));
        }
        let at = at.min(n);
        self.fit_data();
        let named = self.kind.data_shape() == DataShape::Categories;
        if named || !self.categories.is_empty() {
            let names: Vec<String> = (0..count).map(|k| if named { format!("Category {}", at + k + 1) } else { String::new() }).collect();
            if at <= self.categories.len() {
                self.categories.splice(at..at, names);
            }
        }
        for s in &mut self.series {
            insert_n(&mut s.values, at, count, None);
            insert_n(&mut s.x_values, at, count, None);
            insert_n(&mut s.sizes, at, count, None);
            remap_series_points(s, |i| Some(if i >= at { i + count } else { i }));
        }
        self.fit_data();
        Ok(())
    }

    /// Remove `count` points from point `at`.
    pub fn remove_points(&mut self, at: usize, count: usize) -> Result<(), String> {
        let n = self.point_count();
        if at >= n || count == 0 {
            return Err(format!("no point {at}"));
        }
        self.fit_data();
        let end = at.saturating_add(count).min(n);
        remove_n(&mut self.categories, at, end - at);
        for s in &mut self.series {
            remove_n(&mut s.values, at, end - at);
            remove_n(&mut s.x_values, at, end - at);
            remove_n(&mut s.sizes, at, end - at);
            remap_series_points(s, |i| {
                if i < at {
                    Some(i)
                } else if i >= end {
                    Some(i - (end - at))
                } else {
                    None
                }
            });
        }
        Ok(())
    }

    /// Move point `from` to position `to`, with its category and values.
    pub fn move_point(&mut self, from: usize, to: usize) -> Result<(), String> {
        let n = self.point_count();
        if from >= n || to >= n {
            return Err(format!("no point {}", from.max(to)));
        }
        self.fit_data();
        move_item(&mut self.categories, from, to);
        for s in &mut self.series {
            move_item(&mut s.values, from, to);
            move_item(&mut s.x_values, from, to);
            move_item(&mut s.sizes, from, to);
            remap_series_points(s, |i| Some(moved(i, from, to)));
        }
        Ok(())
    }

    /// Insert a series before series `at` (at the end when `at` is past it), named `name` or
    /// `Series N`. Its values are gaps; in a scatter or bubble chart whose series share their X
    /// values it gets the same ones.
    pub fn insert_series(&mut self, at: usize, name: Option<String>) -> Result<usize, String> {
        if self.series.len() >= MAX_SERIES {
            return Err(format!("a chart holds at most {MAX_SERIES} series"));
        }
        self.fit_data();
        let n = self.point_count();
        let at = at.min(self.series.len());
        let name = name.unwrap_or_else(|| {
            let mut k = self.series.len() + 1;
            while self.series.iter().any(|s| s.name == format!("Series {k}")) {
                k += 1;
            }
            format!("Series {k}")
        });
        let shape = self.kind.data_shape();
        let x_values = match self.series.first() {
            Some(first) if shape != DataShape::Categories && self.shares_x() => first.x_values.clone(),
            _ => vec![],
        };
        let kind = if self.kind == ChartType::Combo { self.series.last().and_then(|s| s.kind) } else { None };
        let s = Series {
            name,
            values: vec![None; n],
            x_values,
            sizes: if shape == DataShape::Bubble { vec![None; n] } else { vec![] },
            kind,
            ..Default::default()
        };
        self.series.insert(at, s);
        Ok(at)
    }

    /// Remove `count` series from series `at`.
    pub fn remove_series(&mut self, at: usize, count: usize) -> Result<(), String> {
        if at >= self.series.len() || count == 0 {
            return Err(format!("no series {at}"));
        }
        remove_n(&mut self.series, at, count);
        Ok(())
    }

    /// Move series `from` to position `to`, with its values and formatting.
    pub fn move_series(&mut self, from: usize, to: usize) -> Result<(), String> {
        if from >= self.series.len() || to >= self.series.len() {
            return Err(format!("no series {}", from.max(to)));
        }
        move_item(&mut self.series, from, to);
        Ok(())
    }

    /// Switch rows and columns: categories become series and series categories. Series keep
    /// their formatting by position. Only for charts that plot categories.
    pub fn switch_rows_columns(&mut self) -> Result<(), String> {
        if self.kind.data_shape() != DataShape::Categories {
            return Err(format!("a {} chart has no categories to switch with its series", self.kind.label()));
        }
        self.fit_data();
        let n = self.point_count();
        if n > MAX_SERIES {
            return Err(format!("a chart holds at most {MAX_SERIES} series"));
        }
        let old = std::mem::take(&mut self.series);
        let categories = std::mem::take(&mut self.categories);
        self.categories = old.iter().map(|s| s.name.clone()).collect();
        self.series = (0..n)
            .map(|i| {
                let mut s = old.get(i).cloned().unwrap_or_default();
                s.name = categories.get(i).cloned().unwrap_or_default();
                s.values = old.iter().map(|o| o.values.get(i).copied().flatten()).collect();
                s.x_values.clear();
                s.sizes.clear();
                s.point_fills.clear();
                s.format.point_explosion.clear();
                s
            })
            .collect();
        self.fit_data();
        Ok(())
    }

    /// What the data can't show as it stands, in words for the data editor.
    pub fn data_notes(&self) -> Vec<String> {
        let mut notes = vec![];
        let n = self.series.len();
        if n == 0 {
            notes.push("Add a series to plot data.".to_string());
        }
        if matches!(self.kind, ChartType::Pie | ChartType::ExplodedPie | ChartType::PieOfPie | ChartType::BarOfPie) && n > 1 {
            notes.push("A pie chart plots the first series only.".to_string());
        }
        if self.kind == ChartType::Stock && !(3..=5).contains(&n) {
            notes.push("A stock chart plots 3 series (High, Low, Close), 4 (Open first) or 5 (Volume first).".to_string());
        }
        if self.kind.is_stock() && self.kind != ChartType::Stock {
            let roles = stock_roles_for(self.kind, n);
            if n != roles.len() {
                notes.push(format!("This stock chart plots {} series ({}).", roles.len(), roles.join(", ")));
            }
        }
        if self.kind == ChartType::Bubble && self.series.iter().any(|s| s.sizes.iter().flatten().any(|z| *z < 0.0)) {
            notes.push("Negative bubble sizes are not drawn.".to_string());
        }
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(c: &Chart, s: usize) -> Vec<Option<f64>> {
        c.series[s].values.clone()
    }

    #[test]
    fn numbers_in_any_usual_notation() {
        for (text, v) in [
            ("3", Some(3.0)),
            ("3.5", Some(3.5)),
            ("3,5", Some(3.5)),
            (" -2,25 ", Some(-2.25)),
            ("+4", Some(4.0)),
            ("1 234,5", Some(1234.5)),
            ("1\u{a0}234,5", Some(1234.5)),
            ("1,234.5", Some(1234.5)),
            ("1.234,5", Some(1234.5)),
            ("1,234,567", Some(1_234_567.0)),
            ("1.234.567", Some(1_234_567.0)),
            ("1'234", Some(1234.0)),
            ("12%", Some(0.12)),
            ("1e3", Some(1000.0)),
            ("", None),
            ("  ", None),
        ] {
            assert_eq!(parse_number(text), Ok(v), "{text}");
        }
        for bad in ["abc", "inf", "NaN", "0x10", "1e999", "-", ".", "1,2,3.4.5x"] {
            assert!(parse_number(bad).is_err(), "{bad}");
        }
        assert_eq!(format_number(4.0), "4");
        assert_eq!(format_number(4.25), "4.25");
        assert_eq!(format_number(f64::NAN), "");
    }

    #[test]
    fn tsv_from_spreadsheets() {
        assert_eq!(parse_tsv("a\tb\r\n1\t2\r\n"), vec![vec!["a", "b"], vec!["1", "2"]]);
        assert_eq!(parse_tsv("\"x\ty\"\t\"say \"\"hi\"\"\"\n\"two\nlines\"\t3"), vec![vec!["x\ty", "say \"hi\""], vec!["two\nlines", "3"]]);
        assert_eq!(parse_tsv("1\t\t3"), vec![vec!["1", "", "3"]]);
        assert!(parse_tsv("").is_empty());
        let c = Chart::sample(ChartType::Column);
        let tsv = c.data_tsv(true);
        assert!(tsv.starts_with("\tSeries 1\tSeries 2\tSeries 3\nQ1\t4.2\t2.6\t2.1\n"), "{tsv}");
    }

    #[test]
    fn columns_per_chart_type() {
        use DataColumn::*;
        let c = Chart::sample(ChartType::Column);
        assert_eq!(c.data_columns(true), vec![Categories, Values { series: 0 }, Values { series: 1 }, Values { series: 2 }]);
        let mut s = Chart::sample(ChartType::Scatter);
        s.series.truncate(2);
        assert_eq!(s.data_columns(true), vec![X { series: None }, Values { series: 0 }, Values { series: 1 }]);
        assert_eq!(s.data_columns(false), vec![X { series: Some(0) }, Values { series: 0 }, X { series: Some(1) }, Values { series: 1 }]);
        // Different X values: one X column per series whatever is asked.
        s.series[1].x_values[0] = Some(9.0);
        assert!(!s.shares_x());
        assert_eq!(s.data_columns(true).len(), 4);
        let mut b = Chart::sample(ChartType::Bubble);
        b.series.truncate(2);
        assert_eq!(
            b.data_columns(true),
            vec![X { series: None }, Values { series: 0 }, Sizes { series: 0 }, Values { series: 1 }, Sizes { series: 1 }]
        );
        assert_eq!(b.column_role(Sizes { series: 0 }).as_deref(), Some("Size"));
        let mut stock = Chart::sample(ChartType::Column);
        stock.kind = ChartType::Stock;
        stock.series.push(Series { name: "Close".into(), values: vec![Some(1.0); 4], ..Default::default() });
        assert_eq!(stock.column_role(Values { series: 0 }).as_deref(), Some("Open"));
        assert_eq!(stock.column_role(Values { series: 3 }).as_deref(), Some("Close"));
        assert!(stock.data_notes().is_empty());
        stock.series.truncate(2);
        assert_eq!(stock.data_notes().len(), 1);
        let pie = Chart::sample(ChartType::Pie);
        assert_eq!(pie.data_columns(true).len(), 2);
        assert_eq!(pie.data_grid(true)[1], vec!["Q1", "4.2"]);
    }

    #[test]
    fn cells_set_values_names_and_gaps() {
        let mut c = Chart::sample(ChartType::Line);
        c.set_cell(0, 2, true, "Costs", false).unwrap();
        c.set_cell(2, 1, true, "9,5", false).unwrap();
        c.set_cell(3, 3, true, "", false).unwrap();
        c.set_cell(1, 0, true, "Spring", false).unwrap();
        assert_eq!(c.series[1].name, "Costs");
        assert_eq!(values(&c, 0)[1], Some(9.5));
        assert_eq!(values(&c, 2)[2], None);
        assert_eq!(c.categories[0], "Spring");
        assert!(c.set_cell(1, 1, true, "lots", false).is_err());
        assert!(c.set_cell(0, 0, true, "x", false).is_err());
        assert!(c.set_cell(1, 9, true, "1", false).is_err());
        // The row past the last point adds one.
        c.set_cell(5, 2, true, "7", false).unwrap();
        assert_eq!(c.point_count(), 5);
        assert_eq!(c.categories[4], "Category 5");
        assert_eq!(values(&c, 1), vec![Some(2.6), Some(4.1), Some(1.9), Some(3.0), Some(7.0)]);
        assert_eq!(values(&c, 0)[4], None);
        assert!(c.set_cell(MAX_POINTS + 1, 1, true, "1", false).is_err());
    }

    #[test]
    fn scatter_x_values_shared_or_own() {
        let mut c = Chart::sample(ChartType::Scatter);
        // The shared X column changes every series.
        c.set_cell(1, 0, true, "0,5", false).unwrap();
        assert!(c.series.iter().all(|s| s.x_values[0] == Some(0.5)));
        assert!(c.shares_x());
        // One series' own X column changes only it.
        c.set_cell(2, 2, false, "8", false).unwrap();
        assert_eq!(c.series[1].x_values[1], Some(8.0));
        assert_eq!(c.series[0].x_values[1], Some(2.0));
        assert!(!c.shares_x());
        // A series without X values plots 1, 2, 3…; setting one keeps the others.
        let mut c = Chart::sample(ChartType::Line);
        c.kind = ChartType::Scatter;
        c.set_cell(3, 0, true, "10", false).unwrap();
        assert_eq!(c.series[2].x_values, vec![Some(1.0), Some(2.0), Some(10.0), Some(4.0)]);
        // A new series shares the X values.
        c.insert_series(3, None).unwrap();
        assert_eq!(c.series[3].x_values, c.series[0].x_values);
        assert_eq!(c.series[3].name, "Series 4");
        assert!(c.shares_x());
    }

    #[test]
    fn bubble_sizes() {
        let mut c = Chart::sample(ChartType::Bubble);
        c.series.truncate(1);
        c.set_cell(1, 2, true, "3", false).unwrap();
        assert_eq!(c.series[0].sizes.first().copied().flatten(), Some(3.0));
        c.set_cell(2, 2, true, "-1", false).unwrap();
        assert_eq!(c.data_notes().len(), 1);
        c.insert_series(1, Some("B".into())).unwrap();
        assert_eq!(c.series[1].sizes.len(), 4);
        assert_eq!(c.data_columns(true).len(), 5);
    }

    #[test]
    fn rows_inserted_removed_and_moved() {
        let mut c = Chart::sample(ChartType::Column);
        c.series[0].point_fills = vec![(1, crate::Fill::None), (3, crate::Fill::None)];
        c.insert_points(1, 2).unwrap();
        assert_eq!(c.categories, vec!["Q1", "Category 2", "Category 3", "Q2", "Q3", "Q4"]);
        assert_eq!(values(&c, 0), vec![Some(4.2), None, None, Some(2.8), Some(3.6), Some(4.7)]);
        assert_eq!(c.series[0].point_fills.iter().map(|p| p.0).collect::<Vec<_>>(), vec![3, 5]);
        c.remove_points(1, 2).unwrap();
        assert_eq!(c, {
            let mut s = Chart::sample(ChartType::Column);
            s.series[0].point_fills = vec![(1, crate::Fill::None), (3, crate::Fill::None)];
            s
        });
        c.remove_points(1, 1).unwrap();
        assert_eq!(c.categories, vec!["Q1", "Q3", "Q4"]);
        assert_eq!(c.series[0].point_fills.iter().map(|p| p.0).collect::<Vec<_>>(), vec![2]);
        c.move_point(0, 2).unwrap();
        assert_eq!(c.categories, vec!["Q3", "Q4", "Q1"]);
        assert_eq!(values(&c, 2), vec![Some(3.3), Some(5.1), Some(2.1)]);
        assert_eq!(c.series[0].point_fills.iter().map(|p| p.0).collect::<Vec<_>>(), vec![1]);
        assert!(c.move_point(0, 3).is_err());
        assert!(c.remove_points(3, 1).is_err());
        // Appended at the end.
        c.insert_points(99, 1).unwrap();
        assert_eq!(c.categories.last().map(String::as_str), Some("Category 4"));
    }

    #[test]
    fn series_inserted_removed_and_moved() {
        let mut c = Chart::sample(ChartType::Column);
        c.series[2].fill = Some(crate::Fill::None);
        c.insert_series(1, None).unwrap();
        assert_eq!(c.series.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Series 1", "Series 4", "Series 2", "Series 3"]);
        assert_eq!(values(&c, 1), vec![None; 4]);
        c.move_series(3, 0).unwrap();
        assert_eq!(c.series[0].name, "Series 3");
        assert_eq!(c.series[0].fill, Some(crate::Fill::None));
        c.remove_series(1, 2).unwrap();
        assert_eq!(c.series.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Series 3", "Series 2"]);
        assert!(c.remove_series(2, 1).is_err());
        assert!(c.move_series(0, 2).is_err());
    }

    #[test]
    fn rows_and_columns_switch() {
        let mut c = Chart::sample(ChartType::Column);
        c.switch_rows_columns().unwrap();
        assert_eq!(c.categories, vec!["Series 1", "Series 2", "Series 3"]);
        assert_eq!(c.series.len(), 4);
        assert_eq!(c.series[3].name, "Q4");
        assert_eq!(values(&c, 3), vec![Some(4.7), Some(3.0), Some(5.1)]);
        c.switch_rows_columns().unwrap();
        assert_eq!(c.categories, Chart::sample(ChartType::Column).categories);
        assert_eq!(c.series.len(), 3);
        assert_eq!(values(&c, 1), values(&Chart::sample(ChartType::Column), 1));
        assert!(Chart::sample(ChartType::Scatter).switch_rows_columns().is_err());
    }

    #[test]
    fn pasted_blocks_grow_the_table() {
        let mut c = Chart::sample(ChartType::Column);
        // From the corner: names, categories and values, one more series and point than before.
        let (h, w) = c
            .paste_tsv(
                0,
                0,
                true,
                "\tNorth\tSouth\tEast\tWest\nJan\t1,5\t2\t3\t4\nFeb\t5\tn/a\t7\t8\nMar\t\t\t\t\nApr\t1\t1\t1\t1\nMay\t2\t2\t2\t2\n",
            )
            .unwrap();
        assert_eq!((h, w), (6, 5));
        assert_eq!(c.series.len(), 4);
        assert_eq!(c.series[3].name, "West");
        assert_eq!(c.categories, vec!["Jan", "Feb", "Mar", "Apr", "May"]);
        assert_eq!(values(&c, 0), vec![Some(1.5), Some(5.0), None, Some(1.0), Some(2.0)]);
        assert_eq!(values(&c, 1)[1], None);
        // A block inside the table changes only its cells.
        c.paste_tsv(2, 2, true, "10\t11\r\n12\t13").unwrap();
        assert_eq!(values(&c, 1)[1..3], [Some(10.0), Some(12.0)]);
        assert_eq!(values(&c, 2)[1..3], [Some(11.0), Some(13.0)]);
        assert_eq!(c.point_count(), 5);
        // Scatter: the shared X column then Y columns.
        let mut s = Chart::sample(ChartType::Scatter);
        s.series.truncate(1);
        s.paste_tsv(1, 0, true, "0.5\t1\n1.5\t2\n2.5\t3\n3.5\t4\n4.5\t5\n").unwrap();
        assert_eq!(s.series[0].x_values, vec![Some(0.5), Some(1.5), Some(2.5), Some(3.5), Some(4.5)]);
        assert_eq!(values(&s, 0), vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0), Some(5.0)]);
        assert_eq!(c.paste_tsv(0, 0, true, "").unwrap(), (0, 0));
    }
}
