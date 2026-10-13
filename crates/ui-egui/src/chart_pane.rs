//! Chart formatting UI: the Format pane's Chart Options (a chart element picker, then that
//! element's options), Chart Design's layout, colour and style galleries, and the Chart Format
//! tab. Everything acts through the `chart.*` commands.
//!
//! The element being formatted lives in the Format pane's tab: `chart` (the chart area) or
//! `chart:<element>` (`chart:valueAxis`, `chart:series:1`…), so the ribbon, the pane and the
//! control channel (`ui.set {formatTab}`) share it.

use deckcraft_model::chart::{Chart, ChartType};
use deckcraft_model::chart_format::{ChartText, LabelPos, MARKERS, NUM_FORMATS, PALETTES, TickLabelPos, TickMark, palette_color};
use deckcraft_model::style::{Dash, Fill, Line};
use deckcraft_model::{ShapeId, ShapeKind};
use egui::{CornerRadius, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};

use crate::icons::Icon;
use crate::ribbon::cref_param;
use crate::theme::{self, Tokens};
use crate::{SlideApp, widgets};

/// The selected chart, if any.
pub fn selected_chart(app: &SlideApp) -> Option<(Chart, ShapeId)> {
    app.session.active().and_then(|d| {
        d.selected_shapes().into_iter().find_map(|s| if let ShapeKind::Chart(c) = &s.kind { Some(((**c).clone(), s.id)) } else { None })
    })
}

/// The chart element being formatted (from the Format pane tab).
pub fn element(app: &SlideApp) -> String {
    app.ui.format_tab.strip_prefix("chart:").unwrap_or("chartArea").to_string()
}

pub fn set_element(app: &mut SlideApp, el: &str) {
    app.ui.format_tab = if el == "chartArea" { "chart".into() } else { format!("chart:{el}") };
}

/// Pie charts of every kind: no axes.
fn pie(c: &Chart) -> bool {
    c.kind.is_pie()
}

/// Series `i` is drawn as a line (its own type in a combo chart): line, scatter and radar
/// charts other than the filled radar.
fn series_lines(c: &Chart, i: usize) -> bool {
    let k = match c.kind {
        ChartType::Combo => c.series.get(i).and_then(|s| s.kind).unwrap_or(ChartType::Column),
        k => k,
    };
    k.has_lines() || (k.is_radar() && k != ChartType::FilledRadar)
}

/// The chart's elements, as the pane's picker lists them: (id, name).
pub fn elements(c: &Chart) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = vec![("chartArea".into(), "Chart Area".into()), ("plotArea".into(), "Plot Area".into())];
    v.push(("title".into(), "Chart Title".into()));
    v.push(("legend".into(), "Legend".into()));
    if !pie(c) {
        let (h, vv) = if c.kind.horizontal() { ("Vertical (Category)", "Horizontal (Value)") } else { ("Horizontal (Category)", "Vertical (Value)") };
        v.push(("categoryAxis".into(), format!("{h} Axis")));
        v.push(("valueAxis".into(), format!("{vv} Axis")));
        v.push(("categoryAxisTitle".into(), format!("{h} Axis Title")));
        v.push(("valueAxisTitle".into(), format!("{vv} Axis Title")));
        v.push(("majorGridlines".into(), format!("{vv} Axis Major Gridlines")));
        v.push(("minorGridlines".into(), format!("{vv} Axis Minor Gridlines")));
        v.push(("categoryMajorGridlines".into(), format!("{h} Axis Major Gridlines")));
    }
    v.push(("dataLabels".into(), "Data Labels".into()));
    for (i, s) in c.series.iter().enumerate() {
        v.push((format!("series:{i}"), format!("Series \"{}\"", s.name)));
    }
    v
}

fn element_name(c: &Chart, el: &str) -> String {
    elements(c).into_iter().find(|(k, _)| k == el).map(|(_, n)| n).unwrap_or_else(|| "Chart Area".into())
}

fn run(app: &mut SlideApp, id: &str, p: Value) {
    let _ = app.run(id, p);
}

fn with_id(id: ShapeId, mut p: Value) -> Value {
    if let Some(o) = p.as_object_mut() {
        o.insert("id".into(), json!(id.0));
    }
    p
}

fn scheme(app: &SlideApp) -> deckcraft_color::ColorScheme {
    app.session.active().and_then(|d| d.doc.masters.first().map(|m| m.scheme())).unwrap_or_else(|| deckcraft_model::Theme::default().colors)
}

fn heading(ui: &mut Ui, s: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(s).font(theme::bold(13.0)));
}

/// A colour button opening the theme colour grid; `on_pick` gets the colour (`None`: the
/// "none" entry) and returns the command to run.
fn colour_button(app: &mut SlideApp, ui: &mut Ui, label: &str, none_label: Option<&str>, on_pick: impl Fn(Option<Value>) -> (&'static str, Value)) {
    let sc = scheme(app);
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = widgets::drop_button(ui, "Color", vec2(70.0, 22.0), true);
            widgets::color_popup(&r).show(|ui| {
                if let Some(c) = widgets::color_grid(ui, &sc, none_label) {
                    let (id, p) = on_pick(c.as_ref().map(cref_param));
                    run(app, id, p);
                }
            });
        });
    });
}

