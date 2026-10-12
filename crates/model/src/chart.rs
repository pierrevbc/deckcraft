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
    // Variants of the types above that PowerPoint and LibreOffice offer as types of their own.
    /// Line chart with markers and no lines (LibreOffice's "Points Only").
    LinePoints,
    StackedLineMarkers,
    PercentLine,
    PercentLineMarkers,
    PercentArea,
    ExplodedPie,
    ExplodedDoughnut,
    PieOfPie,
    BarOfPie,
    /// Scatter with lines and no markers.
    ScatterLines,
    ScatterLinesMarkers,
    RadarMarkers,
    /// Radar with markers and no lines.
    RadarPoints,
    /// Stock: open, high, low, close (candlesticks). `Stock` is high, low, close.
    StockOhlc,
    /// Stock: volume, high, low, close.
    StockVhlc,
    /// Stock: volume, open, high, low, close.
    StockVohlc,
}

impl ChartType {
    /// The first five primary chart types: Clustered Column, Clustered Bar, Line, Area, Pie.
    pub const FIRST_FIVE: [ChartType; 5] = [ChartType::Column, ChartType::Bar, ChartType::Line, ChartType::Area, ChartType::Pie];
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
            ChartType::Stock => "Stock: High-Low-Close",
            ChartType::Surface => "Surface",
            ChartType::Histogram => "Histogram",
            ChartType::Pareto => "Pareto",
            ChartType::BoxWhisker => "Box and Whisker",
            ChartType::Waterfall => "Waterfall",
            ChartType::Funnel => "Funnel",
            ChartType::Treemap => "Treemap",
            ChartType::Sunburst => "Sunburst",
            ChartType::Combo => "Combo",
            ChartType::LinePoints => "Line (Points Only)",
            ChartType::StackedLineMarkers => "Stacked Line with Markers",
            ChartType::PercentLine => "100% Stacked Line",
            ChartType::PercentLineMarkers => "100% Stacked Line with Markers",
            ChartType::PercentArea => "100% Stacked Area",
            ChartType::ExplodedPie => "Exploded Pie",
            ChartType::ExplodedDoughnut => "Exploded Doughnut",
            ChartType::PieOfPie => "Pie of Pie",
            ChartType::BarOfPie => "Bar of Pie",
            ChartType::ScatterLines => "Scatter with Lines",
            ChartType::ScatterLinesMarkers => "Scatter with Lines and Markers",
            ChartType::RadarMarkers => "Radar with Markers",
            ChartType::RadarPoints => "Radar (Points Only)",
            ChartType::StockOhlc => "Stock: Open-High-Low-Close",
            ChartType::StockVhlc => "Stock: Volume-High-Low-Close",
            ChartType::StockVohlc => "Stock: Volume-Open-High-Low-Close",
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
                | ChartType::StackedLineMarkers
                | ChartType::StackedArea
                | ChartType::PercentColumn
                | ChartType::PercentBar
                | ChartType::PercentLine
                | ChartType::PercentLineMarkers
                | ChartType::PercentArea
        )
    }
    pub fn percent(self) -> bool {
        matches!(
            self,
            ChartType::PercentColumn | ChartType::PercentBar | ChartType::PercentLine | ChartType::PercentLineMarkers | ChartType::PercentArea
        )
    }
    /// Line charts: categories along X, a line per series.
    pub fn is_line(self) -> bool {
        matches!(
            self,
            ChartType::Line
                | ChartType::LineMarkers
                | ChartType::LinePoints
                | ChartType::StackedLine
                | ChartType::StackedLineMarkers
                | ChartType::PercentLine
                | ChartType::PercentLineMarkers
        )
    }
    pub fn is_area(self) -> bool {
        matches!(self, ChartType::Area | ChartType::StackedArea | ChartType::PercentArea)
    }
    /// Pie and doughnut charts, exploded or not, and the pie and bar of pie.
    pub fn is_pie(self) -> bool {
        matches!(
            self,
            ChartType::Pie | ChartType::ExplodedPie | ChartType::PieOfPie | ChartType::BarOfPie | ChartType::Doughnut | ChartType::ExplodedDoughnut
        )
    }
    pub fn is_doughnut(self) -> bool {
        matches!(self, ChartType::Doughnut | ChartType::ExplodedDoughnut)
    }
    pub fn is_of_pie(self) -> bool {
        matches!(self, ChartType::PieOfPie | ChartType::BarOfPie)
    }
    pub fn exploded(self) -> bool {
        matches!(self, ChartType::ExplodedPie | ChartType::ExplodedDoughnut)
    }
    /// Scatter charts: numbers along both axes.
    pub fn is_scatter(self) -> bool {
        matches!(self, ChartType::Scatter | ChartType::ScatterLines | ChartType::ScatterLinesMarkers)
    }
    /// Charts that plot X values rather than categories: scatter and bubble.
    pub fn is_xy(self) -> bool {
        self.is_scatter() || self == ChartType::Bubble
    }
    pub fn is_radar(self) -> bool {
        matches!(self, ChartType::Radar | ChartType::RadarMarkers | ChartType::RadarPoints | ChartType::FilledRadar)
    }
    pub fn is_stock(self) -> bool {
        matches!(self, ChartType::Stock | ChartType::StockOhlc | ChartType::StockVhlc | ChartType::StockVohlc)
    }
    /// Stock charts whose first series is the traded volume, drawn as columns.
    pub fn has_volume(self) -> bool {
        matches!(self, ChartType::StockVhlc | ChartType::StockVohlc)
    }
    /// Stock charts with an opening price: candlesticks (up and down bars).
    pub fn has_open(self) -> bool {
        matches!(self, ChartType::StockOhlc | ChartType::StockVohlc)
    }
    /// The series show markers at their points (line, scatter and radar charts).
    pub fn markers(self) -> bool {
        matches!(
            self,
            ChartType::LineMarkers
                | ChartType::LinePoints
                | ChartType::StackedLineMarkers
                | ChartType::PercentLineMarkers
                | ChartType::Scatter
                | ChartType::ScatterLinesMarkers
                | ChartType::RadarMarkers
                | ChartType::RadarPoints
        )
    }
    /// Line and scatter charts, whose lines can be straight, smooth or stepped.
    pub fn has_lines(self) -> bool {
        self.is_line() || self.is_scatter()
    }
    /// Line, scatter and radar variants that show markers and no lines between them.
    pub fn points_only(self) -> bool {
        matches!(self, ChartType::LinePoints | ChartType::Scatter | ChartType::RadarPoints)
    }
    /// Column and bar charts (clustered, stacked, 100%): the ones with 3-D bar shapes.
    pub fn is_column_or_bar(self) -> bool {
        matches!(
            self,
            ChartType::Column | ChartType::StackedColumn | ChartType::PercentColumn | ChartType::Bar | ChartType::StackedBar | ChartType::PercentBar
        )
    }
    /// Types a PPTX can hold as a 3-D chart (`c:bar3DChart`, `c:pie3DChart`, `c:area3DChart`,
    /// `c:line3DChart`, `c:surface3DChart`).
    pub fn three_d_capable(self) -> bool {
        self.is_column_or_bar()
            || self.is_area()
            || matches!(self, ChartType::Pie | ChartType::ExplodedPie | ChartType::Line | ChartType::LineMarkers | ChartType::Surface)
    }
}

