//! Chart formatting: fonts of the text elements, element boxes, axes, gridlines, data labels and
//! the look of series and points. Every field defaults to "as the chart style draws it", so a
//! chart without formatting looks like a new one.

use serde::{Deserialize, Serialize};

use crate::style::{ColorRef, Fill, Line};

/// Font of a chart text element (title, axis labels, legend, data labels). Unset fields follow
/// the chart's text ([`ChartFormat::text`]), then the chart style.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChartText {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// Size in points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorRef>,
}

impl ChartText {
    pub fn is_empty(&self) -> bool {
        *self == ChartText::default()
    }
    /// This text with unset fields taken from `base`.
    pub fn over(&self, base: &ChartText) -> ChartText {
        ChartText {
            font: self.font.clone().or_else(|| base.font.clone()),
            size: self.size.or(base.size),
            bold: self.bold.or(base.bold),
            italic: self.italic.or(base.italic),
            color: self.color.clone().or_else(|| base.color.clone()),
        }
    }
}

/// Tick marks on an axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TickMark {
    None,
    Inside,
    Outside,
    Cross,
}

impl TickMark {
    pub fn xml(self) -> &'static str {
        match self {
            TickMark::None => "none",
            TickMark::Inside => "in",
            TickMark::Outside => "out",
            TickMark::Cross => "cross",
        }
    }
    pub fn from_xml(s: &str) -> Option<Self> {
        Some(match s {
            "none" => TickMark::None,
            "in" | "inside" => TickMark::Inside,
            "out" | "outside" => TickMark::Outside,
            "cross" => TickMark::Cross,
            _ => return None,
        })
    }
}

/// Where the tick labels of an axis sit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TickLabelPos {
    NextTo,
    Low,
    High,
    None,
}

impl TickLabelPos {
    pub fn xml(self) -> &'static str {
        match self {
            TickLabelPos::NextTo => "nextTo",
            TickLabelPos::Low => "low",
            TickLabelPos::High => "high",
            TickLabelPos::None => "none",
        }
    }
    pub fn from_xml(s: &str) -> Option<Self> {
        Some(match s {
            "nextTo" => TickLabelPos::NextTo,
            "low" => TickLabelPos::Low,
            "high" => TickLabelPos::High,
            "none" => TickLabelPos::None,
            _ => return None,
        })
    }
}

/// Where an axis crosses the other one, on the other axis' scale: at zero (automatic), at its
/// minimum, its maximum, or at a value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Crosses {
    AutoZero,
    Min,
    Max,
    At { value: f64 },
}

/// One axis: scale options (value axes), tick marks and labels, line and gridlines.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AxisFormat {
    /// Hidden (`c:delete`): no line, ticks or labels. Gridlines stay. The category (or X) axis'
    /// switch: the value axes' are [`Chart::y_hidden`](crate::chart::Chart::y_hidden) and
    /// [`Chart::y2_hidden`](crate::chart::Chart::y2_hidden).
    pub hidden: bool,
    /// Bounds of the X axis of scatter and bubble charts (the value axis' bounds are
    /// [`Chart::y_min`](crate::chart::Chart::y_min) and [`Chart::y_max`](crate::chart::Chart::y_max)).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Values (or categories) in reverse order.
    pub reverse: bool,
    /// Logarithmic scale with this base (value axes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_base: Option<f64>,
    /// Major and minor units; automatic when unset. The major unit is the X axis' of scatter and
    /// bubble charts: the value axes' are [`Chart::y_major`](crate::chart::Chart::y_major) and
    /// [`Chart::y2_major`](crate::chart::Chart::y2_major).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major_unit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor_unit: Option<f64>,
    /// Number format of the X axis' labels of scatter and bubble charts (`0.0`, `#,##0`, `0%`,
    /// `$#,##0.00`…): the value axes' are [`Chart::y_format`](crate::chart::Chart::y_format) and
    /// [`Chart::y2_format`](crate::chart::Chart::y2_format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_fmt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major_tick: Option<TickMark>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor_tick: Option<TickMark>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_pos: Option<TickLabelPos>,
    /// Where this axis crosses the other axis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crosses: Option<Crosses>,
    /// Tick label font.
    #[serde(skip_serializing_if = "ChartText::is_empty")]
    pub text: ChartText,
    /// Tick label rotation in degrees (-90 to 90).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    /// The axis line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<Line>,
    /// Major gridlines across the plot from this axis. The value axis uses
    /// [`Chart::gridlines`](crate::chart::Chart::gridlines) instead; this is for the category axis.
    pub major_gridlines: bool,
    pub minor_gridlines: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major_grid_line: Option<Line>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor_grid_line: Option<Line>,
    /// Axis title font (the text is in [`Chart::axis_titles`](crate::chart::Chart::axis_titles)).
    #[serde(skip_serializing_if = "ChartText::is_empty")]
    pub title_text: ChartText,
}

