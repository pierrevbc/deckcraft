# Chart type parity

> **Last reviewed:** 2026-10-13 · **Last updated:** 2026-10-13 · **Change:** minor (built on upstream's chart legend, combo, axis format and text size work; previous: minor, data editing and formatting per type; before that: major, created; every LibreOffice Impress chart type and variant audited, then filled in) · **Target:** the chart types of LibreOffice Impress 26 and their PowerPoint counterparts

Every chart type and variant LibreOffice Impress offers in its chart type dialog, with
PowerPoint's name where it has one, and what DeckCraft does with it: model, drawing, PPTX read,
PPTX write, gallery. The PowerPoint-only `chartex` types (waterfall, funnel, treemap, sunburst,
histogram, box and whisker) are tracked in [file-format-parity.md](file-format-parity.md) and
[gaps.md](gaps.md).

The gallery is `ChartPreset::GALLERY` (`crates/model/src/chart.rs`): 61 entries in ten families,
listed by Insert ▸ Chart and Chart Design ▸ Change Chart Type, and taken by `insert.chart {type}`
and `chart.type {type}` as preset ids (`column3d`, `lineSmooth`, `stockOhlc`,
`columnLineSecondary`…). A type is `Chart.kind` (`ChartType`); what makes a variant beyond it
(stepped lines, explosion, pie of pie split, 3-D view and bar shape) is `Chart.variant`.

## Audit

Legend: **Y** works, **P** partial, **N** missing. "Before" is the tree this work stands on:
upstream `main` (525ec7b, with its legend placement #262, combo charts 5c82c08, axis number
formats ce1c7b7 and chart text sizes #358), the chart workbook fix (#241) and the first chart
data editor (#195).

| Type / variant | Model | Draw | PPTX read | PPTX write | Gallery | Before (model / draw / read / write / gallery) |
|---|---|---|---|---|---|---|
| Column: clustered, stacked, 100% | Y | Y | Y | Y | Y | Y / Y / Y / Y / clustered only |
| Bar: clustered, stacked, 100% | Y | Y | Y | Y | Y | Y / P (categories top-down) / Y / Y / clustered only |
| Pie | Y | Y | Y | Y | Y | Y / Y / Y / Y / Y |
| Exploded pie | Y | Y | Y | Y | Y | N / N / read as pie / N / N |
| Doughnut | Y | Y (a ring per series) | Y | Y | Y | Y / P (first series, white hole) / Y / Y / Y |
| Exploded doughnut | Y | Y | Y | Y | Y | N / N / read as doughnut / N / N |
| Pie of pie, bar of pie | Y | Y | Y | Y | Y | N / N / read as pie / N / N |
| Area | Y | Y | Y | Y | Y | Y / Y / Y / Y / Y |
| Stacked area | Y | Y | Y | Y | Y | Y / P (series covered each other) / Y / Y / N |
| 100% stacked area | Y | Y | Y | Y | Y | N / N / read as stacked / N / N |
| Line: lines only | Y | Y | Y | Y | Y | Y / Y / Y / Y / Y |
| Line: points and lines | Y | Y | Y | Y | Y | Y / Y / Y / Y / N |
| Line: points only | Y | Y | Y | Y | Y | N / N (drawn with lines) / read with markers / N / N |
| Line: stacked, with or without markers | Y | Y | Y | Y | Y | lines only / Y / markers lost / Y / N |
| Line: 100% stacked, with or without markers | Y | Y | Y | Y | Y | N / N / read as stacked / N / N |
| Line type: smooth | Y | Y | Y | Y | Y | P (per series) / N / Y / Y / N |
| Line type: stepped | Y | Y | Y (our extension) | Y (our extension) | Y | N / N / N / N / N |
| XY scatter: points only | Y | Y | Y | Y | Y | Y / P (no X axis) / Y / P (other apps drew lines) / Y |
| XY scatter: points and lines, lines only | Y | Y | Y | Y | Y | N / N / read as points only / N / N |
| XY scatter: smooth, stepped | Y | Y | Y (stepped: our extension) | Y (stepped: our extension) | Y | P / N / P / P / N |
| Bubble | Y | Y (sized bubbles) | Y | Y | Y | Y / P (fixed dots) / Y / Y / N |
| Radar (net): lines only | Y | Y | Y | Y | Y | Y / N (drawn as a line chart) / Y / Y / Y |
| Radar: points and lines, points only | Y | Y | Y | Y | Y | N / N / read as lines only / N / N |
| Radar: filled | Y | Y | Y | Y | Y | Y / N (drawn as an area chart) / Y / Y / N |
| Stock: high-low-close | Y | Y | Y | Y | Y | Y / N (drawn as lines) / Y / N (written as a line chart) / N |
| Stock: open-high-low-close (candlestick) | Y | Y | Y | Y | Y | N / N / read as high-low-close / N / N |
| Stock: volume-high-low-close | Y | Y | Y | Y | Y | N / N / read as a combo / N / N |
| Stock: volume-open-high-low-close | Y | Y | Y | Y | Y | N / N / read as a combo / N / N |
| Column and line | Y | Y | Y | Y | Y | Y / Y (5c82c08) / Y / Y / N |
| Stacked columns and line | Y | Y | Y | Y | Y | Y / Y (5c82c08) / Y / Y / N |
| Secondary value axis | Y | Y | Y | Y | Y | Y / Y (5c82c08) / Y (5c82c08, ce1c7b7) / N / N |
| 3-D column and bar (clustered, stacked, 100%, deep) | Y | P (simple depth) | Y | Y | Y | N / N / read as 2-D / N / N |
| Cylinder, cone, pyramid bars | Y | P (shape outlines) | Y | Y | Y | N / N / read as 2-D / N / N |
| 3-D pie, exploded 3-D pie | Y | Y (tilted, with rim) | Y | Y | Y | N / N / read as 2-D / N / N |
| 3-D area (deep, stacked, 100%) | Y | P (drawn flat) | Y | Y | Y | N / N / read as 2-D / N / N |
| 3-D line (deep) | Y | P (drawn flat) | Y | Y | Y | N / N / read as 2-D / N / N |

Before this work, an imported chart left unchanged kept its original chart part, so 3-D and other
unmodelled variants survived a plain open-and-save; any edit lost them. Now the model holds them.

## Built on upstream

Upstream `main` draws and reads some of what this work covers; DeckCraft keeps upstream's model
fields and code for them, and the gallery, data editor, format commands, Format pane and PPTX
writer set and write those fields:

- Legend position and overlay (#262): `Chart::legend`, `Chart::legend_overlay`, laid out by the
  renderer's `legend_layout`.
- Combo charts with columns, lines and a secondary axis (5c82c08): `Chart::series_kind`,
  `Chart::on_secondary`, `Chart::axis_range`; the reader's secondary-axis detection (by axis
  position, not plot group order).
- Axis number formats, automatic major unit and hidden axes (ce1c7b7): `Chart::y_format`,
  `y_major`, `y_hidden` and the secondary axis' `y2_*`, `Chart::lang`, `Series::number_format`
  for data labels, `deckcraft_model::numfmt` (here also scientific notation and halves rounded
  away from zero) and `Chart::axis_scale`'s automatic unit, which every value axis uses.
- Chart text sizes from the file (#358): `Chart::text_size` and `Chart::title_size`; the legend,
  axes and data labels have their own sizes on top (`ChartFormat`), and a chart's own `c:txPr`
  size is its text size when it sets one.

## Data and formatting per type

The Chart Data window (Chart Design ▸ Edit Data, `crates/ui-egui/src/chartdata.rs`) and the
`chart.grid`, `chart.setCell`, `chart.paste`, row and series commands show each type's data as a
table (`crates/model/src/chart_data.rs`); the Format pane's Chart Options and the `chart.format.*`
commands format every type (`crates/model/src/chart_format.rs`).

| Family | Data table | Formatting that applies |
|---|---|---|
| Column, bar (2-D and 3-D) | categories, a column per series | fills (gradients), outlines, gap width, overlap (2-D), invert if negative, point colours, data labels with positions, both axes |
| Line, area | categories, a column per series | lines (width, colour, dash), markers (symbol, size, fill, outline) on the variants with markers, smoothing, area fills, data labels, both axes |
| Pie, doughnut, pie and bar of pie | categories, a column per series (pies plot the first) | point colours, outlines, first slice angle, explosion by series (the exploded types) or by point, hole size, percentage labels |
| XY scatter, bubble | X shared or per series, Y per series, bubble sizes | lines and markers per variant, X axis bounds, units, log scale and reversal, both axes' labels and gridlines |
| Radar | categories, a column per series | lines, markers on the variants with markers, filled shapes, value axis scale and labels |
| Stock | prices by role (Volume, Open, High, Low, Close as the variant asks) | volume columns' fills, axes, labels; the high-low lines and up-down bars keep their default look |
| Combo | categories, a column per series, each showing its series type | each series as its type, primary axis formatting; the secondary axis' bounds, major unit, number format and visibility |

The chart type says whether a line, scatter or radar series shows markers, the series which
symbol: picking a marker on a type without them turns it into its variant with markers. A pie's
series explosion is the exploded type's explosion.

## How it is verified

- `crates/pptx/tests/chart_types.rs`: a deck with one chart per gallery entry is exported, imported
  and compared (type, variant, series, data, categories, X values and bubble sizes, secondary axis,
  combo series types), a second export writes the same chart parts and workbooks; odd data (no
  series, a single point, stock charts short of prices) still makes valid parts; changing the type
  of an imported chart keeps its workbook.
- `crates/pptx/src/write/chart.rs` tests: for every gallery entry, each cached value of the chart
  part sits in the generated workbook cell its formula points to (Edit Data in PowerPoint, Excel
  and LibreOffice opens the chart's own numbers); each type writes its own plot groups and axes.
- `crates/render/src/chart.rs` tests: every gallery entry draws, variants draw differently, stacked
  areas do not cover each other, odd data never panics.
- `crates/model` and `crates/engine` tests: presets apply and match one gallery entry, every entry
  inserts and changes a chart through the commands; data edits (cells, rows, series) on stock,
  radar, scatter, bubble, of-pie, doughnut, 3-D and combo charts keep their formatting; every chart
  style, palette and quick layout applies to every gallery entry and draws.
- `crates/pptx/tests/chart_format.rs`: formatting round trips on column, line, pie, bar, stock,
  radar, scatter, of-pie, exploded doughnut, 3-D column, combo with a secondary axis and bubble
  charts, with their type and variant; `crates/pptx/tests/chart_workbook.rs`: the workbook follows
  added and removed series and points on seven types.
- By hand (2026-10-12): the gallery deck (`deckcraft-cli run … insert.chart`) rendered with
  `deckcraft-cli render` and converted to PDF with LibreOffice 26: LibreOffice opens every chart as
  the same type and variant, 3-D ones in 3-D. The deck saved again as PPTX by LibreOffice reads back
  as the same types, except pie of pie and bar of pie (LibreOffice writes them as plain pies) and
  stepped lines (LibreOffice has no OOXML for them).

## Limits

- Stepped lines: OOXML has no stepped line. DeckCraft keeps them in a chart-space extension
  (`urn:deckcraft:chart`); PowerPoint and LibreOffice draw straight lines. LibreOffice's step
  variants (start, end, centred) are not modelled: DeckCraft steps across, then up or down.
- 3-D: no 3-D renderer. Columns and bars get a simple depth (box faces, cylinder caps, cone and
  pyramid outlines), pies a tilt and a rim; deep charts draw their series side by side, 3-D areas
  and lines are flat. View angles, perspective and depth are kept for the file.
- Pie of pie and bar of pie: the second plot shows the last points (a third of them by default, or
  the file's split position); split by value, percentage or a custom choice is read as automatic.
- Explosion: the exploded types move every slice out by the same amount; single slices moved out
  are point formatting (`c:dPt/c:explosion`).
- LibreOffice's "stack series" for radar charts and "sort by X values" for scatter charts are not
  modelled.
- Not formatted yet: the secondary value axis' line, tick marks, font and title (its bounds,
  unit, number format and visibility are), the high-low lines and up-down bars of stock charts, bubble scale, the pie of pie's connector lines, 3-D walls and floor.
- Markers are on or off per type: a line chart where only some series show markers reads back
  without markers.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-13 | minor | Rebased on upstream main: legend overlay (#262), combo charts and the secondary axis (5c82c08), axis number formats, units and hidden axes (ce1c7b7), text sizes (#358) are upstream's; the secondary axis' bounds, unit, number format and visibility are formatted and written |
| 2026-10-12 | minor | Data table and formatting per type; verification and limits after the data and formatting work |
| 2026-10-12 | major | Created: audit of the LibreOffice Impress chart types and variants, all filled in (model, drawing, PPTX read and write, gallery) |