/// The shape of the bars of a 3-D column or bar chart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BarShape {
    #[default]
    Box,
    Cylinder,
    Cone,
    ConeToMax,
    Pyramid,
    PyramidToMax,
}

impl BarShape {
    /// The `c:shape` value.
    pub fn ooxml(self) -> &'static str {
        match self {
            BarShape::Box => "box",
            BarShape::Cylinder => "cylinder",
            BarShape::Cone => "cone",
            BarShape::ConeToMax => "coneToMax",
            BarShape::Pyramid => "pyramid",
            BarShape::PyramidToMax => "pyramidToMax",
        }
    }
    pub fn from_ooxml(v: &str) -> BarShape {
        match v {
            "cylinder" => BarShape::Cylinder,
            "cone" => BarShape::Cone,
            "coneToMax" => BarShape::ConeToMax,
            "pyramid" => BarShape::Pyramid,
            "pyramidToMax" => BarShape::PyramidToMax,
            _ => BarShape::Box,
        }
    }
}

/// A 3-D chart: its view and bar shape. DeckCraft draws it as its 2-D type and keeps this for
/// the file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThreeD {
    /// Rotation around the X axis (-90…90°) and the Y axis (0…360°).
    pub rot_x: i32,
    pub rot_y: u32,
    /// Field of view (0…240, in half degrees).
    pub perspective: u32,
    /// Axes at right angles (no perspective).
    pub right_angle_axes: bool,
    /// Depth of the chart, % of its width (20…2000).
    pub depth_percent: u32,
    pub shape: BarShape,
    /// The series sit one behind the other along a depth axis (LibreOffice's "Deep",
    /// PowerPoint's "3-D Column", "3-D Area" and "3-D Line").
    pub deep: bool,
}

impl Default for ThreeD {
    fn default() -> Self {
        ThreeD { rot_x: 15, rot_y: 20, perspective: 30, right_angle_axes: true, depth_percent: 100, shape: BarShape::Box, deep: false }
    }
}

/// Type options beyond [`ChartType`]: what makes a stepped line, an exploded pie, a pie of pie or
/// a 3-D chart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChartVariant {
    /// Line and scatter charts: steps between points rather than straight segments.
    pub stepped: bool,
    /// Exploded pie and doughnut: how far the slices move out, % of the radius (0: 25).
    pub explosion: u32,
    /// Pie of pie and bar of pie: how many points, the last ones, the second plot shows (0: a
    /// third of them).
    pub split_pos: u32,
    /// Pie of pie and bar of pie: size of the second plot, % of the first (0: 75).
    pub second_size: u32,
    /// A 3-D chart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub three_d: Option<ThreeD>,
}

impl ChartVariant {
    pub fn is_plain(&self) -> bool {
        *self == ChartVariant::default()
    }
    /// Explosion in % of the radius, the default when unset.
    pub fn explosion_or_default(&self) -> u32 {
        if self.explosion == 0 { 25 } else { self.explosion.min(400) }
    }
    /// Points the second plot of a pie or bar of pie shows, out of `n`: at least one, and at
    /// least one left for the first plot.
    pub fn split_count(&self, n: usize) -> usize {
        let k = if self.split_pos == 0 { n.div_ceil(3) } else { self.split_pos as usize };
        k.min(n.saturating_sub(1)).max(usize::from(n > 1))
    }
    pub fn second_size_or_default(&self) -> u32 {
        if self.second_size == 0 { 75 } else { self.second_size.clamp(5, 200) }
    }
}