impl AxisFormat {
    pub fn is_default(&self) -> bool {
        *self == AxisFormat::default()
    }
}

/// Where data labels sit relative to their point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LabelPos {
    Center,
    InsideEnd,
    InsideBase,
    OutsideEnd,
    Left,
    Right,
    Above,
    Below,
    BestFit,
}

impl LabelPos {
    pub const ALL: [LabelPos; 9] = [
        LabelPos::Center,
        LabelPos::InsideEnd,
        LabelPos::InsideBase,
        LabelPos::OutsideEnd,
        LabelPos::Left,
        LabelPos::Right,
        LabelPos::Above,
        LabelPos::Below,
        LabelPos::BestFit,
    ];
    pub fn xml(self) -> &'static str {
        match self {
            LabelPos::Center => "ctr",
            LabelPos::InsideEnd => "inEnd",
            LabelPos::InsideBase => "inBase",
            LabelPos::OutsideEnd => "outEnd",
            LabelPos::Left => "l",
            LabelPos::Right => "r",
            LabelPos::Above => "t",
            LabelPos::Below => "b",
            LabelPos::BestFit => "bestFit",
        }
    }
    pub fn from_xml(s: &str) -> Option<Self> {
        LabelPos::ALL.into_iter().find(|p| p.xml() == s).or(match s {
            "center" => Some(LabelPos::Center),
            "insideEnd" => Some(LabelPos::InsideEnd),
            "insideBase" => Some(LabelPos::InsideBase),
            "outsideEnd" => Some(LabelPos::OutsideEnd),
            "left" => Some(LabelPos::Left),
            "right" => Some(LabelPos::Right),
            "above" | "top" => Some(LabelPos::Above),
            "below" | "bottom" => Some(LabelPos::Below),
            _ => None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            LabelPos::Center => "Center",
            LabelPos::InsideEnd => "Inside End",
            LabelPos::InsideBase => "Inside Base",
            LabelPos::OutsideEnd => "Outside End",
            LabelPos::Left => "Left",
            LabelPos::Right => "Right",
            LabelPos::Above => "Above",
            LabelPos::Below => "Below",
            LabelPos::BestFit => "Best Fit",
        }
    }
}

/// What data labels show and how. The labels are on when
/// [`Chart::data_labels`](crate::chart::Chart::data_labels) (or a series' own switch) is; their
/// number format is each series' [`Series::number_format`](crate::chart::Series::number_format).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataLabels {
    pub value: bool,
    pub category: bool,
    pub series: bool,
    pub percent: bool,
    pub legend_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<LabelPos>,
    /// Between the parts of a label (`, ` when unset).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
    #[serde(skip_serializing_if = "ChartText::is_empty")]
    pub text: ChartText,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<Line>,
}

impl Default for DataLabels {
    fn default() -> Self {
        DataLabels {
            value: true,
            category: false,
            series: false,
            percent: false,
            legend_key: false,
            position: None,
            separator: None,
            text: ChartText::default(),
            fill: None,
            line: None,
        }
    }
}

impl DataLabels {
    pub fn is_default(&self) -> bool {
        *self == DataLabels::default()
    }
    /// The label of one point: the parts this shows, joined by the separator. `value` and
    /// `percent` come formatted ([`Chart::data_label`](crate::chart::Chart::data_label)).
    pub fn text_for(&self, value: &str, percent: Option<&str>, category: &str, series: &str) -> String {
        let mut parts: Vec<String> = vec![];
        if self.series && !series.is_empty() {
            parts.push(series.to_string());
        }
        if self.category && !category.is_empty() {
            parts.push(category.to_string());
        }
        if self.value {
            parts.push(value.to_string());
        }
        if self.percent
            && let Some(p) = percent
        {
            parts.push(p.to_string());
        }
        parts.join(self.separator.as_deref().unwrap_or(", "))
    }
}

