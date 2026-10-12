//! Chart data (Chart Design ▸ Data): the data table of a chart, cell by cell, as the Edit Data
//! window shows it. Rows are points (row 0 holds the series names), columns depend on the chart
//! type (see [`deckcraft_model::chart_data`]): categories and values; X and Y values for scatter
//! charts; X and Y values and sizes for bubble charts.
//!
//! Each command edits one chart: `id`, or the first chart in the selection. A command that
//! changes nothing leaves no undo step. With `undoGroup`, successive edits (the keystrokes of one
//! cell) make one undo step (see [`crate::Session::execute`]).

use deckcraft_model::chart::Series;
use deckcraft_model::chart_data::{DataShape, format_number};
use deckcraft_model::{Chart, Shape, ShapeId, ShapeKind};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "chart.grid",
            "Chart Data Table",
            [],
            None,
            "{id?, sharedX?: bool (one X column for scatter/bubble series that share X values, default true)} → {shape: categories|xy|bubble, sharedX, columns: [{role: categories|x|values|sizes, series?, label?}], rows: [[text]] (row 0 = series names), notes: [text], ranges?: [formula]}",
            has_selection,
            grid
        ),
        cmd!(
            "chart.setCell",
            "Edit Data Cell",
            ["Chart Design", "Data"],
            None,
            "{row (0 = series names, n+1 adds a point), col, value: text|number|null (decimal comma accepted, empty = gap), sharedX?, id?, undoGroup?}",
            has_selection,
            set_cell
        ),
        cmd!(
            "chart.paste",
            "Paste Chart Data",
            ["Chart Design", "Data"],
            None,
            "{text: tab-separated rows (as copied from a spreadsheet), row?: 0, col?: 0, sharedX?, id?} → {rows, cols}; grows the table as needed",
            has_selection,
            paste
        ),
        cmd!(
            "chart.insertRows",
            "Insert Data Rows",
            ["Chart Design", "Data"],
            None,
            "{at?: point index (end by default), count?: 1, id?}",
            has_selection,
            insert_rows
        ),
        cmd!("chart.deleteRows", "Delete Data Rows", ["Chart Design", "Data"], None, "{at: point index, count?: 1, id?}", has_selection, delete_rows),
        cmd!("chart.moveRow", "Move Data Row", ["Chart Design", "Data"], None, "{from: point index, to: point index, id?}", has_selection, move_row),
        cmd!(
            "chart.insertSeries",
            "Insert Series",
            ["Chart Design", "Data"],
            None,
            "{at?: series index (end by default), name?, id?} → {index}",
            has_selection,
            insert_series
        ),
        cmd!(
            "chart.deleteSeries",
            "Delete Series",
            ["Chart Design", "Data"],
            None,
            "{at: series index, count?: 1, id?}",
            has_selection,
            delete_series
        ),
        cmd!(
            "chart.moveSeries",
            "Move Series",
            ["Chart Design", "Data"],
            None,
            "{from: series index, to: series index, id?}",
            has_selection,
            move_series
        ),
        cmd!("chart.switchRowColumn", "Switch Row/Column", ["Chart Design", "Data"], None, "{id?}", has_selection, switch_row_column),
    ]
}

/// The chart a command edits: `id`, or the first selected chart.
pub(crate) fn chart_target(s: &Session, p: &Value, cmd: &str) -> Result<(ShapeId, Chart)> {
    let st = s.doc()?;
    let shape = match id_param(p, "id") {
        Some(id) => st.shape(id),
        None => st.selected_shapes().into_iter().find(|sh| matches!(sh.kind, ShapeKind::Chart(_))),
    };
    match shape {
        Some(Shape { id, kind: ShapeKind::Chart(c), .. }) => Ok((*id, (**c).clone())),
        Some(_) => Err(bad(cmd, "not a chart")),
        None => Err(bad(cmd, "select a chart")),
    }
}