/// How lines join their points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineShape {
    #[default]
    Straight,
    Smooth,
    Stepped,
}

/// An entry of the chart type gallery: a type, and the options that make one of its variants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartPreset {
    /// What `insert.chart` and `chart.type` take (`columnCylinder`), matched ignoring case, `-`,
    /// `_` and spaces. A plain type's id is its name (`stackedColumn`).
    pub id: &'static str,
    /// Its group in the gallery (see [`ChartPreset::FAMILIES`]).
    pub family: &'static str,
    pub label: &'static str,
    pub kind: ChartType,
    /// Sets the series' line shape; `None` keeps it.
    pub line: Option<LineShape>,
    /// 3-D bar shape and depth; `None` is a 2-D chart.
    pub three_d: Option<(BarShape, bool)>,
    /// Combo charts: the line goes on the secondary axis.
    pub secondary: bool,
}

const fn pre(id: &'static str, family: &'static str, label: &'static str, kind: ChartType) -> ChartPreset {
    ChartPreset { id, family, label, kind, line: None, three_d: None, secondary: false }
}

const fn pre3(id: &'static str, family: &'static str, label: &'static str, kind: ChartType, shape: BarShape, deep: bool) -> ChartPreset {
    ChartPreset { id, family, label, kind, line: None, three_d: Some((shape, deep)), secondary: false }
}

const fn pre_line(id: &'static str, family: &'static str, label: &'static str, kind: ChartType, line: LineShape) -> ChartPreset {
    ChartPreset { id, family, label, kind, line: Some(line), three_d: None, secondary: false }
}