/// Chart-wide formatting ([`Chart::format`](crate::chart::Chart::format)).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChartFormat {
    /// Font of all the chart's text; its colour falls back to
    /// [`Chart::text_color`](crate::chart::Chart::text_color), its size is
    /// [`Chart::text_size`](crate::chart::Chart::text_size) (`size` here is unused).
    #[serde(skip_serializing_if = "ChartText::is_empty")]
    pub text: ChartText,
    /// The title's font; its size is [`Chart::title_size`](crate::chart::Chart::title_size)
    /// (`size` here is unused).
    #[serde(skip_serializing_if = "ChartText::is_empty")]
    pub title_text: ChartText,
    /// The title sits over the plot rather than above it.
    pub title_overlay: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_fill: Option<Fill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_line: Option<Line>,
    /// The legend's font (whether it sits over the plot is
    /// [`Chart::legend_overlay`](crate::chart::Chart::legend_overlay)).
    #[serde(skip_serializing_if = "ChartText::is_empty")]
    pub legend_text: ChartText,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend_fill: Option<Fill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend_line: Option<Line>,
    /// Chart area border (its fill is [`Chart::chart_fill`](crate::chart::Chart::chart_fill)).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_line: Option<Line>,
    /// Plot area border (its fill is [`Chart::plot_fill`](crate::chart::Chart::plot_fill)).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plot_line: Option<Line>,
    pub rounded_corners: bool,
    /// Horizontal (category, or X) axis.
    #[serde(skip_serializing_if = "AxisFormat::is_default")]
    pub cat_axis: AxisFormat,
    /// Vertical (value) axis. Its bounds are [`Chart::y_min`](crate::chart::Chart::y_min) and
    /// [`Chart::y_max`](crate::chart::Chart::y_max).
    #[serde(skip_serializing_if = "AxisFormat::is_default")]
    pub val_axis: AxisFormat,
    #[serde(skip_serializing_if = "DataLabels::is_default")]
    pub labels: DataLabels,
    /// Pie and doughnut: angle of the first slice, degrees clockwise from 12 o'clock.
    pub first_slice_angle: f64,
}

impl ChartFormat {
    pub fn is_default(&self) -> bool {
        *self == ChartFormat::default()
    }
}

/// Per-series formatting ([`Series::format`](crate::chart::Series::format)) beyond fill, line,
/// marker symbol and smoothing, which [`Series`](crate::chart::Series) holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SeriesFormat {
    /// Marker size in points (2–72).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_fill: Option<Fill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_line: Option<Line>,
    /// Bars below zero drawn inverted (white, outlined).
    pub invert_if_negative: bool,
    /// Pie and doughnut: how far every slice is pulled out, percent of the radius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explosion: Option<u32>,
    /// Per-point explosion (pie slices pulled out).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub point_explosion: Vec<(usize, u32)>,
}

impl SeriesFormat {
    pub fn is_default(&self) -> bool {
        *self == SeriesFormat::default()
    }
}

/// Colour palettes of Change Colors: id and name. The colourful ones walk the theme accents from
/// different starts; the monochromatic ones shade one accent (or the text colour) from dark to
/// light.
pub const PALETTES: [(&str, &str); 11] = [
    ("colorful1", "Colorful Palette 1"),
    ("colorful2", "Colorful Palette 2"),
    ("colorful3", "Colorful Palette 3"),
    ("colorful4", "Colorful Palette 4"),
    ("monochrome1", "Monochromatic Palette 1"),
    ("monochrome2", "Monochromatic Palette 2"),
    ("monochrome3", "Monochromatic Palette 3"),
    ("monochrome4", "Monochromatic Palette 4"),
    ("monochrome5", "Monochromatic Palette 5"),
    ("monochrome6", "Monochromatic Palette 6"),
    ("monochrome7", "Monochromatic Palette 7"),
];

/// The palette a new chart uses.
pub const DEFAULT_PALETTE: &str = "colorful1";