/// Apply `f` to the target chart. No change, no edit (and no undo step).
fn edit_chart(s: &mut Session, p: &Value, cmd: &str, f: impl FnOnce(&mut Chart) -> std::result::Result<Value, String>) -> Result<Value> {
    let (id, before) = chart_target(s, p, cmd)?;
    let mut c = before.clone();
    let out = f(&mut c).map_err(|e| bad(cmd, e))?;
    if c == before {
        return Ok(out);
    }
    // The chart part is written again from the model; `source` keeps the workbook ranges.
    c.raw = None;
    edit_shapes(s, &json!({"id": id.0}), cmd, |sh| {
        sh.kind = ShapeKind::Chart(Box::new(c.clone()));
        Ok(())
    })?;
    Ok(out)
}

fn shared_x(p: &Value) -> bool {
    bool_param(p, "sharedX").unwrap_or(true)
}

fn required(p: &Value, key: &str, cmd: &str) -> Result<usize> {
    usize_param(p, key).ok_or_else(|| bad(cmd, format!("missing {key}")))
}

/// A cell value param as text: numbers as the table shows them, `null` as an empty cell.
fn value_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Number(n)) => n.as_f64().map(format_number).unwrap_or_default(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// Numbers of a JSON array (numbers, numeric text with a decimal comma, `null` for a gap).
pub(crate) fn numbers(v: Option<&Value>) -> Option<Vec<Option<f64>>> {
    let a = v?.as_array()?;
    Some(
        a.iter()
            .take(deckcraft_model::chart_data::MAX_POINTS)
            .map(|x| match x {
                Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
                Value::String(t) => deckcraft_model::chart_data::parse_number(t).ok().flatten(),
                _ => None,
            })
            .collect(),
    )
}

/// The series of a stock chart given by role (`volume`, `open`, `high`, `low`, `close`: arrays of
/// numbers), in the order stock charts take them: volume, open, high, low, close. `None` when no
/// role is given.
pub(crate) fn stock_series(p: &Value) -> Option<Vec<Value>> {
    let roles = [("volume", "Volume"), ("open", "Open"), ("high", "High"), ("low", "Low"), ("close", "Close")];
    let list: Vec<Value> =
        roles.iter().filter_map(|(key, name)| p.get(*key).filter(|v| v.is_array()).map(|v| json!({"name": name, "values": v}))).collect();
    (!list.is_empty()).then_some(list)
}

/// A series of `chart.data` / `insert.chart` params: `{name, values, xValues?, sizes?}` over the
/// series `base` it replaces (formatting and data it doesn't give are kept).
pub(crate) fn series_from(v: &Value, base: Option<Series>) -> Series {
    let mut s = base.unwrap_or_default();
    s.name = v.get("name").and_then(Value::as_str).unwrap_or("Series").to_string();
    s.values = numbers(v.get("values")).unwrap_or_default();
    if let Some(x) = numbers(v.get("xValues").or_else(|| v.get("x"))) {
        s.x_values = x;
    }
    if let Some(z) = numbers(v.get("sizes")) {
        s.sizes = z;
    }
    s
}

fn grid(s: &mut Session, p: &Value) -> Result<Value> {
    let (_, c) = chart_target(s, p, "chart.grid")?;
    let shared = shared_x(p) && c.kind.data_shape() != DataShape::Categories && c.shares_x();
    let columns: Vec<Value> = c
        .data_columns(shared)
        .into_iter()
        .map(|col| {
            let mut v = serde_json::to_value(col).unwrap_or(Value::Null);
            if let (Some(label), Some(o)) = (c.column_role(col), v.as_object_mut()) {
                o.insert("label".into(), Value::from(label));
            }
            v
        })
        .collect();
    let ranges: Option<Vec<Value>> = c.source.as_ref().map(|src| {
        src.series
            .iter()
            .flat_map(|r| [&r.name_ref, &r.categories_ref, &r.values_ref, &r.sizes_ref])
            .flatten()
            .map(|f| Value::from(f.as_str()))
            .collect()
    });
    let mut out = json!({
        "shape": c.kind.data_shape(),
        "sharedX": shared,
        "columns": columns,
        "rows": c.data_grid(shared),
        "notes": c.data_notes(),
    });
    if let (Some(r), Some(o)) = (ranges, out.as_object_mut()) {
        o.insert("ranges".into(), Value::Array(r));
    }
    Ok(out)
}

fn set_cell(s: &mut Session, p: &Value) -> Result<Value> {
    let row = required(p, "row", "chart.setCell")?;
    let col = required(p, "col", "chart.setCell")?;
    let text = value_text(p.get("value"));
    let shared = shared_x(p);
    edit_chart(s, p, "chart.setCell", |c| c.set_cell(row, col, shared, &text, false).map(|_| Value::Null))
}

fn paste(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("chart.paste", "missing text"))?.to_string();
    let row = usize_param(p, "row").unwrap_or(0);
    let col = usize_param(p, "col").unwrap_or(0);
    let shared = shared_x(p);
    edit_chart(s, p, "chart.paste", |c| c.paste_tsv(row, col, shared, &text).map(|(rows, cols)| json!({"rows": rows, "cols": cols})))
}