impl ChartPreset {
    /// Every chart type and variant of the gallery: the LibreOffice Impress set, with
    /// PowerPoint's names where it has one.
    pub const GALLERY: &'static [ChartPreset] = &[
        pre("column", "Column", "Clustered Column", ChartType::Column),
        pre("stackedColumn", "Column", "Stacked Column", ChartType::StackedColumn),
        pre("percentColumn", "Column", "100% Stacked Column", ChartType::PercentColumn),
        pre3("column3d", "Column", "3-D Clustered Column", ChartType::Column, BarShape::Box, false),
        pre3("stackedColumn3d", "Column", "3-D Stacked Column", ChartType::StackedColumn, BarShape::Box, false),
        pre3("percentColumn3d", "Column", "3-D 100% Stacked Column", ChartType::PercentColumn, BarShape::Box, false),
        pre3("deepColumn3d", "Column", "3-D Column", ChartType::Column, BarShape::Box, true),
        pre3("columnCylinder", "Column", "Cylinder Column", ChartType::Column, BarShape::Cylinder, false),
        pre3("columnCone", "Column", "Cone Column", ChartType::Column, BarShape::Cone, false),
        pre3("columnPyramid", "Column", "Pyramid Column", ChartType::Column, BarShape::Pyramid, false),
        pre("bar", "Bar", "Clustered Bar", ChartType::Bar),
        pre("stackedBar", "Bar", "Stacked Bar", ChartType::StackedBar),
        pre("percentBar", "Bar", "100% Stacked Bar", ChartType::PercentBar),
        pre3("bar3d", "Bar", "3-D Clustered Bar", ChartType::Bar, BarShape::Box, false),
        pre3("stackedBar3d", "Bar", "3-D Stacked Bar", ChartType::StackedBar, BarShape::Box, false),
        pre3("percentBar3d", "Bar", "3-D 100% Stacked Bar", ChartType::PercentBar, BarShape::Box, false),
        pre3("barCylinder", "Bar", "Cylinder Bar", ChartType::Bar, BarShape::Cylinder, false),
        pre3("barCone", "Bar", "Cone Bar", ChartType::Bar, BarShape::Cone, false),
        pre3("barPyramid", "Bar", "Pyramid Bar", ChartType::Bar, BarShape::Pyramid, false),
        pre("pie", "Pie", "Pie", ChartType::Pie),
        pre("explodedPie", "Pie", "Exploded Pie", ChartType::ExplodedPie),
        pre("doughnut", "Pie", "Doughnut", ChartType::Doughnut),
        pre("explodedDoughnut", "Pie", "Exploded Doughnut", ChartType::ExplodedDoughnut),
        pre("pieOfPie", "Pie", "Pie of Pie", ChartType::PieOfPie),
        pre("barOfPie", "Pie", "Bar of Pie", ChartType::BarOfPie),
        pre3("pie3d", "Pie", "3-D Pie", ChartType::Pie, BarShape::Box, false),
        pre3("explodedPie3d", "Pie", "3-D Exploded Pie", ChartType::ExplodedPie, BarShape::Box, false),
        pre("area", "Area", "Area", ChartType::Area),
        pre("stackedArea", "Area", "Stacked Area", ChartType::StackedArea),
        pre("percentArea", "Area", "100% Stacked Area", ChartType::PercentArea),
        pre3("area3d", "Area", "3-D Area", ChartType::Area, BarShape::Box, true),
        pre3("stackedArea3d", "Area", "3-D Stacked Area", ChartType::StackedArea, BarShape::Box, false),
        pre3("percentArea3d", "Area", "3-D 100% Stacked Area", ChartType::PercentArea, BarShape::Box, false),
        pre("line", "Line", "Line", ChartType::Line),
        pre("lineMarkers", "Line", "Line with Markers", ChartType::LineMarkers),
        pre("linePoints", "Line", "Points Only", ChartType::LinePoints),
        pre("stackedLine", "Line", "Stacked Line", ChartType::StackedLine),
        pre("stackedLineMarkers", "Line", "Stacked Line with Markers", ChartType::StackedLineMarkers),
        pre("percentLine", "Line", "100% Stacked Line", ChartType::PercentLine),
        pre("percentLineMarkers", "Line", "100% Stacked Line with Markers", ChartType::PercentLineMarkers),
        pre_line("lineSmooth", "Line", "Smooth Line", ChartType::Line, LineShape::Smooth),
        pre_line("lineStepped", "Line", "Stepped Line", ChartType::Line, LineShape::Stepped),
        pre3("line3d", "Line", "3-D Line", ChartType::Line, BarShape::Box, true),
        pre("scatter", "XY (Scatter)", "Scatter (Points Only)", ChartType::Scatter),
        pre("scatterLinesMarkers", "XY (Scatter)", "Scatter with Lines and Markers", ChartType::ScatterLinesMarkers),
        pre("scatterLines", "XY (Scatter)", "Scatter with Lines", ChartType::ScatterLines),
        pre_line("scatterSmoothMarkers", "XY (Scatter)", "Scatter with Smooth Lines and Markers", ChartType::ScatterLinesMarkers, LineShape::Smooth),
        pre_line("scatterSmooth", "XY (Scatter)", "Scatter with Smooth Lines", ChartType::ScatterLines, LineShape::Smooth),
        pre_line("scatterStepped", "XY (Scatter)", "Scatter with Stepped Lines", ChartType::ScatterLines, LineShape::Stepped),
        pre("bubble", "Bubble", "Bubble", ChartType::Bubble),
        pre("radar", "Radar", "Radar", ChartType::Radar),
        pre("radarMarkers", "Radar", "Radar with Markers", ChartType::RadarMarkers),
        pre("radarPoints", "Radar", "Radar (Points Only)", ChartType::RadarPoints),
        pre("filledRadar", "Radar", "Filled Radar", ChartType::FilledRadar),
        pre("stock", "Stock", "High-Low-Close", ChartType::Stock),
        pre("stockOhlc", "Stock", "Open-High-Low-Close", ChartType::StockOhlc),
        pre("stockVhlc", "Stock", "Volume-High-Low-Close", ChartType::StockVhlc),
        pre("stockVohlc", "Stock", "Volume-Open-High-Low-Close", ChartType::StockVohlc),
        pre("columnLine", "Combo", "Clustered Column - Line", ChartType::Combo),
        pre("stackedColumnLine", "Combo", "Stacked Column - Line", ChartType::Combo),
        ChartPreset {
            id: "columnLineSecondary",
            family: "Combo",
            label: "Clustered Column - Line on Secondary Axis",
            kind: ChartType::Combo,
            line: None,
            three_d: None,
            secondary: true,
        },
    ];

    /// The gallery families, in order.
    pub const FAMILIES: [&'static str; 10] = ["Column", "Bar", "Pie", "Area", "Line", "XY (Scatter)", "Bubble", "Radar", "Stock", "Combo"];

    /// The preset named `id`, ignoring case, `-`, `_` and spaces.
    pub fn find(id: &str) -> Option<&'static ChartPreset> {
        let want = norm_id(id);
        ChartPreset::GALLERY.iter().find(|p| norm_id(p.id) == want)
    }

    /// The presets of `family`.
    pub fn family(family: &str) -> impl Iterator<Item = &'static ChartPreset> + '_ {
        ChartPreset::GALLERY.iter().filter(move |p| p.family == family)
    }

    /// Whether `c` is of this type and variant.
    pub fn matches(&self, c: &Chart) -> bool {
        let three_d = c.variant.three_d.map(|t| (t.shape, t.deep));
        let line = if !self.kind.has_lines() {
            LineShape::Straight
        } else if c.variant.stepped {
            LineShape::Stepped
        } else if !c.series.is_empty() && c.series.iter().all(|s| s.smooth) {
            LineShape::Smooth
        } else {
            LineShape::Straight
        };
        let combo = || {
            let n = c.series.len();
            let col = if self.id == "stackedColumnLine" { ChartType::StackedColumn } else { ChartType::Column };
            c.series.iter().enumerate().all(|(i, s)| {
                let last = i + 1 == n && n > 1;
                // Stacked columns may come with a stacked line (LibreOffice stacks every series).
                let line = |k: Option<ChartType>| k == Some(ChartType::Line) || (col.stacked() && k == Some(ChartType::StackedLine));
                (if last { line(s.kind) } else { s.kind == Some(col) }) && s.secondary_axis == (last && self.secondary)
            })
        };
        c.kind == self.kind
            && three_d == self.three_d
            && self.line.unwrap_or(LineShape::Straight) == line
            && (self.kind != ChartType::Combo || combo())
    }

    /// Turn `c` into this type and variant, keeping its data.
    pub fn apply(&self, c: &mut Chart) {
        c.kind = self.kind;
        let line = self.line.unwrap_or_default();
        c.variant.stepped = line == LineShape::Stepped && self.kind.has_lines();
        if self.kind.has_lines() {
            c.series.iter_mut().for_each(|s| s.smooth = line == LineShape::Smooth);
        }
        c.variant.three_d = self.three_d.map(|(shape, deep)| {
            let base = c.variant.three_d.unwrap_or_default();
            let mut t = ThreeD { shape, deep, ..base };
            if self.kind.is_pie() {
                t.rot_x = 30;
                t.rot_y = 0;
                t.right_angle_axes = false;
            }
            t
        });
        if self.kind.exploded() && c.variant.explosion == 0 {
            c.variant.explosion = 25;
        }
        if self.kind == ChartType::Combo {
            let col = if self.id == "stackedColumnLine" { ChartType::StackedColumn } else { ChartType::Column };
            let n = c.series.len();
            for (i, s) in c.series.iter_mut().enumerate() {
                let line = i + 1 == n && n > 1;
                s.kind = Some(if line { ChartType::Line } else { col });
                s.secondary_axis = line && self.secondary;
            }
        }
        c.vary_colors = c.kind.is_pie() || c.series.len() == 1 && c.vary_colors;
    }
}

