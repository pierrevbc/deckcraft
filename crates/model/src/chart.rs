//! Charts: an embedded data grid plus type and element options.

use serde::{Deserialize, Serialize};

use crate::MediaId;
use crate::style::{ColorRef, Fill, Line};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartType {
    #[default]
    Column,
    StackedColumn,
    PercentColumn,
    Bar,
    StackedBar,
    PercentBar,
    Line,
    LineMarkers,
    StackedLine,
    Area,
    StackedArea,
    Pie,
    Doughnut,
    Scatter,
    Bubble,
    Radar,
    FilledRadar,
    Stock,
    Surface,
    Histogram,
    Pareto,
    BoxWhisker,
    Waterfall,
    Funnel,
    Treemap,
    Sunburst,
    Combo,
}

impl ChartType {
    /// The first five primary chart types: Clustered Column, Clustered Bar, Line, Area, Pie.
    pub const FIRST_FIVE: [ChartType; 5] = [
        ChartType::Column,
        ChartType::Bar,
        ChartType::Line,
        ChartType::Area,
        ChartType::Pie,
    ];
    pub const MAIN: [ChartType; 12] = [
        ChartType::Column,
        ChartType::Bar,
        ChartType::Line,
        ChartType::Area,
        ChartType::Pie,
        ChartType::Doughnut,
        ChartType::Scatter,
        ChartType::Radar,
        ChartType::Waterfall,
        ChartType::Funnel,
        ChartType::Histogram,
        ChartType::Treemap,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ChartType::Column => "Clustered Column",
            ChartType::StackedColumn => "Stacked Column",
            ChartType::PercentColumn => "100% Stacked Column",
            ChartType::Bar => "Clustered Bar",
            ChartType::StackedBar => "Stacked Bar",
            ChartType::PercentBar => "100% Stacked Bar",
            ChartType::Line => "Line",
            ChartType::LineMarkers => "Line with Markers",
            ChartType::StackedLine => "Stacked Line",
            ChartType::Area => "Area",
            ChartType::StackedArea => "Stacked Area",
            ChartType::Pie => "Pie",
            ChartType::Doughnut => "Doughnut",
            ChartType::Scatter => "Scatter",
            ChartType::Bubble => "Bubble",
            ChartType::Radar => "Radar",
            ChartType::FilledRadar => "Filled Radar",
            ChartType::Stock => "Stock",
            ChartType::Surface => "Surface",
            ChartType::Histogram => "Histogram",
            ChartType::Pareto => "Pareto",
            ChartType::BoxWhisker => "Box and Whisker",
            ChartType::Waterfall => "Waterfall",
            ChartType::Funnel => "Funnel",
            ChartType::Treemap => "Treemap",
            ChartType::Sunburst => "Sunburst",
            ChartType::Combo => "Combo",
        }
    }
    pub fn is_bar_like(self) -> bool {
        matches!(
            self,
            ChartType::Column
                | ChartType::StackedColumn
                | ChartType::PercentColumn
                | ChartType::Bar
                | ChartType::StackedBar
                | ChartType::PercentBar
                | ChartType::Histogram
                | ChartType::Pareto
                | ChartType::Waterfall
        )
    }
    pub fn horizontal(self) -> bool {
        matches!(self, ChartType::Bar | ChartType::StackedBar | ChartType::PercentBar | ChartType::Funnel)
    }
    pub fn stacked(self) -> bool {
        matches!(
            self,
            ChartType::StackedColumn
                | ChartType::StackedBar
                | ChartType::StackedLine
                | ChartType::StackedArea
                | ChartType::PercentColumn
                | ChartType::PercentBar
        )
    }
    pub fn percent(self) -> bool {
        matches!(self, ChartType::PercentColumn | ChartType::PercentBar)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Series {
    pub name: String,
    pub values: Vec<Option<f64>>,
    /// X values for scatter/bubble.
    pub x_values: Vec<Option<f64>>,
    pub sizes: Vec<Option<f64>>,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    /// Per-point fill overrides (pie slices).
    pub point_fills: Vec<(usize, Fill)>,
    pub data_labels: Option<bool>,
    pub smooth: bool,
    pub marker: Option<String>,
    /// Combo charts: this series' own type.
    pub kind: Option<ChartType>,
    pub secondary_axis: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Chart {
    pub kind: ChartType,
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    pub legend: Option<String>,
    pub data_labels: bool,
    pub gridlines: bool,
    pub axis_titles: (Option<String>, Option<String>),
    /// Chart style number (1–48) and colour palette (`colorful1`, `monochrome3`…).
    pub style: u32,
    pub palette: String,
    pub plot_fill: Option<Fill>,
    pub chart_fill: Option<Fill>,
    pub text_color: Option<ColorRef>,
    pub hole_size: f64,
    pub gap_width: f64,
    pub overlap: f64,
    pub vary_colors: bool,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    /// Original chart XML part kept for round-trip of unmodelled options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// The spreadsheet that holds the chart data in a PPTX (embedded workbook, or a link to one),
    /// kept so PowerPoint's Edit Data still works after a round trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workbook: Option<MediaId>,
    /// Where the data sits in that workbook, and the data as read from there. Unlike `raw`, edits
    /// keep it: an export compares the chart data with it to keep the workbook, or to update only
    /// the cells that changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ChartSource>,
}

/// The data of an imported chart as its chart part cached it, with the workbook formulas each
/// series reads it from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChartSource {
    /// The series read X values (scatter and bubble charts) rather than categories.
    pub xy: bool,
    pub categories: Vec<String>,
    pub series: Vec<SeriesSource>,
}

/// One series of a [`ChartSource`]: its data as read, and its formulas (`Sheet1!$B$2:$B$5`).
/// A formula is `None` when the chart part held the data as a literal, or had none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SeriesSource {
    pub name: String,
    pub values: Vec<Option<f64>>,
    pub x_values: Vec<Option<f64>>,
    pub sizes: Vec<Option<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_ref: Option<String>,
    /// Categories, or X values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizes_ref: Option<String>,
}