fn insert_rows(s: &mut Session, p: &Value) -> Result<Value> {
    let at = usize_param(p, "at").unwrap_or(usize::MAX);
    let count = usize_param(p, "count").unwrap_or(1).max(1);
    edit_chart(s, p, "chart.insertRows", |c| c.insert_points(at, count).map(|_| Value::Null))
}

fn delete_rows(s: &mut Session, p: &Value) -> Result<Value> {
    let at = required(p, "at", "chart.deleteRows")?;
    let count = usize_param(p, "count").unwrap_or(1).max(1);
    edit_chart(s, p, "chart.deleteRows", |c| c.remove_points(at, count).map(|_| Value::Null))
}

fn move_row(s: &mut Session, p: &Value) -> Result<Value> {
    let from = required(p, "from", "chart.moveRow")?;
    let to = required(p, "to", "chart.moveRow")?;
    edit_chart(s, p, "chart.moveRow", |c| c.move_point(from, to).map(|_| Value::Null))
}

fn insert_series(s: &mut Session, p: &Value) -> Result<Value> {
    let at = usize_param(p, "at").unwrap_or(usize::MAX);
    let name = str_param(p, "name").map(String::from);
    edit_chart(s, p, "chart.insertSeries", |c| c.insert_series(at, name).map(|i| json!({"index": i})))
}

fn delete_series(s: &mut Session, p: &Value) -> Result<Value> {
    let at = required(p, "at", "chart.deleteSeries")?;
    let count = usize_param(p, "count").unwrap_or(1).max(1);
    edit_chart(s, p, "chart.deleteSeries", |c| c.remove_series(at, count).map(|_| Value::Null))
}

fn move_series(s: &mut Session, p: &Value) -> Result<Value> {
    let from = required(p, "from", "chart.moveSeries")?;
    let to = required(p, "to", "chart.moveSeries")?;
    edit_chart(s, p, "chart.moveSeries", |c| c.move_series(from, to).map(|_| Value::Null))
}

fn switch_row_column(s: &mut Session, p: &Value) -> Result<Value> {
    edit_chart(s, p, "chart.switchRowColumn", |c| c.switch_rows_columns().map(|_| Value::Null))
}

#[cfg(test)]
mod tests {
    use deckcraft_model::chart::ChartType;
    use serde_json::json;

    use crate::Session;

    fn session_with(kind: &str) -> (Session, u64) {
        let mut s = Session::with_new();
        let id = s.execute("insert.chart", &json!({"type": kind})).unwrap()["id"].as_u64().unwrap();
        (s, id)
    }

    fn chart(s: &Session, id: u64) -> deckcraft_model::Chart {
        match &s.doc().unwrap().shape(deckcraft_model::ShapeId(id as u32)).unwrap().kind {
            deckcraft_model::ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("not a chart"),
        }
    }

    fn undo_depth(s: &Session) -> usize {
        s.doc().unwrap().history.undo.len()
    }