/// Fill options of an element: Automatic, No fill, Solid fill, Gradient fill, and a colour.
fn fill_options(app: &mut SlideApp, ui: &mut Ui, cur: Option<&Fill>, send: &dyn Fn(Value) -> (&'static str, Value)) {
    heading(ui, "Fill");
    let kind = match cur {
        None => "auto",
        Some(Fill::None) => "none",
        Some(Fill::Gradient(_)) => "gradient",
        Some(_) => "solid",
    };
    for (l, k) in [("Automatic", "auto"), ("No fill", "none"), ("Solid fill", "solid"), ("Gradient fill", "gradient")] {
        if ui.radio(kind == k, l).clicked() && kind != k {
            let v = match k {
                "none" => json!("none"),
                "solid" => json!("accent1"),
                "gradient" => {
                    json!({"gradient": {"stops": [[0, {"scheme": "accent1", "lumMod": 40000, "lumOff": 60000}], [1, "accent1"]], "angle": 90}})
                }
                _ => json!("auto"),
            };
            let (id, p) = send(v);
            run(app, id, p);
        }
    }
    if kind != "none" {
        colour_button(app, ui, if kind == "gradient" { "End color" } else { "Color" }, None, |c| {
            let c = c.unwrap_or(json!("accent1"));
            if kind == "gradient" {
                let start = match &c {
                    Value::Object(o) => {
                        let mut o = o.clone();
                        o.insert("lumMod".into(), json!(40000));
                        o.insert("lumOff".into(), json!(60000));
                        Value::Object(o)
                    }
                    v => v.clone(),
                };
                send(json!({"gradient": {"stops": [[0, start], [1, c]], "angle": 90}}))
            } else {
                send(c)
            }
        });
    }
}

/// Line options of an element: Automatic, No line, Solid line, colour, width, dash.
fn line_options(app: &mut SlideApp, ui: &mut Ui, title: &str, cur: Option<&Line>, key: &str, send: &dyn Fn(Value) -> (&'static str, Value)) {
    heading(ui, title);
    let kind = match cur.and_then(|l| l.fill.as_ref()) {
        Some(Fill::None) => "none",
        Some(_) => "solid",
        None if cur.is_some() => "solid",
        None => "auto",
    };
    for (l, k) in [("Automatic", "auto"), ("No line", "none"), ("Solid line", "solid")] {
        if ui.radio(kind == k, l).clicked() && kind != k {
            let v = match k {
                "none" => json!("none"),
                "solid" => json!({"color": "tx1", "width": 0.75}),
                _ => json!("auto"),
            };
            let (id, p) = send(v);
            run(app, id, p);
        }
    }
    if kind != "solid" {
        return;
    }
    colour_button(app, ui, "Color", None, |c| send(json!({"color": c.unwrap_or(json!("tx1"))})));
    ui.horizontal(|ui| {
        ui.label("Width");
        let w = cur.and_then(|l| l.width).unwrap_or(0.75);
        if let Some(nv) = widgets::spinner(ui, &format!("{key}-w"), w, 0.25, " pt", 60.0) {
            let (id, p) = send(json!({"width": nv.clamp(0.0, 100.0)}));
            run(app, id, p);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Dash type");
        let cur_dash = match cur.and_then(|l| l.dash.as_ref()) {
            Some(Dash::Preset { name }) => name.clone(),
            _ => "solid".into(),
        };
        widgets::dropdown(ui, &format!("{key}-dash"), &cur_dash, 120.0, true, |ui| {
            for d in Dash::PRESETS {
                if ui.selectable_label(cur_dash == d, d).clicked() {
                    let (id, p) = send(json!({"dash": d}));
                    run(app, id, p);
                    ui.close();
                }
            }
        });
    });
}

/// Font options of a text element: size, bold, italic, colour, typeface.
fn font_options(app: &mut SlideApp, ui: &mut Ui, cur: &ChartText, key: &str, send: &dyn Fn(Value) -> (&'static str, Value)) {
    heading(ui, "Font");
    ui.horizontal(|ui| {
        ui.label("Size");
        let size = cur.size.unwrap_or(0.0);
        let shown = if size > 0.0 { size } else { 10.0 };
        if let Some(nv) = widgets::spinner(ui, &format!("{key}-size"), shown, 1.0, " pt", 50.0) {
            let (id, p) = send(json!({"size": nv.clamp(1.0, 400.0)}));
            run(app, id, p);
        }
        let b = cur.bold.unwrap_or(false);
        if widgets::icon_toggle(ui, Icon::Bold, "Bold", true, b).clicked() {
            let (id, p) = send(json!({"bold": !b}));
            run(app, id, p);
        }
        let i = cur.italic.unwrap_or(false);
        if widgets::icon_toggle(ui, Icon::Italic, "Italic", true, i).clicked() {
            let (id, p) = send(json!({"italic": !i}));
            run(app, id, p);
        }
    });
    colour_button(app, ui, "Text color", Some("Automatic"), |c| send(json!({"color": c})));
    ui.horizontal(|ui| {
        ui.label("Font");
        let fid = ui.id().with((key, "font"));
        let mut name: String = ui.data_mut(|d| d.get_temp(fid)).unwrap_or_else(|| cur.font.clone().unwrap_or_default());
        let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("Theme font").desired_width(140.0));
        if r.lost_focus() && Some(name.as_str()) != cur.font.as_deref() {
            let (id, p) = send(json!({"font": if name.trim().is_empty() { Value::Null } else { json!(name.trim()) }}));
            run(app, id, p);
        }
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(fid, name));
        } else {
            ui.data_mut(|d| d.remove::<String>(fid));
        }
    });
    if !cur.is_empty() && ui.small_button("Reset font").clicked() {
        let (id, p) = send(json!({"size": null, "bold": null, "italic": null, "color": null, "font": null}));
        run(app, id, p);
    }
}

/// A slider committing `cmd {key: value}` when released.
#[allow(clippy::too_many_arguments)]
fn slider(
    app: &mut SlideApp,
    ui: &mut Ui,
    id: ShapeId,
    label: &str,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    cmd: &str,
    key: &str,
) {
    let mut v = value;
    let r = ui.add(egui::Slider::new(&mut v, range).text(label).suffix(suffix));
    if r.drag_stopped() || (r.changed() && !r.dragged()) {
        run(app, cmd, with_id(id, json!({key: v.round()})));
    }
}

/// Number format picker: the usual formats, then a custom code field.
fn num_format(app: &mut SlideApp, ui: &mut Ui, cur: Option<&str>, key: &str, send: &dyn Fn(Value) -> (&'static str, Value)) {
    heading(ui, "Number");
    let shown = NUM_FORMATS.iter().find(|(_, code)| Some(*code) == cur).map(|(n, _)| *n).unwrap_or(if cur.is_some() { "Custom" } else { "General" });
    ui.horizontal(|ui| {
        ui.label("Category");
        widgets::dropdown(ui, &format!("{key}-nf"), shown, 150.0, true, |ui| {
            for (name, code) in NUM_FORMATS {
                if ui.selectable_label(shown == name, name).clicked() {
                    let (id, p) = send(json!({"numFmt": if code == "General" { Value::Null } else { json!(code) }}));
                    run(app, id, p);
                    ui.close();
                }
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label("Format code");
        let fid = ui.id().with((key, "code"));
        let mut code: String = ui.data_mut(|d| d.get_temp(fid)).unwrap_or_else(|| cur.unwrap_or("General").to_string());
        let r = ui.add(egui::TextEdit::singleline(&mut code).desired_width(110.0));
        if r.lost_focus() && Some(code.as_str()) != cur {
            let (id, p) = send(json!({"numFmt": code.trim()}));
            run(app, id, p);
        }
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(fid, code));
        } else {
            ui.data_mut(|d| d.remove::<String>(fid));
        }
    });
}

/// The Format pane's Chart Options tab.
pub fn pane(app: &mut SlideApp, ui: &mut Ui, c: &Chart, id: ShapeId) {
    let mut el = element(app);
    if !elements(c).iter().any(|(k, _)| *k == el) {
        el = "chartArea".into();
    }
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Chart Options").font(theme::bold(13.0)));
        let name = element_name(c, &el);
        widgets::dropdown(ui, "chart-element", &name, (ui.available_width() - 4.0).max(120.0), true, |ui| {
            for (k, n) in elements(c) {
                if ui.selectable_label(k == el, n).clicked() {
                    set_element(app, &k);
                    ui.close();
                }
            }
        });
    });
    ui.separator();
    let e = |v: Value| -> Value { with_id(id, v) };
    let f = &c.format;
    match el.as_str() {
        "chartArea" => {
            chart_basics(app, ui, c, id);
            let send = |v: Value| ("chart.format.element", e(json!({"element": "chartArea", "fill": v})));
            fill_options(app, ui, c.chart_fill.as_ref(), &send);
            let send = |v: Value| ("chart.format.element", e(json!({"element": "chartArea", "line": v})));
            line_options(app, ui, "Border", f.chart_line.as_ref(), "ca", &send);
            let mut rc = f.rounded_corners;
            if ui.checkbox(&mut rc, "Rounded corners").changed() {
                run(app, "chart.format.element", e(json!({"element": "chartArea", "roundedCorners": rc})));
            }
            let mut text = ChartText { size: c.text_size, ..f.text.clone() };
            if text.color.is_none() {
                text.color.clone_from(&c.text_color);
            }
            let send = |v: Value| ("chart.format.element", e(json!({"element": "chartArea", "font": v})));
            font_options(app, ui, &text, "ca", &send);
        }
        "plotArea" => {
            let send = |v: Value| ("chart.format.element", e(json!({"element": "plotArea", "fill": v})));
            fill_options(app, ui, c.plot_fill.as_ref(), &send);
            let send = |v: Value| ("chart.format.element", e(json!({"element": "plotArea", "line": v})));
            line_options(app, ui, "Border", f.plot_line.as_ref(), "pa", &send);
        }
        "title" => {
            let mut show = c.title.is_some();
            if ui.checkbox(&mut show, "Show chart title").changed() {
                run(app, "chart.title", e(json!({"title": if show { json!("Chart Title") } else { Value::Null }})));
            }
            if c.title.is_some() {
                let mut ov = f.title_overlay;
                if ui.checkbox(&mut ov, "Overlay on the chart").changed() {
                    run(app, "chart.format.element", e(json!({"element": "title", "overlay": ov})));
                }
                let send = |v: Value| ("chart.format.element", e(json!({"element": "title", "font": v})));
                font_options(app, ui, &ChartText { size: c.title_size, ..f.title_text.clone() }, "ti", &send);
                let send = |v: Value| ("chart.format.element", e(json!({"element": "title", "fill": v})));
                fill_options(app, ui, f.title_fill.as_ref(), &send);
                let send = |v: Value| ("chart.format.element", e(json!({"element": "title", "line": v})));
                line_options(app, ui, "Border", f.title_line.as_ref(), "ti", &send);
            }
        }
        "legend" => {
            heading(ui, "Legend Position");
            let cur = c.legend.as_deref();
            for (l, k, v) in [
                ("None", None, "none"),
                ("Top", Some("t"), "top"),
                ("Bottom", Some("b"), "bottom"),
                ("Left", Some("l"), "left"),
                ("Right", Some("r"), "right"),
                ("Top Right", Some("tr"), "topRight"),
            ] {
                if ui.radio(cur == k, l).clicked() {
                    run(app, "chart.format.legend", e(json!({"position": v})));
                }
            }
            if c.legend.is_some() {
                let mut ov = c.legend_overlay;
                if ui.checkbox(&mut ov, "Show the legend without overlapping the chart").changed() {
                    run(app, "chart.format.legend", e(json!({"overlay": !ov})));
                }
                let send = |v: Value| ("chart.format.element", e(json!({"element": "legend", "font": v})));
                font_options(app, ui, &f.legend_text, "lg", &send);
                let send = |v: Value| ("chart.format.element", e(json!({"element": "legend", "fill": v})));
                fill_options(app, ui, f.legend_fill.as_ref(), &send);
                let send = |v: Value| ("chart.format.element", e(json!({"element": "legend", "line": v})));
                line_options(app, ui, "Border", f.legend_line.as_ref(), "lg", &send);
            }
        }
        "categoryAxis" | "valueAxis" => axis_pane(app, ui, c, id, el == "valueAxis"),
        "categoryAxisTitle" | "valueAxisTitle" => {
            let value = el == "valueAxisTitle";
            let cur = if value { &c.axis_titles.1 } else { &c.axis_titles.0 };
            let axis = if value { "value" } else { "category" };
            let mut show = cur.is_some();
            if ui.checkbox(&mut show, "Show axis title").changed() {
                run(app, "chart.format.axis", e(json!({"axis": axis, "title": if show { json!("Axis Title") } else { Value::Null }})));
            }
            if let Some(t) = cur {
                ui.label("Text");
                let tid = ui.id().with(("axis-title", value));
                let mut s: String = ui.data_mut(|d| d.get_temp(tid)).unwrap_or_else(|| t.clone());
                let r = ui.text_edit_singleline(&mut s);
                if r.lost_focus() && s != *t {
                    run(app, "chart.format.axis", e(json!({"axis": axis, "title": s})));
                }
                if r.has_focus() {
                    ui.data_mut(|d| d.insert_temp(tid, s));
                } else {
                    ui.data_mut(|d| d.remove::<String>(tid));
                }
                let a = if value { &f.val_axis } else { &f.cat_axis };
                let send = |v: Value| ("chart.format.element", e(json!({"element": el, "font": v})));
                font_options(app, ui, &a.title_text, &el, &send);
            }
        }
        "majorGridlines" | "minorGridlines" | "categoryMajorGridlines" => {
            let (on, cur, axis, key) = match el.as_str() {
                "majorGridlines" => (c.gridlines, f.val_axis.major_grid_line.as_ref(), "value", "majorGridlines"),
                "minorGridlines" => (f.val_axis.minor_gridlines, f.val_axis.minor_grid_line.as_ref(), "value", "minorGridlines"),
                _ => (f.cat_axis.major_gridlines, f.cat_axis.major_grid_line.as_ref(), "category", "majorGridlines"),
            };
            let mut show = on;
            if ui.checkbox(&mut show, "Show gridlines").changed() {
                run(app, "chart.format.axis", e(json!({"axis": axis, key: show})));
            }
            if on {
                let send = |v: Value| ("chart.format.element", e(json!({"element": el, "line": v})));
                line_options(app, ui, "Line", cur, &el, &send);
            }
        }
        "dataLabels" => labels_pane(app, ui, c, id, None),
        s if s.starts_with("series:") => {
            let i: usize = s.trim_start_matches("series:").parse().unwrap_or(0);
            series_pane(app, ui, c, id, i);
        }
        _ => {}
    }
}

/// #195's chart basics, kept at the top of the chart area options: title, type, elements, data.
fn chart_basics(app: &mut SlideApp, ui: &mut Ui, c: &Chart, id: ShapeId) {
    heading(ui, "Chart Title");
    let mut cur_title = c.title.clone().unwrap_or_default();
    if ui.text_edit_singleline(&mut cur_title).changed() {
        run(app, "chart.title", json!({"id": id.0, "title": cur_title}));
    }
    heading(ui, "Chart Type");
    let current = deckcraft_model::chart::ChartPreset::GALLERY.iter().find(|p| p.matches(c)).map_or(c.kind.label(), |p| p.label);
    ui.menu_button(current, |ui| {
        if let Some(p) = crate::ribbon::chart_gallery(ui, Some(c)) {
            run(app, "chart.type", json!({"id": id.0, "type": p.id}));
        }
    });
    heading(ui, "Chart Elements");
    ui.horizontal_wrapped(|ui| {
        let mut dl = c.data_labels;
        if ui.checkbox(&mut dl, "Data Labels").changed() {
            run(app, "chart.options", json!({"id": id.0, "dataLabels": dl}));
        }
        let mut gl = c.gridlines;
        if ui.checkbox(&mut gl, "Gridlines").changed() {
            run(app, "chart.options", json!({"id": id.0, "gridlines": gl}));
        }
        let mut has_legend = c.legend.is_some();
        if ui.checkbox(&mut has_legend, "Legend").changed() {
            run(app, "chart.options", json!({"id": id.0, "legend": if has_legend { json!("b") } else { Value::Null }}));
        }
    });
    if ui.button("📊 Edit Data in GridCraft").clicked() {
        run(app, "app.dialog", json!({"id": "chartData"}));
    }
}

fn tick_name(t: Option<TickMark>) -> &'static str {
    match t {
        Some(TickMark::Inside) => "Inside",
        Some(TickMark::Outside) => "Outside",
        Some(TickMark::Cross) => "Cross",
        _ => "None",
    }
}