fn norm_id(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '-' | '_' | ' ')).flat_map(char::to_lowercase).collect()
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
    /// The number format of the series' data labels (Excel code): the labels' own, else the
    /// source data's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_format: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Chart {
    pub kind: ChartType,
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    pub legend: Option<String>,
    /// `c:legend/c:overlay`: the legend is drawn over the plot instead of shrinking it.
    pub legend_overlay: bool,
    /// Text size in points of the axis labels and legend (from their `c:txPr`, else the chart's);
    /// `None`: the file gives none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f64>,
    /// Title text size in points (its rich text or `c:txPr`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_size: Option<f64>,
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
    /// The value axis' major unit, when the chart fixes it.
    pub y_major: Option<f64>,
    /// The value axis' number format (an Excel format code such as `#,##0` or `0.0%`).
    pub y_format: Option<String>,
    /// The value axis is deleted (`c:delete`): no labels, though gridlines may stay.
    pub y_hidden: bool,
    /// A combo chart's secondary value axis: bounds, major unit and number format.
    pub y2_min: Option<f64>,
    pub y2_max: Option<f64>,
    pub y2_major: Option<f64>,
    pub y2_format: Option<String>,
    pub y2_hidden: bool,
    /// The chart's language (`c:lang`, e.g. `ru-RU`): digit grouping and decimal mark.
    pub lang: Option<String>,
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
    /// Type options beyond `kind`: stepped lines, explosion, pie of pie split, 3-D.
    #[serde(skip_serializing_if = "ChartVariant::is_plain")]
    pub variant: ChartVariant,
}

/// The series of a stock chart by role (see [`Chart::stock_series`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct StockSeries<'a> {
    pub volume: Option<&'a Series>,
    pub open: Option<&'a Series>,
    pub high: Option<&'a Series>,
    pub low: Option<&'a Series>,
    pub close: Option<&'a Series>,
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
            legend_overlay: false,
            text_size: None,
            title_size: None,
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
            y_major: None,
            y_format: None,
            y_hidden: false,
            y2_min: None,
            y2_max: None,
            y2_major: None,
            y2_format: None,
            y2_hidden: false,
            lang: None,
            raw: None,
            workbook: None,
            source: None,
            variant: ChartVariant::default(),
        }
    }
}