    #[test]
    fn cells_edit_one_undo_step_per_group() {
        let (mut s, id) = session_with("column");
        let base = undo_depth(&s);
        for text in ["1", "12", "12,", "12,5"] {
            s.execute("chart.setCell", &json!({"id": id, "row": 1, "col": 1, "value": text, "undoGroup": "cell-1-1-a"})).unwrap();
        }
        assert_eq!(chart(&s, id).series[0].values[0], Some(12.5));
        assert_eq!(undo_depth(&s), base + 1);
        // Another cell, another step.
        s.execute("chart.setCell", &json!({"id": id, "row": 0, "col": 2, "value": "Costs", "undoGroup": "cell-0-2-b"})).unwrap();
        assert_eq!(undo_depth(&s), base + 2);
        assert_eq!(chart(&s, id).series[1].name, "Costs");
        // The same value again changes nothing: no step.
        s.execute("chart.setCell", &json!({"id": id, "row": 0, "col": 2, "value": "Costs"})).unwrap();
        assert_eq!(undo_depth(&s), base + 2);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(chart(&s, id).series[1].name, "Series 2");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(chart(&s, id).series[0].values[0], Some(4.2));
        // After an undo the same group starts a new step.
        s.execute("chart.setCell", &json!({"id": id, "row": 1, "col": 1, "value": 3, "undoGroup": "cell-1-1-a"})).unwrap();
        assert_eq!(undo_depth(&s), base + 1);
        assert!(s.execute("chart.setCell", &json!({"id": id, "row": 1, "col": 1, "value": "abc"})).is_err());
        assert!(s.execute("chart.setCell", &json!({"id": id, "row": 1})).is_err());
    }