fn axis_pane(app: &mut SlideApp, ui: &mut Ui, c: &Chart, id: ShapeId, value: bool) {
    let a = if value { &c.format.val_axis } else { &c.format.cat_axis };
    let axis = if value { "value" } else { "category" };
    let e = |v: Value| -> Value {
        let mut v = with_id(id, v);
        if let Some(o) = v.as_object_mut() {
            o.insert("axis".into(), json!(axis));
        }
        v
    };
    heading(ui, "Axis Options");
    // The value axis' switch, major unit and number format are the chart's own fields.
    let (hidden, major_unit, num_fmt) =
        if value { (c.y_hidden, c.y_major, c.y_format.as_deref()) } else { (a.hidden, a.major_unit, a.num_fmt.as_deref()) };
    let mut visible = !hidden;
    if ui.checkbox(&mut visible, "Show axis").changed() {
        run(app, "chart.format.axis", e(json!({"visible": visible})));
    }
    let xy = c.kind.is_xy();
    if value || xy {
        // What the automatic scale would be (PowerPoint's: 5% headroom, a 1-2-5 unit).
        let (nmin, nmax, nstep) = if value {
            c.axis_scale(false, 8).unwrap_or((0.0, 1.0, 0.2))
        } else {
            let xs = c.series.iter().flat_map(|s| s.x_values.iter().flatten().copied());
            let (l, h) = xs.fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
            let (lo, hi) = if l <= h { (l, h) } else { (0.0, 1.0) };
            deckcraft_model::chart::axis_scale(lo, hi, None, None, None, 8, true)
        };
        let bounds = if value { (c.y_min, c.y_max) } else { (a.min, a.max) };
        for (label, key, cur, auto) in [("Minimum", "min", bounds.0, nmin), ("Maximum", "max", bounds.1, nmax)] {
            ui.horizontal(|ui| {
                ui.add_sized(vec2(70.0, 20.0), egui::Label::new(label));
                if let Some(nv) = widgets::spinner(ui, &format!("ax-{key}"), cur.unwrap_or(auto), nstep.max(0.01), "", 70.0) {
                    run(app, "chart.format.axis", e(json!({key: nv})));
                }
                if cur.is_some() {
                    if ui.small_button("Reset").on_hover_text("Automatic").clicked() {
                        run(app, "chart.format.axis", e(json!({key: "auto"})));
                    }
                } else {
                    ui.weak("Auto");
                }
            });
        }
        for (label, key, cur, auto) in [("Major unit", "majorUnit", major_unit, nstep), ("Minor unit", "minorUnit", a.minor_unit, nstep / 5.0)] {
            ui.horizontal(|ui| {
                ui.add_sized(vec2(70.0, 20.0), egui::Label::new(label));
                if let Some(nv) = widgets::spinner(ui, &format!("ax-{key}"), cur.unwrap_or(auto), (auto / 2.0).max(0.01), "", 70.0) {
                    run(app, "chart.format.axis", e(json!({key: nv})));
                }
                if cur.is_some() {
                    if ui.small_button("Reset").on_hover_text("Automatic").clicked() {
                        run(app, "chart.format.axis", e(json!({key: "auto"})));
                    }
                } else {
                    ui.weak("Auto");
                }
            });
        }
        let mut log = a.log_base.is_some();
        if value {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut log, "Logarithmic scale").changed() {
                    run(app, "chart.format.axis", e(json!({"logBase": if log { json!(10) } else { Value::Null }})));
                }
                if let Some(b) = a.log_base
                    && let Some(nv) = widgets::spinner(ui, "ax-log", b, 1.0, "", 50.0)
                {
                    run(app, "chart.format.axis", e(json!({"logBase": nv.clamp(2.0, 1000.0)})));
                }
            });
        }
    }
    let mut rev = a.reverse;
    if ui.checkbox(&mut rev, if value { "Values in reverse order" } else { "Categories in reverse order" }).changed() {
        run(app, "chart.format.axis", e(json!({"reverse": rev})));
    }
    // Where the other axis crosses this one: stored on the other axis.
    let other = if value { &c.format.cat_axis } else { &c.format.val_axis };
    let other_name = if value { "category" } else { "value" };
    ui.label(if value { "Horizontal axis crosses" } else { "Vertical axis crosses" });
    ui.horizontal(|ui| {
        use deckcraft_model::chart_format::Crosses;
        let cur = other.crosses;
        if ui.radio(matches!(cur, None | Some(Crosses::AutoZero | Crosses::Min)), "Automatic").clicked() {
            run(app, "chart.format.axis", with_id(id, json!({"axis": other_name, "crosses": "auto"})));
        }
        if ui.radio(matches!(cur, Some(Crosses::Max)), "Maximum").clicked() {
            run(app, "chart.format.axis", with_id(id, json!({"axis": other_name, "crosses": "max"})));
        }
        if value {
            let at = if let Some(Crosses::At { value }) = cur { Some(value) } else { None };
            if ui.radio(at.is_some(), "At").clicked() && at.is_none() {
                run(app, "chart.format.axis", with_id(id, json!({"axis": other_name, "crosses": 0})));
            }
            if let Some(v) = at
                && let Some(nv) = widgets::spinner(ui, "ax-cross", v, 1.0, "", 50.0)
            {
                run(app, "chart.format.axis", with_id(id, json!({"axis": other_name, "crosses": nv})));
            }
        }
    });
    heading(ui, "Tick Marks");
    for (label, key, cur) in [("Major type", "majorTick", a.major_tick), ("Minor type", "minorTick", a.minor_tick)] {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(70.0, 20.0), egui::Label::new(label));
            widgets::dropdown(ui, &format!("ax-{key}"), tick_name(cur), 100.0, true, |ui| {
                for (n, v) in [("None", "none"), ("Inside", "in"), ("Outside", "out"), ("Cross", "cross")] {
                    if ui.selectable_label(tick_name(cur) == n, n).clicked() {
                        run(app, "chart.format.axis", e(json!({key: v})));
                        ui.close();
                    }
                }
            });
        });
    }
    heading(ui, "Labels");
    let lp = a.label_pos.unwrap_or(TickLabelPos::NextTo);
    let lp_name = |p: TickLabelPos| match p {
        TickLabelPos::NextTo => "Next to Axis",
        TickLabelPos::Low => "Low",
        TickLabelPos::High => "High",
        TickLabelPos::None => "None",
    };
    ui.horizontal(|ui| {
        ui.add_sized(vec2(70.0, 20.0), egui::Label::new("Position"));
        widgets::dropdown(ui, "ax-lp", lp_name(lp), 110.0, true, |ui| {
            for p in [TickLabelPos::NextTo, TickLabelPos::Low, TickLabelPos::High, TickLabelPos::None] {
                if ui.selectable_label(p == lp, lp_name(p)).clicked() {
                    run(app, "chart.format.axis", e(json!({"labelPosition": p.xml()})));
                    ui.close();
                }
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add_sized(vec2(70.0, 20.0), egui::Label::new("Rotation"));
        if let Some(nv) = widgets::spinner(ui, "ax-rot", a.rotation.unwrap_or(0.0), 15.0, "°", 50.0) {
            run(app, "chart.format.axis", e(json!({"rotation": nv.clamp(-90.0, 90.0)})));
        }
    });
    if value || c.kind.is_xy() {
        let send = |v: Value| ("chart.format.axis", e(v));
        num_format(app, ui, num_fmt, &format!("ax-{axis}"), &send);
    }
    heading(ui, "Gridlines");
    ui.horizontal(|ui| {
        let mut major = if value { c.gridlines } else { a.major_gridlines };
        if ui.checkbox(&mut major, "Major").changed() {
            run(app, "chart.format.axis", e(json!({"majorGridlines": major})));
        }
        let mut minor = a.minor_gridlines;
        if ui.checkbox(&mut minor, "Minor").changed() {
            run(app, "chart.format.axis", e(json!({"minorGridlines": minor})));
        }
    });
    let el = if value { "valueAxis" } else { "categoryAxis" };
    let send = |v: Value| ("chart.format.element", with_id(id, json!({"element": el, "line": v})));
    line_options(app, ui, "Line", a.line.as_ref(), el, &send);
    let send = |v: Value| ("chart.format.element", with_id(id, json!({"element": el, "font": v})));
    font_options(app, ui, &a.text, el, &send);
    if value && c.series.iter().any(|s| c.on_secondary(s)) {
        secondary_axis_pane(app, ui, c, id);
    }
}

/// A combo chart's secondary value axis: shown or not, bounds, major unit, number format.
fn secondary_axis_pane(app: &mut SlideApp, ui: &mut Ui, c: &Chart, id: ShapeId) {
    let e = |v: Value| -> Value {
        let mut v = with_id(id, v);
        if let Some(o) = v.as_object_mut() {
            o.insert("axis".into(), json!("secondary"));
        }
        v
    };
    heading(ui, "Secondary Vertical Axis");
    let mut visible = !c.y2_hidden;
    if ui.checkbox(&mut visible, "Show secondary axis").changed() {
        run(app, "chart.format.axis", e(json!({"visible": visible})));
    }
    let (nmin, nmax, nstep) = c.axis_scale(true, 8).unwrap_or((0.0, 1.0, 0.2));
    for (label, key, cur, auto) in
        [("Minimum", "min", c.y2_min, nmin), ("Maximum", "max", c.y2_max, nmax), ("Major unit", "majorUnit", c.y2_major, nstep)]
    {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(70.0, 20.0), egui::Label::new(label));
            if let Some(nv) = widgets::spinner(ui, &format!("ax2-{key}"), cur.unwrap_or(auto), nstep.max(0.01), "", 70.0) {
                run(app, "chart.format.axis", e(json!({key: nv})));
            }
            if cur.is_some() {
                if ui.small_button("Reset").on_hover_text("Automatic").clicked() {
                    run(app, "chart.format.axis", e(json!({key: "auto"})));
                }
            } else {
                ui.weak("Auto");
            }
        });
    }
    let send = |v: Value| ("chart.format.axis", e(v));
    num_format(app, ui, c.y2_format.as_deref(), "ax-secondary", &send);
}

