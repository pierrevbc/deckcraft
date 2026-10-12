//! The chart data editor (Chart Design ▸ Edit Data): the chart's data as a small spreadsheet,
//! updated live (#195). The columns follow the chart type (categories and values; X and Y values;
//! X and Y values and sizes; stock and combo roles), see [`deckcraft_model::chart_data`].
//!
//! Every edit is a `chart.*` command: the chart changes at each key, and the keystrokes of one
//! cell make one undo step (`undoGroup`). A block of cells pasted from a spreadsheet goes in at the
//! current cell and grows the table. The grid reads the chart each frame, so undo, redo and other
//! commands show at once; only the text of a cell that isn't a number yet is kept here.

use deckcraft_model::chart::ChartType;
use deckcraft_model::chart_data::{DataColumn, DataShape, parse_number};
use deckcraft_model::{Chart, ShapeId, ShapeKind};
use egui::{Align, Align2, CornerRadius, Id, Key, Modifiers, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::dialogs::Dialog;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};
use crate::{SlideApp, widgets};

const ROW_H: f32 = 22.0;
const HEAD_W: f32 = 36.0;
const CAT_W: f32 = 116.0;
const NUM_W: f32 = 86.0;

/// The editor's own state, kept in the dialog params between frames.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct State {
    /// The current cell (grid row, column): the row and series the toolbar acts on.
    cur: Option<(usize, usize)>,
    /// The cell being typed in and its text, kept while it isn't a number yet.
    edit: Option<(usize, usize)>,
    buf: String,
    /// Bumped when a cell or the title gains focus: one undo step per edit.
    group: u64,
    /// One X column per series in scatter and bubble charts.
    own_x: bool,
    /// Give this cell the focus when it is drawn.
    focus: Option<(usize, usize)>,
}

/// The chart the editor shows: the one it was opened for, else the first selected chart.
fn find_chart(app: &SlideApp, d: &Dialog) -> Option<(ShapeId, Chart)> {
    let st = app.session.active()?;
    let shape = match d.params.get("id").and_then(Value::as_u64).and_then(|v| u32::try_from(v).ok()) {
        Some(id) => st.shape(ShapeId(id)),
        None => st.selected_shapes().into_iter().find(|s| matches!(s.kind, ShapeKind::Chart(_))),
    }?;
    match &shape.kind {
        ShapeKind::Chart(c) => Some((shape.id, (**c).clone())),
        _ => None,
    }
}

/// The series a column belongs to.
fn series_of(col: &DataColumn) -> Option<usize> {
    match *col {
        DataColumn::Values { series } | DataColumn::Sizes { series } | DataColumn::X { series: Some(series) } => Some(series),
        DataColumn::Categories | DataColumn::X { series: None } => None,
    }
}

/// Spreadsheet column letters for a 0-based column.
fn letters(mut i: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, char::from(b'A' + (i % 26) as u8));
        if i < 26 || s.len() >= 3 {
            return s;
        }
        i = i / 26 - 1;
    }
}

fn surrender_focus(ui: &Ui) {
    ui.memory_mut(|m| {
        if let Some(f) = m.focused() {
            m.surrender_focus(f);
        }
    });
}

/// A separator between toolbar groups, as tall as the buttons.
fn vsep(ui: &mut Ui, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(13.0, 22.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range().shrink(3.0), Stroke::new(1.0, t.separator));
}

/// A toolbar icon button.
fn tool(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool) -> bool {
    widgets::icon_toggle(ui, icon, tip, enabled, false).clicked() && enabled
}