    #[test]
    fn rows_and_series_commands() {
        let (mut s, id) = session_with("line");
        s.execute("chart.insertRows", &json!({"id": id})).unwrap();
        s.execute("chart.insertRows", &json!({"id": id, "at": 0, "count": 2})).unwrap();
        assert_eq!(chart(&s, id).categories, vec!["Category 1", "Category 2", "Q1", "Q2", "Q3", "Q4", "Category 5"]);
        s.execute("chart.deleteRows", &json!({"id": id, "at": 0, "count": 2})).unwrap();
        s.execute("chart.moveRow", &json!({"id": id, "from": 4, "to": 0})).unwrap();
        assert_eq!(chart(&s, id).categories[0], "Category 5");
        let r = s.execute("chart.insertSeries", &json!({"id": id, "name": "Target"})).unwrap();
        assert_eq!(r["index"], 3);
        s.execute("chart.moveSeries", &json!({"id": id, "from": 3, "to": 0})).unwrap();
        s.execute("chart.deleteSeries", &json!({"id": id, "at": 1})).unwrap();
        let c = chart(&s, id);
        assert_eq!(c.series.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Target", "Series 2", "Series 3"]);
        assert!(c.series.iter().all(|x| x.values.len() == 5));
        assert!(s.execute("chart.deleteRows", &json!({"id": id, "at": 99})).is_err());
        assert!(s.execute("chart.deleteSeries", &json!({"id": id})).is_err());
        s.execute("chart.switchRowColumn", &json!({"id": id})).unwrap();
        let c = chart(&s, id);
        assert_eq!(c.categories, vec!["Target", "Series 2", "Series 3"]);
        assert_eq!(c.series.len(), 5);
    }

    #[test]
    fn paste_and_grid_for_every_data_shape() {
        let (mut s, id) = session_with("column");
        let r = s.execute("chart.paste", &json!({"id": id, "text": "\tA\tB\tC\tD\nx\t1\t2\t3\t4\ny\t5\t6\t7\t8\n"})).unwrap();
        assert_eq!(r, json!({"rows": 3, "cols": 5}));
        let g = s.execute("chart.grid", &json!({"id": id})).unwrap();
        assert_eq!(g["shape"], "categories");
        assert_eq!(g["rows"][0], json!(["", "A", "B", "C", "D"]));
        assert_eq!(g["rows"][2], json!(["y", "5", "6", "7", "8"]));
        assert_eq!(g["columns"][0], json!({"role": "categories", "label": "Categories"}));
        assert_eq!(g["columns"][4], json!({"role": "values", "series": 3}));

        let (mut s, id) = session_with("scatter");
        let g = s.execute("chart.grid", &json!({"id": id})).unwrap();
        assert_eq!(g["sharedX"], true);
        assert_eq!(g["columns"][0], json!({"role": "x", "label": "X values"}));
        assert_eq!(g["columns"][1], json!({"role": "values", "series": 0, "label": "Y values"}));
        s.execute("chart.setCell", &json!({"id": id, "row": 2, "col": 2, "value": "7,5", "sharedX": false})).unwrap();
        let c = chart(&s, id);
        assert_eq!(c.series[0].x_values[1], Some(2.0));
        assert_eq!(c.series[1].x_values[1], Some(7.5));
        let g = s.execute("chart.grid", &json!({"id": id})).unwrap();
        assert_eq!(g["sharedX"], false);
        assert_eq!(g["columns"].as_array().unwrap().len(), 6);

        let (mut s, id) = session_with("bubble");
        let g = s.execute("chart.grid", &json!({"id": id})).unwrap();
        assert_eq!(g["shape"], "bubble");
        assert_eq!(g["columns"][2], json!({"role": "sizes", "series": 0, "label": "Size"}));
        s.execute("chart.setCell", &json!({"id": id, "row": 1, "col": 2, "value": 6})).unwrap();
        assert_eq!(chart(&s, id).series[0].sizes[0], Some(6.0));

        let (mut s, id) = session_with("stock");
        s.execute("chart.insertSeries", &json!({"id": id})).unwrap();
        let g = s.execute("chart.grid", &json!({"id": id})).unwrap();
        let labels: Vec<_> = g["columns"].as_array().unwrap().iter().map(|c| c["label"].clone()).collect();
        assert_eq!(labels, [json!("Categories"), json!("Open"), json!("High"), json!("Low"), json!("Close")]);
        assert_eq!(chart(&s, id).kind, ChartType::Stock);
    }

    #[test]
    fn chart_data_takes_x_values_sizes_and_text_numbers() {
        let (mut s, id) = session_with("bubble");
        s.execute(
            "chart.data",
            &json!({"id": id, "series": [{"name": "S", "values": ["1,5", 2, null], "xValues": [10, 20, 30], "sizes": [1, 2, 3]}]}),
        )
        .unwrap();
        let c = chart(&s, id);
        assert_eq!(c.series.len(), 1);
        assert_eq!(c.series[0].values, vec![Some(1.5), Some(2.0), None]);
        assert_eq!(c.series[0].x_values, vec![Some(10.0), Some(20.0), Some(30.0)]);
        assert_eq!(c.series[0].sizes, vec![Some(1.0), Some(2.0), Some(3.0)]);
    }

    #[test]
    fn stock_data_by_role() {
        let mut s = Session::with_new();
        let p = json!({"type": "stock", "categories": ["Mon", "Tue"], "volume": [120, 90], "open": [44, 46], "high": [47, "48,5"], "low": [43, 45], "close": [46, 45]});
        let id = s.execute("insert.chart", &p).unwrap()["id"].as_u64().unwrap();
        let c = chart(&s, id);
        assert_eq!(c.series.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Volume", "Open", "High", "Low", "Close"]);
        assert_eq!(c.series[2].values, vec![Some(47.0), Some(48.5)]);
        s.execute("chart.data", &json!({"id": id, "high": [50, 51], "low": [40, 41], "close": [45, 46]})).unwrap();
        let c = chart(&s, id);
        assert_eq!(c.series.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["High", "Low", "Close"]);
        assert_eq!(c.series[0].values, vec![Some(50.0), Some(51.0)]);
    }

    #[test]
    fn data_commands_need_a_chart() {
        let mut s = Session::with_new();
        let shape = s.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 50, 50]})).unwrap()["id"].as_u64().unwrap();
        assert!(s.execute("chart.setCell", &json!({"id": shape, "row": 1, "col": 1, "value": 1})).is_err());
        assert!(s.execute("chart.grid", &json!({"id": shape})).is_err());
    }
}