/// Label positions offered for the chart's type (as PowerPoint offers them).
fn positions(c: &Chart) -> &'static [LabelPos] {
    use LabelPos::*;
    match c.kind {
        k if k.is_doughnut() || k.is_area() || k.is_radar() => &[],
        k if k.is_pie() => &[Center, InsideEnd, OutsideEnd, BestFit],
        k if k.is_bar_like() && k.stacked() => &[Center, InsideEnd, InsideBase],
        k if k.is_bar_like() => &[Center, InsideEnd, InsideBase, OutsideEnd],
        _ => &[Center, Left, Right, Above, Below],
    }
}

fn labels_pane(app: &mut SlideApp, ui: &mut Ui, c: &Chart, id: ShapeId, series: Option<usize>) {
    let l = &c.format.labels;
    let e = |v: Value| with_id(id, v);
    let shown = match series {
        Some(i) => c.series.get(i).and_then(|s| s.data_labels).unwrap_or(c.data_labels),
        None => c.data_labels,
    };
    let mut show = shown;
    if ui.checkbox(&mut show, "Show data labels").changed() {
        let mut p = json!({"show": show});
        if let (Some(i), Some(o)) = (series, p.as_object_mut()) {
            o.insert("series".into(), json!(i));
        }
        run(app, "chart.format.labels", e(p));
    }
    if !shown {
        return;
    }
    heading(ui, "Label Contains");
    for (label, key, on) in [
        ("Series Name", "seriesName", l.series),
        ("Category Name", "category", l.category),
        ("Value", "value", l.value),
        ("Percentage", "percent", l.percent),
        ("Legend key", "legendKey", l.legend_key),
    ] {
        if key == "percent" && !pie(c) {
            continue;
        }
        let mut v = on;
        if ui.checkbox(&mut v, label).changed() {
            run(app, "chart.format.labels", e(json!({key: v})));
        }
    }
    ui.horizontal(|ui| {
        ui.label("Separator");
        let cur = l.separator.clone().unwrap_or_else(|| ", ".into());
        let name = |s: &str| match s {
            ", " => "Comma".to_string(),
            "; " => "Semicolon".to_string(),
            ". " => "Period".to_string(),
            "\n" => "New Line".to_string(),
            " " => "Space".to_string(),
            other => format!("\"{other}\""),
        };
        widgets::dropdown(ui, "dl-sep", &name(&cur), 110.0, true, |ui| {
            for s in [", ", "; ", ". ", "\n", " "] {
                if ui.selectable_label(cur == s, name(s)).clicked() {
                    run(app, "chart.format.labels", e(json!({"separator": s})));
                    ui.close();
                }
            }
        });
    });
    let allowed = positions(c);
    if !allowed.is_empty() {
        heading(ui, "Label Position");
        for p in allowed {
            if ui.radio(l.position == Some(*p), p.label()).clicked() {
                run(app, "chart.format.labels", e(json!({"position": p.xml()})));
            }
        }
    }
    // The labels' number format is their series' (all of them unless one is formatted).
    let send = |v: Value| {
        let mut v = with_id(id, v);
        if let (Some(i), Some(o)) = (series, v.as_object_mut()) {
            o.insert("series".into(), json!(i));
        }
        ("chart.format.labels", v)
    };
    let cur = c.series.get(series.unwrap_or(0)).and_then(|s| s.number_format.as_deref()).filter(|f| !f.eq_ignore_ascii_case("general"));
    num_format(app, ui, cur, "dl", &send);
    let send = |v: Value| ("chart.format.element", with_id(id, json!({"element": "dataLabels", "font": v})));
    font_options(app, ui, &l.text, "dl", &send);
    let send = |v: Value| ("chart.format.element", with_id(id, json!({"element": "dataLabels", "fill": v})));
    fill_options(app, ui, l.fill.as_ref(), &send);
    let send = |v: Value| ("chart.format.element", with_id(id, json!({"element": "dataLabels", "line": v})));
    line_options(app, ui, "Border", l.line.as_ref(), "dl", &send);
}