impl Default for Chart {
    fn default() -> Self {
        Chart {
            kind: ChartType::Column,
            title: Some("Chart Title".into()),
            categories: vec![],
            series: vec![],
            legend: Some("b".into()),
            data_labels: false,
            gridlines: true,
            axis_titles: (None, None),
            style: 2,
            palette: "colorful1".into(),
            plot_fill: None,
            chart_fill: None,
            text_color: None,
            hole_size: 0.5,
            gap_width: 2.19,
            overlap: -0.27,
            vary_colors: false,
            y_min: None,
            y_max: None,
            raw: None,
            workbook: None,
            source: None,
        }
    }
}

impl Chart {
    /// The sample data a new chart starts with (our own numbers).
    pub fn sample(kind: ChartType) -> Self {
        let cats = ["Q1", "Q2", "Q3", "Q4"];
        let data = [[4.2, 2.6, 2.1], [2.8, 4.1, 2.4], [3.6, 1.9, 3.3], [4.7, 3.0, 5.1]];
        let mut c = Chart { kind, categories: cats.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        let n_series =
            if matches!(kind, ChartType::Pie | ChartType::Doughnut | ChartType::Funnel | ChartType::Waterfall | ChartType::Treemap) { 1 } else { 3 };
        for s in 0..n_series {
            c.series.push(Series {
                name: format!("Series {}", s + 1),
                values: data.iter().map(|row| row.get(s).copied()).collect(),
                x_values: if kind == ChartType::Scatter { (1..=4).map(|x| Some(x as f64)).collect() } else { vec![] },
                ..Default::default()
            });
        }
        c.vary_colors = n_series == 1;
        c
    }
    /// Value range across series (stacked sums for stacked types).
    pub fn value_range(&self) -> (f64, f64) {
        let mut lo: f64 = 0.0;
        let mut hi: f64 = 0.0;
        let n = self.categories.len().max(self.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
        if self.kind.stacked() {
            for i in 0..n {
                let (mut p, mut m) = (0.0, 0.0);
                for s in &self.series {
                    let v = s.values.get(i).copied().flatten().unwrap_or(0.0);
                    if v >= 0.0 { p += v } else { m += v }
                }
                hi = hi.max(p);
                lo = lo.min(m);
            }
        } else {
            for s in &self.series {
                for v in s.values.iter().flatten() {
                    if v.is_finite() {
                        hi = hi.max(*v);
                        lo = lo.min(*v);
                    }
                }
            }
        }
        if self.kind.percent() {
            return (0.0, 1.0);
        }
        (self.y_min.unwrap_or(lo), self.y_max.unwrap_or(hi))
    }

    /// Exports the chart's categories, series names, and values to CSV for GridCraft spreadsheet exchange.
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        out.push_str("Category");
        for s in &self.series {
            out.push(',');
            out.push_str(&csv_escape(&s.name));
        }
        out.push('\n');
        let n = self.categories.len().max(self.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
        for i in 0..n {
            let cat = self.categories.get(i).map(String::as_str).unwrap_or("");
            out.push_str(&csv_escape(cat));
            for s in &self.series {
                out.push(',');
                if let Some(Some(v)) = s.values.get(i) {
                    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
                        out.push_str(&format!("{}", v.round() as i64));
                    } else {
                        out.push_str(&format!("{v}"));
                    }
                }
            }
            out.push('\n');
        }
        out
    }

    /// Loads categories, series names, and numerical values from CSV text (GridCraft exchange).
    pub fn load_csv(&mut self, text: &str) -> Result<(), String> {
        let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let Some(first) = lines.first() else {
            return Err("empty CSV".into());
        };
        let header = parse_csv_line(first);
        let Some(series_names) = header.get(1..) else {
            return Err("CSV must have at least Category and one Series column".into());
        };
        if series_names.is_empty() {
            return Err("CSV must have at least one Series column".into());
        }
        let mut cats = Vec::new();
        let mut series_vals: Vec<Vec<Option<f64>>> = vec![Vec::new(); series_names.len()];
        for line in lines.iter().skip(1) {
            let row = parse_csv_line(line);
            cats.push(row.first().cloned().unwrap_or_default());
            for (si, slot) in series_vals.iter_mut().enumerate() {
                let val = row.get(si + 1).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
                slot.push(val);
            }
        }
        let old = std::mem::take(&mut self.series);
        self.categories = cats;
        self.series = series_names
            .iter()
            .zip(series_vals)
            .enumerate()
            .map(|(i, (name, vals))| {
                let mut s = old.get(i).cloned().unwrap_or_default();
                s.name = name.clone();
                s.values = vals;
                s
            })
            .collect();
        self.raw = None;
        Ok(())
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    let _ = chars.next();
                    cur.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// "Nice" axis scale: (min, max, major step) covering [lo, hi].
pub fn nice_scale(lo: f64, hi: f64) -> (f64, f64, f64) {
    let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi), lo.max(hi)) } else { (0.0, 1.0) };
    let span = if hi - lo < 1e-12 { hi.abs().max(1.0) } else { hi - lo };
    let raw = span / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;
    let min = (lo / step).floor() * step;
    let max = (hi / step).ceil() * step;
    let max = if (max - min).abs() < 1e-12 { min + step } else { max };
    (min, max, step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_scale_covers() {
        let (a, b, s) = nice_scale(0.0, 5.1);
        assert!(a <= 0.0 && b >= 5.1 && s > 0.0);
        let (a, b, _) = nice_scale(f64::NAN, 3.0);
        assert!(a.is_finite() && b.is_finite());
        let (a, b, _) = nice_scale(7.0, 7.0);
        assert!(b > a);
    }

    #[test]
    fn sample_ranges() {
        let c = Chart::sample(ChartType::StackedColumn);
        let (_, hi) = c.value_range();
        assert!(hi > 10.0);
        assert_eq!(Chart::sample(ChartType::Pie).series.len(), 1);
    }

    #[test]
    fn first_five_types_and_csv_roundtrip() {
        assert_eq!(
            ChartType::FIRST_FIVE.map(|k| k.label()),
            ["Clustered Column", "Clustered Bar", "Line", "Area", "Pie"]
        );
        for kind in ChartType::FIRST_FIVE {
            let orig = Chart::sample(kind);
            let csv = orig.to_csv();
            let mut loaded = Chart::sample(kind);
            loaded.categories.clear();
            loaded.series.clear();
            loaded.load_csv(&csv).unwrap();
            assert_eq!(loaded.categories, orig.categories);
            assert_eq!(loaded.series.len(), orig.series.len());
            for (a, b) in loaded.series.iter().zip(orig.series.iter()) {
                assert_eq!(a.name, b.name);
                assert_eq!(a.values, b.values);
            }
        }
    }
}