impl Chart {
    /// The sample data a new chart starts with (our own numbers).
    pub fn sample(kind: ChartType) -> Self {
        let cats = ["Q1", "Q2", "Q3", "Q4"];
        let data = [[4.2, 2.6, 2.1], [2.8, 4.1, 2.4], [3.6, 1.9, 3.3], [4.7, 3.0, 5.1]];
        if kind.is_stock() {
            return Chart::sample_stock(kind);
        }
        let mut c = Chart { kind, categories: cats.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        let n_series = if kind.is_pie() || matches!(kind, ChartType::Funnel | ChartType::Waterfall | ChartType::Treemap) { 1 } else { 3 };
        let sizes = [[3.0, 1.5, 2.0], [1.0, 2.5, 4.0], [2.0, 3.5, 1.0], [5.0, 2.0, 3.0]];
        for s in 0..n_series {
            c.series.push(Series {
                name: format!("Series {}", s + 1),
                values: data.iter().map(|row| row.get(s).copied()).collect(),
                x_values: if kind.is_xy() { (1..=4).map(|x| Some(x as f64)).collect() } else { vec![] },
                sizes: if kind == ChartType::Bubble { sizes.iter().map(|row| row.get(s).copied()).collect() } else { vec![] },
                ..Default::default()
            });
        }
        c.vary_colors = n_series == 1;
        if kind.exploded() {
            c.variant.explosion = 25;
        }
        if kind == ChartType::Combo
            && let Some(p) = ChartPreset::find("columnLine")
        {
            p.apply(&mut c);
        }
        c
    }
    /// A series' own chart type: its own in a combo chart (columns when it names none), else the
    /// chart's.
    pub fn series_kind(&self, s: &Series) -> ChartType {
        match (self.kind, s.kind) {
            (ChartType::Combo, Some(k)) if k != ChartType::Combo => k,
            (ChartType::Combo, _) => ChartType::Column,
            (k, _) => k,
        }
    }

    /// Whether a series is plotted against the secondary value axis (combo charts only).
    pub fn on_secondary(&self, s: &Series) -> bool {
        self.kind == ChartType::Combo && s.secondary_axis
    }

    /// The value range of one value axis: the primary axis (`y_min`/`y_max` apply), or a combo
    /// chart's secondary axis. A combo chart stacks only its stacked series. `None` when no series
    /// uses that axis.
    pub fn axis_range(&self, secondary: bool) -> Option<(f64, f64)> {
        if self.kind != ChartType::Combo {
            return (!secondary).then(|| self.value_range());
        }
        let ser: Vec<&Series> = self.series.iter().filter(|s| self.on_secondary(s) == secondary).collect();
        if ser.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = (0.0_f64, 0.0_f64);
        let n = ser.iter().map(|s| s.values.len()).max().unwrap_or(0);
        let mut percent = false;
        for i in 0..n {
            let (mut p, mut m) = (0.0, 0.0);
            for s in &ser {
                let k = self.series_kind(s);
                let Some(v) = s.values.get(i).copied().flatten().filter(|v| v.is_finite()) else { continue };
                if k.percent() {
                    percent = true;
                } else if k.stacked() {
                    if v >= 0.0 { p += v } else { m += v }
                } else {
                    hi = hi.max(v);
                    lo = lo.min(v);
                }
            }
            hi = hi.max(p);
            lo = lo.min(m);
        }
        if percent {
            hi = hi.max(1.0);
        }
        Some(if secondary { (lo, hi) } else { (self.y_min.unwrap_or(lo), self.y_max.unwrap_or(hi)) })
    }

    /// A value axis' scale — (min, max, major unit) — with at most `max_intervals` intervals:
    /// the bounds and unit the chart fixes, else PowerPoint's automatic ones (the data plus 5 %
    /// headroom, rounded out to a 1-2-5 unit). `None` when no series uses the axis.
    pub fn axis_scale(&self, secondary: bool, max_intervals: usize) -> Option<(f64, f64, f64)> {
        let (lo, hi) = self.axis_range(secondary)?;
        let (fmin, fmax, major) = if secondary { (self.y2_min, self.y2_max, self.y2_major) } else { (self.y_min, self.y_max, self.y_major) };
        let full_percent = !secondary && self.kind.percent();
        Some(axis_scale(lo, hi, fmin, fmax, major, max_intervals, !full_percent))
    }

    /// A value axis label in the axis' number format and the chart's language.
    pub fn axis_label(&self, secondary: bool, v: f64) -> String {
        let code = if secondary { &self.y2_format } else { &self.y_format };
        crate::numfmt::format(code.as_deref().unwrap_or("General"), v, self.lang.as_deref())
    }

    /// A data label of series `s`: its number format, else its axis', in the chart's language.
    pub fn data_label(&self, s: &Series, v: f64) -> String {
        let axis = if self.on_secondary(s) { &self.y2_format } else { &self.y_format };
        let code = s.number_format.as_ref().or(axis.as_ref());
        crate::numfmt::format(code.map_or("General", String::as_str), v, self.lang.as_deref())
    }

    /// Five trading days of our own made-up prices, in the series order of `kind`: volume (when
    /// it has one), open (when it has one), high, low, close.
    fn sample_stock(kind: ChartType) -> Self {
        let days = ["Mon", "Tue", "Wed", "Thu", "Fri"];
        let open = [42.0, 45.0, 42.0, 39.0, 44.0];
        let high = [46.0, 47.0, 44.0, 45.0, 49.0];
        let low = [40.0, 41.0, 38.0, 38.0, 43.0];
        let close = [45.0, 42.0, 39.0, 44.0, 48.0];
        let volume = [120.0, 150.0, 90.0, 170.0, 130.0];
        let mut cols: Vec<(&str, [f64; 5])> = vec![];
        if kind.has_volume() {
            cols.push(("Volume", volume));
        }
        if kind.has_open() {
            cols.push(("Open", open));
        }
        cols.extend([("High", high), ("Low", low), ("Close", close)]);
        Chart {
            kind,
            categories: days.iter().map(|d| d.to_string()).collect(),
            series: cols
                .into_iter()
                .map(|(name, v)| Series { name: name.into(), values: v.iter().map(|x| Some(*x)).collect(), ..Default::default() })
                .collect(),
            ..Default::default()
        }
    }

    /// Stock charts: the series that hold the volume, open, high, low and close prices, by
    /// position (see [`Chart::sample`]). Missing series are `None`.
    pub fn stock_series(&self) -> StockSeries<'_> {
        let s = |i: usize| self.series.get(i);
        let (vol, rest) = if self.kind.has_volume() { (s(0), 1) } else { (None, 0) };
        let open = self.kind.has_open() && self.series.len().saturating_sub(rest) >= 4;
        let first = rest + usize::from(open);
        StockSeries { volume: vol, open: if open { s(rest) } else { None }, high: s(first), low: s(first + 1), close: s(first + 2) }
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
    if s.contains(',') || s.contains('"') || s.contains('\n') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
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

/// The (min, max, major unit) of an axis over the data range [lo, hi]: `fmin`/`fmax`/`major`
/// where the chart fixes them; otherwise the range takes in zero, gets 5 % headroom (`headroom`)
/// and rounds out to the smallest 1-2-5 unit giving at most `max_intervals` intervals.
pub fn axis_scale(
    lo: f64,
    hi: f64,
    fmin: Option<f64>,
    fmax: Option<f64>,
    major: Option<f64>,
    max_intervals: usize,
    headroom: bool,
) -> (f64, f64, f64) {
    // Values past ±1e100 aren't axis values; clamping keeps the arithmetic finite.
    let fin = |v: Option<f64>| v.filter(|v| v.is_finite()).map(|v| v.clamp(-1e100, 1e100));
    let (fmin, fmax) = (fin(fmin), fin(fmax));
    let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi).clamp(-1e100, 1e100), lo.max(hi).clamp(-1e100, 1e100)) } else { (0.0, 1.0) };
    let mut lo = fmin.unwrap_or(lo.min(0.0));
    let mut hi = fmax.unwrap_or(hi.max(lo));
    if hi - lo < 1e-12 {
        hi = lo + lo.abs().max(1.0);
    }
    let span = hi - lo;
    if headroom && fmax.is_none() && hi > 0.0 {
        hi += span * 0.05;
    }
    if headroom && fmin.is_none() && lo < 0.0 {
        lo -= span * 0.05;
    }
    let n = max_intervals.clamp(1, 50) as f64;
    let step = match fin(major).filter(|m| *m > 0.0 && (hi - lo) / m <= 1000.0) {
        Some(m) => m,
        None => {
            let mut step = 10f64.powf(((hi - lo) / n).log10().floor());
            let mut k = 0;
            // 1, 2, 5, 10, 20… × the magnitude until the intervals fit.
            while ((hi / step).ceil() - (lo / step).floor()) > n && k < 30 {
                step *= if matches!(k % 3, 1) { 2.5 } else { 2.0 };
                k += 1;
            }
            step
        }
    };
    let min = fmin.unwrap_or((lo / step + 1e-9).floor() * step);
    let max = fmax.unwrap_or((hi / step - 1e-9).ceil() * step).max(min + step);
    (min, max, step)
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
    fn automatic_axis_scales_like_powerpoint() {
        // Weekly spend up to ~715 000 on a plot with room for 9 intervals: 0–800 000 by 100 000.
        assert_eq!(axis_scale(0.0, 715_000.0, None, None, None, 9, true), (0.0, 800_000.0, 100_000.0));
        // Clicks up to 6 600: 0–7 000 by 1 000.
        assert_eq!(axis_scale(0.0, 6_600.0, None, None, None, 9, true), (0.0, 7_000.0, 1_000.0));
        // Fewer intervals fit on a short axis.
        assert_eq!(axis_scale(0.0, 6_600.0, None, None, None, 4, true), (0.0, 8_000.0, 2_000.0));
        // Negative data gets room below; fixed bounds and unit are kept.
        let (min, max, _) = axis_scale(-30.0, 100.0, None, None, None, 8, true);
        assert!(min <= -30.0 && max >= 105.0);
        assert_eq!(axis_scale(0.0, 715_000.0, Some(100_000.0), Some(750_000.0), Some(50_000.0), 9, true), (100_000.0, 750_000.0, 50_000.0));
        // 100 % stacked stays at 0–1 without headroom.
        assert_eq!(axis_scale(0.0, 1.0, None, None, None, 5, false), (0.0, 1.0, 0.2));
        // Nonsense never loops or divides by zero.
        for (lo, hi) in [(f64::NAN, 1.0), (5.0, 5.0), (0.0, 0.0), (-1e300, 1e300)] {
            let (a, b, s) = axis_scale(lo, hi, None, None, Some(0.0), 0, true);
            assert!(a.is_finite() && b.is_finite() && s > 0.0 && b > a, "{lo} {hi}: {a} {b} {s}");
        }
    }

    #[test]
    fn axis_labels_use_the_format_and_language() {
        let mut c = Chart::sample(ChartType::Column);
        c.y_format = Some(r#"_-* #\ ##0\ "₽"_-;\-* #\ ##0\ "₽"_-;_-* "-"??\ "₽"_-;_-@_-"#.into());
        c.lang = Some("ru-RU".into());
        assert_eq!(c.axis_label(false, 800_000.0), "800\u{a0}000 ₽");
        assert_eq!(c.axis_label(false, 0.0), "- ₽");
        // No format: General.
        assert_eq!(c.axis_label(true, 2.5), "2,5");
        c.lang = None;
        assert_eq!(c.axis_label(true, 2.5), "2.5");
        // Data labels: the series' format, else the axis'.
        c.lang = Some("ru-RU".into());
        c.series[0].number_format = Some(r#"#,##0\ "₽""#.into());
        assert_eq!(c.data_label(&c.series[0], 516_600_000.0), "516\u{a0}600\u{a0}000 ₽");
        assert_eq!(c.data_label(&c.series[1], 0.0), "- ₽");
    }

    #[test]
    fn combo_axis_ranges() {
        // Spend in columns on the primary axis, clicks as a line on the secondary one.
        let mut c = Chart::sample(ChartType::Combo);
        c.series.truncate(2);
        c.series[0].kind = Some(ChartType::Column);
        c.series[0].values = vec![Some(200_000.0), Some(310_000.0)];
        c.series[1].kind = Some(ChartType::Line);
        c.series[1].values = vec![Some(6_000.0), Some(7_400.0)];
        c.series[1].secondary_axis = true;
        assert_eq!(c.axis_range(false), Some((0.0, 310_000.0)));
        assert_eq!(c.axis_range(true), Some((0.0, 7_400.0)));
        assert_eq!(c.series_kind(&c.series[1]), ChartType::Line);
        // Without a secondary series there is no secondary axis; a non-combo chart never has one.
        c.series[1].secondary_axis = false;
        assert_eq!(c.axis_range(true), None);
        let col = Chart::sample(ChartType::Column);
        assert_eq!(col.axis_range(false), Some(col.value_range()));
        assert_eq!(col.axis_range(true), None);
        assert!(!col.on_secondary(&col.series[0]));
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
        assert_eq!(ChartType::FIRST_FIVE.map(|k| k.label()), ["Clustered Column", "Clustered Bar", "Line", "Area", "Pie"]);
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

    #[test]
    fn gallery_presets_apply_and_match() {
        let mut ids: Vec<&str> = ChartPreset::GALLERY.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ChartPreset::GALLERY.len(), "ids are unique");
        for family in ChartPreset::FAMILIES {
            assert!(ChartPreset::family(family).count() > 0, "{family}");
        }
        assert!(ChartPreset::GALLERY.iter().all(|p| ChartPreset::FAMILIES.contains(&p.family)));
        for p in ChartPreset::GALLERY {
            // A new chart of the preset, and a column chart turned into it, are that preset only.
            let mut c = Chart::sample(p.kind);
            p.apply(&mut c);
            let mut from_column = Chart::sample(ChartType::Column);
            p.apply(&mut from_column);
            for c in [&c, &from_column] {
                let hits: Vec<&str> = ChartPreset::GALLERY.iter().filter(|q| q.matches(c)).map(|q| q.id).collect();
                assert_eq!(hits, [p.id], "{}", p.id);
            }
            assert_eq!(ChartPreset::find(&p.id.to_uppercase()).map(|q| q.id), Some(p.id));
            // A plain type is found by its name.
            if p.three_d.is_none() && p.line.is_none() && p.kind != ChartType::Combo {
                assert_eq!(ChartPreset::find(&format!("{:?}", p.kind)).map(|q| q.kind), Some(p.kind), "{}", p.id);
            }
        }
        // Back to 2-D and straight lines.
        let mut c = Chart::sample(ChartType::Line);
        ChartPreset::find("lineSmooth").unwrap().apply(&mut c);
        assert!(c.series.iter().all(|s| s.smooth));
        ChartPreset::find("line3d").unwrap().apply(&mut c);
        assert!(c.variant.three_d.is_some_and(|t| t.deep) && c.series.iter().all(|s| !s.smooth));
        ChartPreset::find("lineStepped").unwrap().apply(&mut c);
        assert!(c.variant.stepped && c.variant.three_d.is_none());
        ChartPreset::find("column").unwrap().apply(&mut c);
        assert!(!c.variant.stepped);
    }

    #[test]
    fn stock_series_by_role() {
        fn name(s: Option<&Series>) -> Option<&str> {
            s.map(|s| s.name.as_str())
        }
        let c = Chart::sample(ChartType::StockVohlc);
        let r = c.stock_series();
        assert_eq!(
            [name(r.volume), name(r.open), name(r.high), name(r.low), name(r.close)],
            [Some("Volume"), Some("Open"), Some("High"), Some("Low"), Some("Close")]
        );
        let c = Chart::sample(ChartType::Stock);
        let r = c.stock_series();
        assert_eq!([name(r.volume), name(r.open), name(r.high), name(r.close)], [None, None, Some("High"), Some("Close")]);
        // Too few series: the missing roles are empty.
        let mut c = Chart::sample(ChartType::StockOhlc);
        c.series.truncate(2);
        let r = c.stock_series();
        assert!(r.open.is_none() && r.low.is_some() && r.close.is_none());
    }

    #[test]
    fn variant_defaults_are_bounded() {
        let v = ChartVariant { split_pos: u32::MAX, second_size: u32::MAX, explosion: u32::MAX, ..Default::default() };
        assert_eq!(v.split_count(5), 4);
        assert_eq!(v.split_count(0), 0);
        assert_eq!(v.second_size_or_default(), 200);
        assert_eq!(v.explosion_or_default(), 400);
        let auto = ChartVariant::default();
        assert_eq!((auto.split_count(4), auto.split_count(1), auto.explosion_or_default()), (2, 0, 25));
    }
}