fn series_pane(app: &mut SlideApp, ui: &mut Ui, c: &Chart, id: ShapeId, i: usize) {
    let Some(s) = c.series.get(i) else { return };
    let e = |v: Value| -> Value {
        let mut v = with_id(id, v);
        if let Some(o) = v.as_object_mut() {
            o.insert("series".into(), json!(i));
        }
        v
    };
    heading(ui, "Series Options");
    if c.kind.is_bar_like() {
        slider(app, ui, id, "Gap width", (c.gap_width * 100.0).round(), 0.0..=500.0, "%", "chart.format.options", "gapWidth");
        if !c.kind.stacked() {
            slider(app, ui, id, "Series overlap", (c.overlap * 100.0).round(), -100.0..=100.0, "%", "chart.format.options", "overlap");
        }
        let mut inv = s.format.invert_if_negative;
        if ui.checkbox(&mut inv, "Invert if negative").changed() {
            run(app, "chart.format.series", e(json!({"invertIfNegative": inv})));
        }
    }
    if pie(c) && !c.kind.is_of_pie() {
        slider(app, ui, id, "Angle of first slice", c.format.first_slice_angle.round(), 0.0..=360.0, "°", "chart.format.options", "firstSliceAngle");
        // The series' own explosion, else the exploded type's.
        let x = s.format.explosion.or(c.kind.exploded().then(|| c.variant.explosion_or_default()));
        let mut x = f64::from(x.unwrap_or(0));
        let r = ui.add(egui::Slider::new(&mut x, 0.0..=100.0).text("Pie explosion").suffix("%"));
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
            run(app, "chart.format.series", e(json!({"explosion": x.round()})));
        }
        if c.kind.is_doughnut() {
            slider(app, ui, id, "Doughnut hole size", (c.hole_size * 100.0).round(), 10.0..=90.0, "%", "chart.format.options", "holeSize");
        }
    }
    if c.series.len() == 1 || pie(c) {
        let mut vary = c.vary_colors;
        if ui.checkbox(&mut vary, "Vary colors by point").changed() {
            run(app, "chart.format.options", with_id(id, json!({"varyColors": vary})));
        }
    }
    let lines = series_lines(c, i);
    if !lines {
        let send = |v: Value| ("chart.format.series", e(json!({"fill": v})));
        fill_options(app, ui, s.fill.as_ref(), &send);
    }
    let send = |v: Value| ("chart.format.series", e(json!({"line": v})));
    line_options(app, ui, if lines { "Line" } else { "Border" }, s.line.as_ref(), &format!("s{i}"), &send);
    if lines {
        let mut smooth = s.smooth;
        if ui.checkbox(&mut smooth, "Smoothed line").changed() {
            run(app, "chart.format.series", e(json!({"smooth": smooth})));
        }
        heading(ui, "Marker");
        let cur = s.marker.clone().unwrap_or_else(|| "auto".into());
        ui.horizontal(|ui| {
            ui.add_sized(vec2(70.0, 20.0), egui::Label::new("Type"));
            widgets::dropdown(ui, &format!("s{i}-mk"), &cur, 110.0, true, |ui| {
                for m in MARKERS {
                    if ui.selectable_label(cur == m, m).clicked() {
                        run(app, "chart.format.series", e(json!({"marker": m})));
                        ui.close();
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add_sized(vec2(70.0, 20.0), egui::Label::new("Size"));
            if let Some(nv) = widgets::spinner(ui, &format!("s{i}-ms"), f64::from(s.format.marker_size.unwrap_or(7)), 1.0, "", 50.0) {
                run(app, "chart.format.series", e(json!({"markerSize": nv.clamp(2.0, 72.0)})));
            }
        });
        colour_button(app, ui, "Marker fill", Some("Automatic"), |col| {
            ("chart.format.series", e(json!({"markerFill": col.unwrap_or(json!("auto"))})))
        });
        colour_button(app, ui, "Marker border", Some("Automatic"), |col| {
            ("chart.format.series", e(json!({"markerLine": col.map(|c| json!({"color": c})).unwrap_or(json!("auto"))})))
        });
    }
    let mut dl = s.data_labels.unwrap_or(c.data_labels);
    ui.add_space(6.0);
    if ui.checkbox(&mut dl, "Data labels for this series").changed() {
        run(app, "chart.format.labels", e(json!({"show": dl})));
    }
    // Points: colours (and explosion for pies).
    if pie(c) || (c.vary_colors && c.series.len() == 1) || c.kind.is_bar_like() {
        heading(ui, "Points");
        let sc = scheme(app);
        let n = c.categories.len().max(s.values.len());
        for k in 0..n.min(50) {
            let name = c.categories.get(k).cloned().unwrap_or_else(|| format!("Point {}", k + 1));
            ui.horizontal(|ui| {
                let own = s.point_fills.iter().find(|(j, _)| *j == k).map(|(_, f)| f.clone());
                let col = match &own {
                    Some(Fill::Solid { color }) => color.resolve(&sc, None),
                    _ => palette_color(&c.palette, if c.vary_colors || pie(c) { k } else { i }, n).resolve(&sc, None),
                };
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                ui.painter().rect(
                    r,
                    CornerRadius::same(2),
                    theme::to_color32(col),
                    Stroke::new(1.0, Tokens::get(ui.ctx()).border),
                    egui::StrokeKind::Inside,
                );
                ui.label(&name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = widgets::drop_button(ui, "", vec2(26.0, 20.0), true);
                    widgets::color_popup(&b).show(|ui| {
                        if let Some(pick) = widgets::color_grid(ui, &sc, Some("Automatic")) {
                            let fill = pick.as_ref().map(cref_param).unwrap_or(json!("auto"));
                            run(app, "chart.format.point", e(json!({"point": k, "fill": fill})));
                        }
                    });
                    if pie(c) {
                        let x = s.format.point_explosion.iter().find(|(j, _)| *j == k).map(|(_, x)| f64::from(*x)).unwrap_or(0.0);
                        if let Some(nv) = widgets::spinner(ui, &format!("pt{i}-{k}"), x, 5.0, "%", 40.0) {
                            run(app, "chart.format.point", e(json!({"point": k, "explosion": nv.clamp(0.0, 400.0)})));
                        }
                    }
                });
            });
        }
    }
}

// ---------- ribbon ----------

/// Chart Design ▸ Chart Layouts (Add Chart Element, Quick Layout) and Chart Styles (Change
/// Colors, styles).
pub fn design_groups(app: &mut SlideApp, ui: &mut Ui) {
    let Some((c, id)) = selected_chart(app) else { return };
    let e = |v: Value| with_id(id, v);
    widgets::group(ui, |ui| {
        let r = widgets::big_button(ui, Icon::ChartElements, "Add Chart\nElement", true);
        egui::Popup::menu(&r).show(|ui| {
            let axes = !pie(&c);
            if axes {
                ui.menu_button("Axes", |ui| {
                    for (l, axis, hidden) in [("Primary Horizontal", "category", c.format.cat_axis.hidden), ("Primary Vertical", "value", c.y_hidden)]
                    {
                        let mut on = !hidden;
                        if ui.checkbox(&mut on, l).changed() {
                            run(app, "chart.format.axis", e(json!({"axis": axis, "visible": on})));
                        }
                    }
                });
                ui.menu_button("Axis Titles", |ui| {
                    for (l, axis, t) in [("Primary Horizontal", "category", &c.axis_titles.0), ("Primary Vertical", "value", &c.axis_titles.1)] {
                        let mut on = t.is_some();
                        if ui.checkbox(&mut on, l).changed() {
                            run(app, "chart.format.axis", e(json!({"axis": axis, "title": if on { json!("Axis Title") } else { Value::Null }})));
                        }
                    }
                });
            }
            ui.menu_button("Chart Title", |ui| {
                for (l, show, overlay) in [("None", false, false), ("Above Chart", true, false), ("Centered Overlay", true, true)] {
                    let cur = c.title.is_some() == show && (!show || c.format.title_overlay == overlay);
                    if ui.selectable_label(cur, l).clicked() {
                        if !show {
                            run(app, "chart.title", e(json!({"title": null})));
                        } else {
                            if c.title.is_none() {
                                run(app, "chart.title", e(json!({"title": "Chart Title"})));
                            }
                            run(app, "chart.format.element", e(json!({"element": "title", "overlay": overlay})));
                        }
                        ui.close();
                    }
                }
            });
            ui.menu_button("Data Labels", |ui| {
                if ui.selectable_label(!c.data_labels, "None").clicked() {
                    run(app, "chart.format.labels", e(json!({"show": false})));
                    ui.close();
                }
                for p in positions(&c) {
                    let cur = c.data_labels && c.format.labels.position == Some(*p);
                    if ui.selectable_label(cur, p.label()).clicked() {
                        run(app, "chart.format.labels", e(json!({"show": true, "position": p.xml()})));
                        ui.close();
                    }
                }
                if positions(&c).is_empty() && ui.selectable_label(c.data_labels, "Show").clicked() {
                    run(app, "chart.format.labels", e(json!({"show": true})));
                    ui.close();
                }
                ui.separator();
                if ui.button("More Data Label Options…").clicked() {
                    open_pane(app, "dataLabels");
                    ui.close();
                }
            });
            if axes {
                ui.menu_button("Gridlines", |ui| {
                    let rows = [
                        ("Primary Major Horizontal", "value", "majorGridlines", c.gridlines),
                        ("Primary Major Vertical", "category", "majorGridlines", c.format.cat_axis.major_gridlines),
                        ("Primary Minor Horizontal", "value", "minorGridlines", c.format.val_axis.minor_gridlines),
                        ("Primary Minor Vertical", "category", "minorGridlines", c.format.cat_axis.minor_gridlines),
                    ];
                    let rows = if c.kind.horizontal() {
                        rows.map(|(l, a, k, on)| (l, if a == "value" { "category" } else { "value" }, k, on))
                    } else {
                        rows
                    };
                    for (l, axis, key, _) in rows {
                        let a = if axis == "value" { &c.format.val_axis } else { &c.format.cat_axis };
                        let cur = match (axis, key) {
                            ("value", "majorGridlines") => c.gridlines,
                            (_, "majorGridlines") => a.major_gridlines,
                            _ => a.minor_gridlines,
                        };
                        let mut on = cur;
                        if ui.checkbox(&mut on, l).changed() {
                            run(app, "chart.format.axis", e(json!({"axis": axis, key: on})));
                        }
                    }
                });
            }
            ui.menu_button("Legend", |ui| {
                for (l, k, v) in [
                    ("None", None, "none"),
                    ("Right", Some("r"), "right"),
                    ("Top", Some("t"), "top"),
                    ("Left", Some("l"), "left"),
                    ("Bottom", Some("b"), "bottom"),
                    ("Top Right", Some("tr"), "topRight"),
                ] {
                    if ui.selectable_label(c.legend.as_deref() == k, l).clicked() {
                        run(app, "chart.format.legend", e(json!({"position": v})));
                        ui.close();
                    }
                }
            });
        });
        let r = widgets::big_button(ui, Icon::Layout, "Quick\nLayout", true);
        egui::Popup::menu(&r).show(|ui| {
            for (n, name) in LAYOUT_NAMES.iter().enumerate() {
                if ui.button(format!("Layout {}: {name}", n + 1)).clicked() {
                    run(app, "chart.quickLayout", e(json!({"layout": n + 1})));
                    ui.close();
                }
            }
        });
    });
    widgets::group(ui, |ui| {
        let r = widgets::big_button(ui, Icon::Colors, "Change\nColors", true);
        let sc = scheme(app);
        egui::Popup::menu(&r).width(260.0).show(|ui| {
            for (pid, name) in PALETTES {
                let resp = ui.horizontal(|ui| {
                    for k in 0..6 {
                        let col = palette_color(pid, k, 6).resolve(&sc, None);
                        let (rr, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        ui.painter().rect_filled(rr, CornerRadius::same(2), theme::to_color32(col));
                    }
                    ui.selectable_label(c.palette == pid, name)
                });
                if resp.inner.clicked() {
                    run(app, "chart.colors", e(json!({"palette": pid})));
                    ui.close();
                }
            }
        });
        let r = widgets::big_button(ui, Icon::ChartStyles, "Chart\nStyles", true);
        egui::Popup::menu(&r).show(|ui| {
            for (n, name) in STYLE_NAMES.iter().enumerate() {
                if ui.button(format!("Style {}: {name}", n + 1)).clicked() {
                    run(app, "chart.style", e(json!({"style": n + 1})));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Reset to Match Style").clicked() {
                run(app, "chart.resetStyle", e(json!({})));
                ui.close();
            }
        });
    });
}

/// Names of our Quick Layouts and Chart Styles (see `chart.quickLayout`, `chart.style`).
pub const LAYOUT_NAMES: [&str; 8] = [
    "Title, legend right",
    "Title, legend top, data labels",
    "Title, legend bottom",
    "Data labels, legend bottom",
    "Title, axis titles",
    "Title, value axis title, legend right",
    "Title, axis titles, minor gridlines",
    "Title, labels with category and percent",
];
pub const STYLE_NAMES: [&str; 8] =
    ["Clean", "Labels, no gridlines", "Shaded plot", "Dark", "Gradient series", "Bold title", "Outlined", "Bold lines and markers"];

fn open_pane(app: &mut SlideApp, el: &str) {
    run(app, "view.pane", json!({"pane": "format"}));
    set_element(app, el);
}

/// The Chart Format tab: Current Selection, element fill, outline and text.
pub fn format_tab(app: &mut SlideApp, ui: &mut Ui) {
    let Some((c, id)) = selected_chart(app) else { return };
    let el = element(app);
    let el = if elements(&c).iter().any(|(k, _)| *k == el) { el } else { "chartArea".to_string() };
    widgets::group(ui, |ui| {
        widgets::rows(ui, |ui| {
            let name = element_name(&c, &el);
            widgets::dropdown(ui, "chart-sel", &name, 200.0, true, |ui| {
                for (k, n) in elements(&c) {
                    if ui.selectable_label(k == el, n).clicked() {
                        set_element(app, &k);
                        ui.close();
                    }
                }
            });
            ui.add_space(4.0);
            if widgets::small_button(ui, Icon::FormatBackground, "Format Selection", true, false).clicked() {
                open_pane(app, &el);
            }
            if widgets::small_button(ui, Icon::ResetSlide, "Reset to Match Style", true, false).clicked() {
                run(app, "chart.resetStyle", with_id(id, json!({})));
            }
        });
    });
    // Fill, outline and text of the selected element through its command.
    let series = el.strip_prefix("series:").and_then(|s| s.parse::<usize>().ok());
    let fill_cmd = |v: Value| -> Option<(&'static str, Value)> {
        match (series, el.as_str()) {
            (Some(i), _) => Some(("chart.format.series", with_id(id, json!({"series": i, "fill": v})))),
            (None, "chartArea" | "plotArea" | "title" | "legend" | "dataLabels") => {
                Some(("chart.format.element", with_id(id, json!({"element": el, "fill": v}))))
            }
            _ => None,
        }
    };
    let line_cmd = |v: Value| -> Option<(&'static str, Value)> {
        match (series, el.as_str()) {
            (Some(i), _) => Some(("chart.format.series", with_id(id, json!({"series": i, "line": v})))),
            (None, "categoryAxisTitle" | "valueAxisTitle") => None,
            (None, _) => Some(("chart.format.element", with_id(id, json!({"element": el, "line": v})))),
        }
    };
    let font_cmd = |v: Value| -> Option<(&'static str, Value)> {
        match (series, el.as_str()) {
            (Some(_), _) | (None, "plotArea" | "majorGridlines" | "minorGridlines" | "categoryMajorGridlines") => None,
            (None, _) => Some(("chart.format.element", with_id(id, json!({"element": el, "font": v})))),
        }
    };
    let sc = scheme(app);
    let t = Tokens::get(ui.ctx());
    widgets::group(ui, |ui| {
        widgets::rows(ui, |ui| {
            for (icon, label, which) in
                [(Icon::ShapeFill, "Shape Fill", 0), (Icon::ShapeOutline, "Shape Outline", 1), (Icon::TextFill, "Text Fill", 2)]
            {
                let enabled = match which {
                    0 => fill_cmd(json!("auto")).is_some(),
                    1 => line_cmd(json!("auto")).is_some(),
                    _ => font_cmd(json!({})).is_some(),
                };
                let (_, more) = widgets::color_split(ui, icon, label, t.text, enabled);
                widgets::color_popup(&more).show(|ui| {
                    let none = if which == 2 { "Automatic" } else { "No Fill" };
                    if let Some(pick) = widgets::color_grid(ui, &sc, Some(none)) {
                        let v = pick.as_ref().map(cref_param);
                        let cmd = match which {
                            0 => fill_cmd(v.unwrap_or(json!("none"))),
                            1 => line_cmd(v.map(|c| json!({"color": c})).unwrap_or(json!("none"))),
                            _ => font_cmd(json!({"color": v})),
                        };
                        if let Some((cmd, p)) = cmd {
                            run(app, cmd, p);
                        }
                    }
                    if which == 1 {
                        ui.separator();
                        ui.menu_button("Weight", |ui| {
                            for w in [0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.0, 4.5, 6.0] {
                                if ui.button(format!("{w} pt")).clicked()
                                    && let Some((cmd, p)) = line_cmd(json!({"width": w}))
                                {
                                    run(app, cmd, p);
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button("Dashes", |ui| {
                            for d in Dash::PRESETS {
                                if ui.button(d).clicked()
                                    && let Some((cmd, p)) = line_cmd(json!({"dash": d}))
                                {
                                    run(app, cmd, p);
                                    ui.close();
                                }
                            }
                        });
                    }
                });
            }
        });
    });
    // Text size and weight of the selected element.
    let text = element_text(&c, &el);
    widgets::group(ui, |ui| {
        widgets::rows(ui, |ui| {
            ui.horizontal(|ui| {
                let enabled = font_cmd(json!({})).is_some();
                ui.add_enabled_ui(enabled, |ui| {
                    ui.label("Size");
                    let size = text.as_ref().and_then(|t| t.size).unwrap_or(10.0);
                    if let Some(nv) = widgets::spinner(ui, "cf-size", size, 1.0, " pt", 44.0)
                        && let Some((cmd, p)) = font_cmd(json!({"size": nv.clamp(1.0, 400.0)}))
                    {
                        run(app, cmd, p);
                    }
                });
            });
            ui.horizontal(|ui| {
                let enabled = font_cmd(json!({})).is_some();
                let b = text.as_ref().and_then(|t| t.bold).unwrap_or(false);
                if widgets::icon_toggle(ui, Icon::Bold, "Bold", enabled, b).clicked()
                    && let Some((cmd, p)) = font_cmd(json!({"bold": !b}))
                {
                    run(app, cmd, p);
                }
                let i = text.as_ref().and_then(|t| t.italic).unwrap_or(false);
                if widgets::icon_toggle(ui, Icon::Italic, "Italic", enabled, i).clicked()
                    && let Some((cmd, p)) = font_cmd(json!({"italic": !i}))
                {
                    run(app, cmd, p);
                }
            });
        });
    });
}

/// The font an element holds, when it has text.
fn element_text(c: &Chart, el: &str) -> Option<ChartText> {
    let f = &c.format;
    Some(match el {
        "chartArea" => ChartText { size: c.text_size, ..f.text.clone() },
        "title" => ChartText { size: c.title_size, ..f.title_text.clone() },
        "legend" => f.legend_text.clone(),
        "dataLabels" => f.labels.text.clone(),
        "categoryAxis" => f.cat_axis.text.clone(),
        "valueAxis" => f.val_axis.text.clone(),
        "categoryAxisTitle" => f.cat_axis.title_text.clone(),
        "valueAxisTitle" => f.val_axis.title_text.clone(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use deckcraft_engine::Session;

    use super::*;
    use crate::Services;

    /// Every chart element's options and both chart tabs draw, for every kind of chart.
    #[test]
    fn every_element_pane_and_chart_tab_draws() {
        for kind in [
            "column",
            "bar",
            "line",
            "pie",
            "doughnut",
            "area",
            "scatter",
            "stackedColumn",
            "explodedPie",
            "barOfPie",
            "bubble",
            "radarMarkers",
            "filledRadar",
            "stockVohlc",
            "column3d",
            "columnLineSecondary",
        ] {
            let mut harness = egui_kittest::Harness::builder().with_size(vec2(1400.0, 900.0)).build_ui_state(
                |ui, app: &mut SlideApp| {
                    app.logic(ui.ctx());
                    app.ui(ui);
                },
                SlideApp::new(Session::with_new(), Services::default()),
            );
            harness.run_steps(1);
            let id = harness.state_mut().run("insert.chart", json!({"type": kind})).unwrap()["id"].as_u64().unwrap();
            harness.state_mut().run("edit.select", json!({"ids": [id]})).unwrap();
            harness.state_mut().dialog = None;
            harness.state_mut().run("chart.format.labels", json!({"show": true})).unwrap();
            let (c, _) = selected_chart(harness.state()).unwrap();
            assert_eq!(elements(&c).iter().any(|(k, _)| k == "valueAxis"), !pie(&c));
            for tab in ["Chart Design", "Chart Format"] {
                harness.state_mut().run("view.tab", json!({"tab": tab})).unwrap();
                for (el, _) in elements(&c) {
                    harness.state_mut().run("view.pane", json!({"pane": "format"})).unwrap();
                    set_element(harness.state_mut(), &el);
                    harness.run_steps(1);
                    assert_eq!(element(harness.state()), el);
                }
            }
        }
    }
}