/// The editor's body; returns whether to close it.
pub fn body(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    if !d.params.is_object() {
        d.params = json!({});
    }
    let Some((id, _)) = find_chart(app, d) else {
        ui.label("Select a chart first.");
        ui.add_space(8.0);
        return ui.button("Close").clicked();
    };
    d.params["id"] = json!(id.0);
    let mut st: State = d.params.get("state").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    let t = Tokens::get(ui.ctx());

    // Keys the editor takes before the cells see them: undo and redo of the chart, and blocks of
    // cells pasted from a spreadsheet.
    let redo = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y));
    let undo = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z));
    if undo || redo {
        let _ = app.run(if undo { "edit.undo" } else { "edit.redo" }, json!({}));
        st.edit = None;
        surrender_focus(ui);
    }
    if let Some((r, col)) = st.edit {
        let block = ui.input_mut(|i| {
            let mut found = None;
            i.events.retain(|e| match e {
                egui::Event::Paste(text) if found.is_none() && text.contains(['\t', '\n']) => {
                    found = Some(text.clone());
                    false
                }
                _ => true,
            });
            found
        });
        if let Some(text) = block {
            let _ = app.run("chart.paste", json!({"id": id.0, "row": r, "col": col, "text": text, "sharedX": !st.own_x}));
            st.edit = None;
            surrender_focus(ui);
        }
    }
    let Some((id, c)) = find_chart(app, d) else { return true };
    let xy = c.kind.data_shape() != DataShape::Categories;
    let shared = !st.own_x;
    let cols = c.data_columns(shared);
    let n = c.point_count();

    // The banner of #195, in theme colours.
    egui::Frame::NONE.fill(t.accent).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(10, 5)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("GridCraft Spreadsheet Data").color(t.accent_text).strong());
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("Live chart sync").color(t.accent_text).small());
            });
        });
    });
    ui.add_space(6.0);

    // Chart title (#195), one undo step per edit.
    ui.horizontal(|ui| {
        ui.label(RichText::new("Chart Title:").strong());
        let mut title = c.title.clone().unwrap_or_default();
        let te = ui.add(egui::TextEdit::singleline(&mut title).hint_text("Enter title…").desired_width((ui.available_width() - 8.0).max(80.0)));
        if te.gained_focus() {
            st.group += 1;
        }
        if te.changed() {
            let _ = app.run("chart.title", json!({"id": id.0, "title": title, "undoGroup": format!("title-{}-{}", id.0, st.group)}));
        }
    });
    ui.add_space(2.0);
    // Chart type quick selector (#195).
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Type:").strong());
        for kind in ChartType::FIRST_FIVE {
            if ui.selectable_label(c.kind == kind, kind.label()).clicked() {
                let _ = app.run("chart.type", json!({"id": id.0, "type": format!("{kind:?}")}));
            }
        }
    });
    ui.separator();

    // Toolbar: rows (points), series, switch, copy, undo.
    let point = st.cur.map(|(r, _)| r).filter(|r| (1..=n).contains(r)).map(|r| r - 1);
    let series = st.cur.and_then(|(_, col)| cols.get(col)).and_then(series_of);
    let ns = c.series.len();
    let can_undo = app.session.active().is_some_and(|d| !d.history.undo.is_empty());
    let can_redo = app.session.active().is_some_and(|d| !d.history.redo.is_empty());
    let mut action: Option<(&str, Value)> = None;
    let mut cur = st.cur;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.label(RichText::new("Rows").color(t.text_dim).small());
        if tool(ui, Icon::InsertAbove, "Insert Row Above", true) {
            action = Some(("chart.insertRows", json!({"at": point.unwrap_or(n)})));
        }
        if tool(ui, Icon::InsertBelow, "Insert Row Below", true) {
            action = Some(("chart.insertRows", json!({"at": point.map_or(n, |p| p + 1)})));
        }
        if tool(ui, Icon::Minus, "Delete Row", point.is_some() && n > 1)
            && let Some(p) = point
        {
            action = Some(("chart.deleteRows", json!({"at": p})));
        }
        if tool(ui, Icon::ChevronUp, "Move Row Up", point.is_some_and(|p| p > 0))
            && let (Some(p), Some((r, col))) = (point, cur)
        {
            action = Some(("chart.moveRow", json!({"from": p, "to": p - 1})));
            cur = Some((r - 1, col));
        }
        if tool(ui, Icon::ChevronDown, "Move Row Down", point.is_some_and(|p| p + 1 < n))
            && let (Some(p), Some((r, col))) = (point, cur)
        {
            action = Some(("chart.moveRow", json!({"from": p, "to": p + 1})));
            cur = Some((r + 1, col));
        }
        vsep(ui, &t);
        ui.label(RichText::new("Series").color(t.text_dim).small());
        if tool(ui, Icon::InsertLeft, "Insert Series Before", true) {
            action = Some(("chart.insertSeries", json!({"at": series.unwrap_or(ns)})));
        }
        if tool(ui, Icon::InsertRight, "Insert Series After", true) {
            action = Some(("chart.insertSeries", json!({"at": series.map_or(ns, |s| s + 1)})));
        }
        if tool(ui, Icon::Minus, "Delete Series", series.is_some() && ns > 1)
            && let Some(s) = series
        {
            action = Some(("chart.deleteSeries", json!({"at": s})));
        }
        if tool(ui, Icon::ChevronLeft, "Move Series Left", series.is_some_and(|s| s > 0))
            && let Some(s) = series
        {
            action = Some(("chart.moveSeries", json!({"from": s, "to": s - 1})));
            cur = None;
        }
        if tool(ui, Icon::ChevronRight, "Move Series Right", series.is_some_and(|s| s + 1 < ns))
            && let Some(s) = series
        {
            action = Some(("chart.moveSeries", json!({"from": s, "to": s + 1})));
            cur = None;
        }
        vsep(ui, &t);
        if widgets::small_button(ui, Icon::SwitchRowCol, "Switch Row/Column", !xy, false).clicked() && !xy {
            action = Some(("chart.switchRowColumn", json!({})));
            cur = None;
        }
        if tool(ui, Icon::Copy, "Copy the data (paste it in a spreadsheet)", true) {
            ui.ctx().copy_text(c.data_tsv(shared));
        }
        vsep(ui, &t);
        if tool(ui, Icon::Undo, "Undo", can_undo) {
            action = Some(("edit.undo", json!({})));
        }
        if tool(ui, Icon::Redo, "Redo", can_redo) {
            action = Some(("edit.redo", json!({})));
        }
    });
    if xy {
        let mut same = shared && c.shares_x();
        let r = ui.add_enabled(c.shares_x(), egui::Checkbox::new(&mut same, "Same X values for all series"));
        let r = if c.shares_x() { r } else { r.on_disabled_hover_text("The series have different X values") };
        if r.changed() {
            st.own_x = !same;
            st.edit = None;
        }
    }
    if let Some((cmd, mut p)) = action {
        if cmd.starts_with("chart.") {
            p["id"] = json!(id.0);
        }
        let _ = app.run(cmd, p);
        st.cur = cur;
        st.edit = None;
        surrender_focus(ui);
        d.params["state"] = serde_json::to_value(&st).unwrap_or(Value::Null);
        ui.ctx().request_repaint();
        return false;
    }
    ui.add_space(4.0);

    // The grid: column letters and roles, the series names row, one row per point, and an empty
    // row that adds a point when typed in.
    let widths: Vec<f32> = cols.iter().map(|c| if matches!(c, DataColumn::Categories) { CAT_W } else { NUM_W }).collect();
    let lines = n + 3;
    let grid_h = (ui.available_height() - 64.0).max(ROW_H * 4.0);
    let mut commit: Option<(usize, usize, String)> = None;
    let mut draw_line = |ui: &mut Ui, line: usize| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            if line == 0 {
                let (rect, _) = ui.allocate_exact_size(vec2(HEAD_W, ROW_H), Sense::hover());
                ui.painter().rect(rect, 0.0, t.chrome, Stroke::new(0.5, t.border), StrokeKind::Inside);
                for (ci, col) in cols.iter().enumerate() {
                    let w = widths.get(ci).copied().unwrap_or(NUM_W);
                    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
                    let on = st.cur.is_some_and(|(_, c)| c == ci);
                    ui.painter().rect(rect, 0.0, if on { t.selected } else { t.chrome }, Stroke::new(0.5, t.border), StrokeKind::Inside);
                    let role = c.column_role(*col).map(|r| format!("  {r}")).unwrap_or_default();
                    ui.painter().text(
                        rect.left_center() + vec2(5.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("{}{role}", letters(ci)),
                        theme::font(11.0),
                        t.text_dim,
                    );
                    if resp.clicked() {
                        st.cur = Some((st.cur.map_or(0, |(r, _)| r), ci));
                    }
                }
                return;
            }
            let r = line - 1;
            let (rect, resp) = ui.allocate_exact_size(vec2(HEAD_W, ROW_H), Sense::click());
            let on = st.cur.is_some_and(|(cr, _)| cr == r);
            ui.painter().rect(rect, 0.0, if on { t.selected } else { t.chrome }, Stroke::new(0.5, t.border), StrokeKind::Inside);
            let label = if r <= n { format!("{}", r + 1) } else { "+".into() };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, theme::font(11.0), t.text_dim);
            if resp.clicked() {
                st.cur = Some((r, st.cur.map_or(0, |(_, c)| c)));
            }
            for (ci, col) in cols.iter().enumerate() {
                let w = widths.get(ci).copied().unwrap_or(NUM_W);
                let (rect, _) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::hover());
                let header = r == 0 || matches!(col, DataColumn::Categories);
                ui.painter().rect(rect, 0.0, if header { t.canvas } else { t.card }, Stroke::new(0.5, t.border), StrokeKind::Inside);
                let editable = Chart::cell_editable(r, *col);
                let model = if r <= n { c.cell_text(r, *col) } else { String::new() };
                if !editable {
                    ui.painter().text(rect.left_center() + vec2(5.0, 0.0), Align2::LEFT_CENTER, model, theme::font(11.0), t.text_faint);
                    continue;
                }
                let typing = st.edit == Some((r, ci));
                let mut text = if typing { st.buf.clone() } else { model.clone() };
                let number = r > 0 && !matches!(col, DataColumn::Categories);
                let bad = typing && number && parse_number(&text).is_err();
                let te = egui::TextEdit::singleline(&mut text)
                    .id(Id::new(("chartcell", id.0, r, ci)))
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::symmetric(4, 2))
                    .horizontal_align(if number { Align::RIGHT } else { Align::LEFT })
                    .vertical_align(Align::Center)
                    .font(theme::font(12.0))
                    .text_color(if bad { icons::RED } else { t.text })
                    .desired_width(w - 8.0);
                let resp = ui.put(rect.shrink(1.0), te);
                if st.focus == Some((r, ci)) {
                    resp.request_focus();
                    resp.scroll_to_me(None);
                    st.focus = None;
                }
                if resp.gained_focus() {
                    st.edit = Some((r, ci));
                    st.buf = model.clone();
                    st.cur = Some((r, ci));
                    st.group += 1;
                }
                if resp.changed() {
                    st.edit = Some((r, ci));
                    st.buf = text.clone();
                    if !(number && parse_number(&text).is_err()) {
                        commit = Some((r, ci, text.clone()));
                    }
                }
                if resp.has_focus() {
                    let (down, up) = ui.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp)));
                    if down && r <= n {
                        st.focus = Some((r + 1, ci));
                    } else if up && r > 0 && Chart::cell_editable(r - 1, *col) {
                        st.focus = Some((r - 1, ci));
                    }
                }
                if resp.lost_focus() {
                    if st.edit == Some((r, ci)) {
                        st.edit = None;
                    }
                    if ui.input(|i| i.key_pressed(Key::Enter)) && r <= n {
                        st.focus = Some((r + 1, ci));
                    }
                }
                if st.cur == Some((r, ci)) {
                    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.5, t.accent), StrokeKind::Inside);
                }
            }
        });
    };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let area = egui::ScrollArea::both().id_salt(("chartgrid", id.0)).max_height(grid_h).auto_shrink([false, true]);
        // Small tables are drawn whole, so a cell below the view can take the focus (Enter);
        // large ones only where they show.
        if lines <= 400 {
            area.show(ui, |ui| (0..lines).for_each(|line| draw_line(ui, line)));
        } else {
            area.show_rows(ui, ROW_H, lines, |ui, range| range.for_each(|line| draw_line(ui, line)));
        }
    });
    if let Some((r, ci, text)) = commit {
        let group = format!("cell-{}-{r}-{ci}-{}", id.0, st.group);
        let _ = app.run("chart.setCell", json!({"id": id.0, "row": r, "col": ci, "value": text, "sharedX": shared, "undoGroup": group}));
    }

    // What the chart can't show, and a cell that isn't a number yet.
    let mut notes = c.data_notes();
    if let Some((r, ci)) = st.edit
        && r > 0
        && !matches!(cols.get(ci), Some(DataColumn::Categories) | None)
        && parse_number(&st.buf).is_err()
    {
        notes.push(format!("“{}” is not a number: the cell keeps its value until it is one.", st.buf.trim()));
    }
    ui.add_space(4.0);
    for note in notes {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            icons::paint(ui.painter(), rect, Icon::Warning, t.text_dim, false);
            ui.label(RichText::new(note).color(t.text_dim).small());
        });
    }
    ui.label(
        RichText::new("Type in a cell to update the chart. Paste cells copied from a spreadsheet at the current cell; Enter goes down.")
            .color(t.text_faint)
            .small(),
    );

    ui.separator();
    let mut close = false;
    ui.horizontal(|ui| {
        if ui
            .button(RichText::new("Open in GridCraft").strong())
            .on_hover_text("Export the data to CSV and launch the GridCraft spreadsheet")
            .clicked()
        {
            let _ = app.run("chart.gridcraft", json!({"id": id.0}));
            app.set_status("Chart data exported for GridCraft");
        }
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    d.params["state"] = serde_json::to_value(&st).unwrap_or(Value::Null);
    close
}

/// The bounds of the editor window.
pub fn window_size() -> (egui::Vec2, egui::Vec2) {
    (vec2(660.0, 470.0), vec2(460.0, 300.0))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use deckcraft_engine::Session;

    use super::*;
    use crate::Services;

    fn chart(app: &SlideApp, id: u32) -> Chart {
        match &app.session.active().unwrap().shape(ShapeId(id)).unwrap().kind {
            ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("not a chart"),
        }
    }

    fn undo_depth(app: &SlideApp) -> usize {
        app.session.active().unwrap().history.undo.len()
    }

    /// Typing in a cell, Enter, pasting a block and undo, through real egui input.
    #[test]
    fn typing_and_pasting_in_the_editor() {
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            SlideApp::new(Session::with_new(), Services::default()),
        );
        harness.run_steps(2);
        let id = harness.state_mut().run("insert.chart", json!({"type": "column", "editData": true})).unwrap()["id"].as_u64().unwrap() as u32;
        harness.run_steps(2);
        assert!(harness.state().dialog.as_ref().is_some_and(|d| d.id == "chartData"));
        let base = undo_depth(harness.state());

        // Cell B2 (Series 1, Q1): select all, type a number with a decimal comma.
        harness.ctx.memory_mut(|m| m.request_focus(Id::new(("chartcell", id, 1usize, 1usize))));
        harness.run_steps(2);
        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        for ch in ["9", ",", "5"] {
            harness.event(egui::Event::Text(ch.into()));
            harness.run_steps(1);
        }
        harness.run_steps(1);
        assert_eq!(chart(harness.state(), id).series[0].values[0], Some(9.5));
        assert_eq!(undo_depth(harness.state()), base + 1, "one undo step for the cell");
        // Text that isn't a number leaves the chart as it is.
        harness.event(egui::Event::Text("x".into()));
        harness.run_steps(2);
        assert_eq!(chart(harness.state(), id).series[0].values[0], Some(9.5));
        harness.key_press(Key::Backspace);
        harness.run_steps(1);
        // Enter goes down: the next cell takes the typing, another undo step.
        harness.key_press(Key::Enter);
        harness.run_steps(3);
        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        harness.event(egui::Event::Text("7".into()));
        harness.run_steps(2);
        assert_eq!(chart(harness.state(), id).series[0].values[1], Some(7.0));
        assert_eq!(undo_depth(harness.state()), base + 2);

        // A block copied from a spreadsheet goes in at the current cell and adds a point.
        harness.event(egui::Event::Paste(
            "1	2
3	4
5	6
7	8
"
            .into(),
        ));
        harness.run_steps(2);
        let c = chart(harness.state(), id);
        assert_eq!(c.point_count(), 5);
        assert_eq!(c.series[1].values[4], Some(8.0));
        assert_eq!(c.series[0].values[1], Some(1.0));

        // Undo in the editor undoes the chart.
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        harness.run_steps(2);
        assert_eq!(chart(harness.state(), id).point_count(), 4);
        assert_eq!(chart(harness.state(), id).series[0].values[1], Some(7.0));
    }

    #[test]
    fn column_letters() {
        assert_eq!([letters(0), letters(25), letters(26), letters(27), letters(701), letters(702)], ["A", "Z", "AA", "AB", "ZZ", "AAA"]);
    }

    #[test]
    fn columns_belong_to_series() {
        assert_eq!(series_of(&DataColumn::Sizes { series: 2 }), Some(2));
        assert_eq!(series_of(&DataColumn::X { series: None }), None);
        assert_eq!(series_of(&DataColumn::Categories), None);
    }
}