/// Colour of series (or point) `i` of `n` in `palette`, as a theme colour so it follows theme
/// changes. Unknown palettes are the default one.
pub fn palette_color(palette: &str, i: usize, n: usize) -> ColorRef {
    use deckcraft_color::{ColorTransform, SchemeSlot};
    const ACCENTS: [SchemeSlot; 6] =
        [SchemeSlot::Accent1, SchemeSlot::Accent2, SchemeSlot::Accent3, SchemeSlot::Accent4, SchemeSlot::Accent5, SchemeSlot::Accent6];
    let accent = |k: usize| ACCENTS.get(k % 6).copied().unwrap_or(SchemeSlot::Accent1);
    if let Some(m) = palette.strip_prefix("monochrome") {
        let k: usize = m.parse().unwrap_or(1).clamp(1, 7);
        let slot = if k == 7 { SchemeSlot::Tx1 } else { accent(k - 1) };
        let n = n.max(1);
        // Dark to light around the base colour.
        let f = if n <= 1 { 0.5 } else { (i % n) as f64 / (n - 1) as f64 };
        let c = ColorRef::scheme(slot);
        let c = if k == 7 {
            // From the text colour to a light grey.
            c.with(ColorTransform::LumMod(((1.0 - f * 0.65) * 100_000.0).round() as i32)).with(ColorTransform::LumOff((f * 65_000.0).round() as i32))
        } else if f < 0.5 {
            c.with(ColorTransform::Shade(((0.55 + f * 0.9) * 100_000.0).round() as i32))
        } else if f > 0.5 {
            c.with(ColorTransform::Tint(((1.0 - (f - 0.5) * 1.2) * 100_000.0).round() as i32))
        } else {
            c
        };
        return c;
    }
    let start = match palette {
        "colorful2" => 1,
        "colorful3" => 2,
        "colorful4" => 5,
        _ => 0,
    };
    let step = if palette == "colorful4" { 5 } else { 1 };
    let base = ColorRef::scheme(accent(start + (i % 6) * step));
    match i / 6 {
        0 => base,
        1 => base.with(ColorTransform::Shade(60_000)),
        2 => base.with(ColorTransform::Tint(60_000)),
        r => base.with(ColorTransform::LumMod(100_000 - (r as i32 % 5) * 12_000)),
    }
}

/// Marker symbols, as written in chart parts.
pub const MARKERS: [&str; 11] = ["none", "circle", "square", "diamond", "triangle", "x", "star", "dash", "dot", "plus", "auto"];

/// Number formats offered in the Format pane (Excel-style codes).
pub const NUM_FORMATS: [(&str, &str); 9] = [
    ("General", "General"),
    ("Number", "0.00"),
    ("Number, no decimals", "0"),
    ("Thousands", "#,##0"),
    ("Currency", "$#,##0.00"),
    ("Euro", "#,##0.00 \u{20ac}"),
    ("Percentage", "0%"),
    ("Percentage, 1 decimal", "0.0%"),
    ("Scientific", "0.00E+00"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_text_parts() {
        let mut l = DataLabels::default();
        assert_eq!(l.text_for("4", Some("25%"), "Q1", "S1"), "4");
        l.category = true;
        l.percent = true;
        l.separator = Some("; ".into());
        assert_eq!(l.text_for("4", Some("25%"), "Q1", "S1"), "Q1; 4; 25%");
        l.value = false;
        l.series = true;
        assert_eq!(l.text_for("4", None, "Q1", "S1"), "S1; Q1");
    }

    #[test]
    fn palettes() {
        use deckcraft_color::{ColorScheme, SchemeSlot};
        let scheme = crate::Theme::default().colors;
        let rgb = |c: ColorRef| c.resolve(&scheme, None);
        let _: &ColorScheme = &scheme;
        assert_eq!(rgb(palette_color("colorful1", 1, 3)), scheme.get(SchemeSlot::Accent2));
        assert_eq!(rgb(palette_color("colorful2", 0, 3)), scheme.get(SchemeSlot::Accent2));
        assert_eq!(rgb(palette_color("unknown", 0, 3)), scheme.get(SchemeSlot::Accent1));
        // Monochromatic: one hue, darker to lighter.
        let lum = |c: deckcraft_color::Rgba| c.r as u32 + c.g as u32 + c.b as u32;
        let a = lum(rgb(palette_color("monochrome2", 0, 4)));
        let b = lum(rgb(palette_color("monochrome2", 3, 4)));
        assert!(a < b);
        for (id, _) in PALETTES {
            for i in 0..20 {
                let _ = palette_color(id, i, 20);
            }
        }
        let _ = palette_color("monochrome99", 5, 0);
    }

    #[test]
    fn text_inherits() {
        let base = ChartText { size: Some(10.0), bold: Some(true), ..Default::default() };
        let t = ChartText { size: Some(14.0), ..Default::default() }.over(&base);
        assert_eq!((t.size, t.bold), (Some(14.0), Some(true)));
    }
}
