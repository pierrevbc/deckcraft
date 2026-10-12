//! An imported chart keeps the workbook PowerPoint opens for Edit Data: as it was when only the
//! presentation of the chart changes, with just the changed cells rewritten when its data changes
//! (ranges and the table over them following added or removed series and points), and rebuilt
//! from the chart data only when the data can't be written where the workbook holds it.
//!
//! Decks and workbooks are built here, in code.

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use deckcraft_engine::{DocState, Session};
use deckcraft_geom::Xfrm;
use deckcraft_model::chart::{Chart, ChartType};
use deckcraft_model::{Presentation, Shape, ShapeId, ShapeKind, defaults};
use serde_json::json;
use zip::write::SimpleFileOptions;

const SHEET: &str = "xl/worksheets/sheet1.xml";

/// A workbook like the ones presentation apps embed with a chart, plus what a person adds to it:
/// a data sheet named `Chart data` with a table over the chart data, a total in F2, a formula in
/// D5 (one of the chart's values), a second sheet with a formula into the first, styles, shared
/// strings and a calculation chain.
fn workbook() -> Vec<u8> {
    let strings = [" ", "Series 1", "Series 2", "Series 3", "Q1", "Q2", "Q3", "Q4", "Total", "Notes for the chart"];
    let sst: String = strings.iter().map(|s| format!("<si><t xml:space=\"preserve\">{s}</t></si>")).collect();
    let values = [[4.2, 2.6, 2.1], [2.8, 4.1, 2.4], [3.6, 1.9, 3.3], [4.7, 3.0, 5.1]];
    let mut rows = String::from(
        r#"<row r="1" spans="1:6"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>2</v></c><c r="D1" t="s"><v>3</v></c><c r="F1" t="s"><v>8</v></c></row>"#,
    );
    for (i, row) in values.iter().enumerate() {
        let r = i + 2;
        rows.push_str(&format!(r#"<row r="{r}" spans="1:6"><c r="A{r}" t="s"><v>{}</v></c>"#, i + 4));
        for (j, v) in row.iter().enumerate() {
            let col = ["B", "C", "D"][j];
            if r == 5 && col == "D" {
                rows.push_str(r#"<c r="D5" s="1"><f>2*2.55</f><v>5.1</v></c>"#);
            } else {
                rows.push_str(&format!(r#"<c r="{col}{r}" s="1"><v>{v}</v></c>"#));
            }
        }
        if r == 2 {
            rows.push_str(r#"<c r="F2" s="1"><f>SUM(B2:B5)</f><v>15.3</v></c>"#);
        }
        rows.push_str("</row>");
    }
    let files: Vec<(&str, String)> = vec![
        (
            "[Content_Types].xml",
            concat!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
                "\n",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
                r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
                r#"<Default Extension="xml" ContentType="application/xml"/>"#,
                r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
                r#"<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
                r#"<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
                r#"<Override PartName="/xl/tables/table1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml"/>"#,
                r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#,
                r#"<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>"#,
                r#"<Override PartName="/xl/calcChain.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml"/>"#,
                "</Types>"
            )
            .into(),
        ),
        (
            "_rels/.rels",
            concat!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
                r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>"#,
                "</Relationships>"
            )
            .into(),
        ),
        (
            "xl/workbook.xml",
            concat!(
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
                r#"<sheets><sheet name="Chart data" sheetId="1" r:id="rId1"/><sheet name="Notes" sheetId="2" r:id="rId2"/></sheets>"#,
                r#"<calcPr calcId="191029"/></workbook>"#
            )
            .into(),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            concat!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
                r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>"#,
                r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>"#,
                r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
                r#"<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>"#,
                r#"<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain" Target="calcChain.xml"/>"#,
                "</Relationships>"
            )
            .into(),
        ),
        (
            SHEET,
            format!(
                concat!(
                    r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
                    r#"<dimension ref="A1:F5"/><sheetData>{}</sheetData><tableParts count="1"><tablePart r:id="rId1"/></tableParts></worksheet>"#
                ),
                rows
            ),
        ),
        (
            "xl/worksheets/_rels/sheet1.xml.rels",
            concat!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
                r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/>"#,
                "</Relationships>"
            )
            .into(),
        ),
        (
            "xl/worksheets/sheet2.xml",
            concat!(
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
                r#"<row r="1"><c r="A1" t="s"><v>9</v></c></row>"#,
                r#"<row r="2"><c r="A2"><f>'Chart data'!B2*2</f><v>8.4</v></c></row>"#,
                "</sheetData></worksheet>"
            )
            .into(),
        ),
        (
            "xl/tables/table1.xml",
            concat!(
                r#"<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="1" name="Table1" displayName="Table1" ref="A1:D5" totalsRowShown="0">"#,
                r#"<tableColumns count="4"><tableColumn id="1" name=" "/><tableColumn id="2" name="Series 1"/><tableColumn id="3" name="Series 2"/>"#,
                r#"<tableColumn id="4" name="Series 3"/></tableColumns><tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>"#
            )
            .into(),
        ),
        (
            "xl/styles.xml",
            concat!(
                r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
                r#"<fonts count="1"><font><sz val="11"/></font></fonts>"#,
                r#"<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>"#,
                r#"<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>"#,
                r#"<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>"#,
                r#"<cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="2" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs>"#,
                r#"</styleSheet>"#
            )
            .into(),
        ),
        (
            "xl/sharedStrings.xml",
            format!(r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{0}" uniqueCount="{0}">{sst}</sst>"#, strings.len()),
        ),
        (
            "xl/calcChain.xml",
            r#"<calcChain xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><c r="F2" i="1"/><c r="D5"/><c r="A2" i="2"/></calcChain>"#.into(),
        ),
    ];
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        z.start_file(name, SimpleFileOptions::default()).expect("start");
        z.write_all(data.as_bytes()).expect("write");
    }
    z.finish().expect("finish").into_inner()
}

/// Entries of a zip package, with their bytes, in order.
fn entries(zip_bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(zip_bytes)).expect("zip");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).expect("entry");
            let mut data = vec![];
            f.read_to_end(&mut data).expect("read");
            (f.name().to_string(), data)
        })
        .collect()
}

fn part(zip_bytes: &[u8], name: &str) -> Option<String> {
    entries(zip_bytes).into_iter().find(|(n, _)| n == name).map(|(_, d)| String::from_utf8(d).expect("utf-8"))
}

/// The package with `name` holding `data` instead.
fn replace(zip_bytes: &[u8], name: &str, data: &[u8]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in entries(zip_bytes) {
        z.start_file(n.as_str(), SimpleFileOptions::default()).expect("start");
        z.write_all(if n == name { data } else { &d }).expect("write");
    }
    z.finish().expect("finish").into_inner()
}

/// A deck with one column chart, as another app would save it: its data on the `Chart data`
/// sheet of [`workbook`]. Returns the PPTX and the workbook.
fn imported_deck() -> (Vec<u8>, Vec<u8>) {
    let mut p = defaults::new_presentation(None);
    let id = ShapeId(p.alloc_id());
    let chart = Shape {
        id,
        name: "Chart".into(),
        xfrm: Some(Xfrm::new(40.0, 100.0, 400.0, 300.0)),
        kind: ShapeKind::Chart(Box::new(Chart::sample(ChartType::Column))),
        ..Default::default()
    };
    Arc::make_mut(&mut p.slides[0]).shapes.push(chart);
    let pptx = deckcraft_pptx::export(&p).expect("export");
    let xml = part(&pptx, "ppt/charts/chart1.xml").expect("chart part").replace("Sheet1!", "'Chart data'!");
    let book = workbook();
    let pptx = replace(&pptx, "ppt/charts/chart1.xml", xml.as_bytes());
    let pptx = replace(&pptx, "ppt/embeddings/workbook1.xlsx", &book);
    (pptx, book)
}

/// A session editing the imported deck, and the chart's id.
fn session(pptx: &[u8]) -> (Session, u32) {
    let p: Presentation = deckcraft_pptx::import(pptx).expect("import");
    let id = p.slides[0].shapes.iter().find(|s| matches!(s.kind, ShapeKind::Chart(_))).expect("chart").id.0;
    let mut s = Session::new();
    s.add_document(DocState::new(p, None, "Imported".into()));
    (s, id)
}

fn export(s: &Session) -> Vec<u8> {
    deckcraft_pptx::export(&s.doc().expect("doc").doc).expect("export")
}

fn embedded(pptx: &[u8]) -> Vec<u8> {
    let books: Vec<_> = entries(pptx).into_iter().filter(|(n, _)| n.starts_with("ppt/embeddings/")).collect();
    assert_eq!(books.len(), 1);
    books.into_iter().next().expect("workbook").1
}

fn data(s: &mut Session, id: u32, categories: &[&str], series: &[(&str, &[f64])]) {
    let series: Vec<_> = series.iter().map(|(name, values)| json!({"name": name, "values": values})).collect();
    s.execute("chart.data", &json!({"id": id, "categories": categories, "series": series})).expect("chart.data");
}

const S1: &[f64] = &[4.2, 2.8, 3.6, 4.7];
const S2: &[f64] = &[2.6, 4.1, 1.9, 3.0];
const S3: &[f64] = &[2.1, 2.4, 3.3, 5.1];
const QS: &[&str] = &["Q1", "Q2", "Q3", "Q4"];

#[test]
fn presentation_changes_keep_the_workbook_byte_for_byte() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    s.execute("chart.options", &json!({"id": id, "title": "Revenue", "legend": "r", "gridlines": false})).expect("options");
    s.execute("chart.type", &json!({"id": id, "type": "bar"})).expect("type");
    // The same numbers sent again are no change either.
    data(&mut s, id, QS, &[("Series 1", S1), ("Series 2", S2), ("Series 3", S3)]);
    let out = export(&s);
    assert_eq!(embedded(&out), book);
    let xml = part(&out, "ppt/charts/chart1.xml").expect("chart");
    assert!(xml.contains("Revenue") && xml.contains(r#"<c:barDir val="bar"/>"#), "{xml}");
    // The chart part still reads the data where the workbook has it.
    for f in ["'Chart data'!$B$1", "'Chart data'!$A$2:$A$5", "'Chart data'!$D$2:$D$5"] {
        assert!(xml.contains(&format!("<c:f>{f}</c:f>")), "{f}: {xml}");
    }
    assert!(!xml.contains("Sheet1!"));
}

#[test]
fn a_changed_value_rewrites_only_its_cell() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    data(&mut s, id, QS, &[("Series 1", &[4.2, 9.5, 3.6, 4.7]), ("Series 2", S2), ("Series 3", S3)]);
    let out = embedded(&export(&s));
    let (before, after) = (entries(&book), entries(&out));
    assert_eq!(before.iter().map(|(n, _)| n).collect::<Vec<_>>(), after.iter().map(|(n, _)| n).collect::<Vec<_>>());
    for ((name, a), (_, b)) in before.iter().zip(&after) {
        let (a, b) = (String::from_utf8_lossy(a), String::from_utf8_lossy(b));
        match name.as_str() {
            SHEET => assert_eq!(b, a.replace(r#"<c r="B3" s="1"><v>2.8</v></c>"#, r#"<c r="B3" s="1"><v>9.5</v></c>"#)),
            // Formulas elsewhere (F2, the Notes sheet) are recalculated when the workbook opens.
            "xl/workbook.xml" => assert_eq!(b, a.replace(r#"<calcPr calcId="191029"/>"#, r#"<calcPr calcId="191029" fullCalcOnLoad="1"/>"#)),
            _ => assert_eq!(a, b, "{name}"),
        }
    }
    let sheet = part(&out, SHEET).expect("sheet");
    assert!(sheet.contains("<f>SUM(B2:B5)</f>") && sheet.contains("<f>2*2.55</f>"));
}

#[test]
fn a_renamed_series_renames_its_table_column() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    data(&mut s, id, &["Q1", "Q2", "Q3", "Spring"], &[("Series 1", S1), ("Costs & fees", S2), ("Series 3", S3)]);
    let out = embedded(&export(&s));
    let sheet = part(&out, SHEET).expect("sheet");
    assert!(sheet.contains(r#"<c r="C1" t="inlineStr"><is><t xml:space="preserve">Costs &amp; fees</t></is></c>"#), "{sheet}");
    assert!(sheet.contains(r#"<c r="A5" t="inlineStr"><is><t xml:space="preserve">Spring</t></is></c>"#), "{sheet}");
    let table = part(&out, "xl/tables/table1.xml").expect("table");
    assert_eq!(table, part(&book, "xl/tables/table1.xml").expect("table").replace(r#"name="Series 2""#, r#"name="Costs &amp; fees""#));
    assert_eq!(part(&out, "xl/sharedStrings.xml"), part(&book, "xl/sharedStrings.xml"));
}

#[test]
fn a_value_over_a_formula_replaces_it() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    data(&mut s, id, QS, &[("Series 1", S1), ("Series 2", S2), ("Series 3", &[2.1, 2.4, 3.3, 6.0])]);
    let out = embedded(&export(&s));
    let sheet = part(&out, SHEET).expect("sheet");
    assert_eq!(
        sheet,
        part(&book, SHEET).expect("sheet").replace(r#"<c r="D5" s="1"><f>2*2.55</f><v>5.1</v></c>"#, r#"<c r="D5" s="1"><v>6</v></c>"#)
    );
    // The calculation chain listed D5 as a formula: it goes, Excel rebuilds it.
    assert!(part(&out, "xl/calcChain.xml").is_none());
    assert!(!part(&out, "xl/_rels/workbook.xml.rels").expect("rels").contains("calcChain"));
    assert!(!part(&out, "[Content_Types].xml").expect("types").contains("calcChain"));
    assert_eq!(part(&out, "xl/worksheets/sheet2.xml"), part(&book, "xl/worksheets/sheet2.xml"));
}

/// The cells of a worksheet by reference (`B2`): the text or number they show (shared strings
/// looked up, formulas by their cached value), empty cells left out.
fn cells(book: &[u8], sheet: &str) -> std::collections::BTreeMap<String, String> {
    use quick_xml::events::Event;
    let read = |name: &str| part(book, name).unwrap_or_default();
    let mut strings = vec![];
    let sst = read("xl/sharedStrings.xml");
    let mut r = quick_xml::Reader::from_str(&sst);
    let mut cur: Option<String> = None;
    loop {
        match r.read_event().expect("xml") {
            Event::Start(e) if e.local_name().as_ref() == b"si" => cur = Some(String::new()),
            Event::Text(t) => {
                if let Some(c) = cur.as_mut() {
                    c.push_str(&t.decode().expect("text"));
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"si" => strings.push(cur.take().unwrap_or_default()),
            Event::Eof => break,
            _ => {}
        }
    }
    let xml = read(sheet);
    let mut r = quick_xml::Reader::from_str(&xml);
    let mut out = std::collections::BTreeMap::new();
    let (mut at, mut kind, mut text, mut inside) = (String::new(), String::new(), String::new(), false);
    loop {
        match r.read_event().expect("xml") {
            Event::Start(e) if e.local_name().as_ref() == b"c" => {
                let attr = |k: &[u8]| e.attributes().flatten().find(|a| a.key.as_ref() == k).map(|a| String::from_utf8_lossy(&a.value).into_owned());
                at = attr(b"r").unwrap_or_default();
                kind = attr(b"t").unwrap_or_default();
                text.clear();
            }
            Event::Start(e) if matches!(e.local_name().as_ref(), b"v" | b"t") => inside = true,
            Event::End(e) if matches!(e.local_name().as_ref(), b"v" | b"t") => inside = false,
            Event::Text(t) if inside => text.push_str(&t.decode().expect("text")),
            Event::End(e) if e.local_name().as_ref() == b"c" => {
                let v = if kind == "s" { text.parse::<usize>().ok().and_then(|i| strings.get(i).cloned()).unwrap_or_default() } else { text.clone() };
                if !v.is_empty() {
                    out.insert(at.clone(), v);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

fn cell(map: &std::collections::BTreeMap<String, String>, at: &str) -> String {
    map.get(at).cloned().unwrap_or_default()
}

/// The chart formulas of the exported chart part, in order.
fn formulas(pptx: &[u8]) -> Vec<String> {
    let xml = part(pptx, "ppt/charts/chart1.xml").expect("chart");
    xml.split("<c:f>").skip(1).filter_map(|s| s.split("</c:f>").next()).map(String::from).collect()
}

/// Everything in the workbook but the data sheet and the table is as it was (the workbook part
/// may only gain `fullCalcOnLoad`, the calculation chain may go).
fn rest_unchanged(before: &[u8], after: &[u8]) {
    for (name, data) in entries(before) {
        if [SHEET, "xl/tables/table1.xml", "xl/workbook.xml", "xl/calcChain.xml", "xl/_rels/workbook.xml.rels", "[Content_Types].xml"]
            .contains(&name.as_str())
        {
            continue;
        }
        assert_eq!(entries(after).into_iter().find(|(n, _)| *n == name).map(|(_, d)| d), Some(data), "{name}");
    }
}

#[test]
fn an_added_series_goes_next_to_the_last_one() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    data(&mut s, id, QS, &[("Series 1", S1), ("Series 2", S2), ("Series 3", S3), ("Margin", &[1.5, 2.5, 3.5, 4.5])]);
    let out = export(&s);
    let wb = embedded(&out);
    rest_unchanged(&book, &wb);
    let c = cells(&wb, SHEET);
    assert_eq!([cell(&c, "E1"), cell(&c, "E2"), cell(&c, "E5")], ["Margin", "1.5", "4.5"]);
    assert_eq!(cell(&c, "F1"), "Total");
    // The table over the chart data takes the new column, named after the series.
    let table = part(&wb, "xl/tables/table1.xml").expect("table");
    assert!(table.contains(r#"ref="A1:E5""#) && table.contains(r#"<tableColumn id="5" name="Margin"/>"#), "{table}");
    assert!(table.contains(r#"<tableColumns count="5">"#), "{table}");
    let f = formulas(&out);
    assert!(f.contains(&"'Chart data'!$E$1".to_string()) && f.contains(&"'Chart data'!$E$2:$E$5".to_string()), "{f:?}");
    assert!(f.contains(&"'Chart data'!$D$2:$D$5".to_string()), "{f:?}");
}

#[test]
fn a_series_where_the_workbook_holds_something_else_rebuilds_it() {
    // A fifth series would go to column F, where the workbook has a total.
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    data(&mut s, id, QS, &[("Series 1", S1), ("Series 2", S2), ("Series 3", S3), ("Series 4", S1), ("Series 5", S2)]);
    let out = export(&s);
    assert_ne!(embedded(&out), book);
    assert!(formulas(&out).contains(&"Sheet1!$F$1".to_string()));
    let names: Vec<String> = entries(&embedded(&out)).into_iter().map(|(n, _)| n).collect();
    assert!(!names.iter().any(|n| n.contains("sheet2") || n.contains("table")), "{names:?}");
}

#[test]
fn added_points_extend_the_ranges() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    data(&mut s, id, &["Q1", "Q2", "Q3", "Q4", "Q5"], &[("Series 1", &[4.2, 2.8, 3.6, 4.7, 1.0][..]), ("Series 2", S2), ("Series 3", S3)]);
    let out = export(&s);
    let wb = embedded(&out);
    rest_unchanged(&book, &wb);
    let c = cells(&wb, SHEET);
    assert_eq!([cell(&c, "A6"), cell(&c, "B6"), cell(&c, "C6"), cell(&c, "D6")], ["Q5", "1", "", ""]);
    // The formulas in the sheet are kept.
    let sheet = part(&wb, SHEET).expect("sheet");
    assert!(sheet.contains("<f>SUM(B2:B5)</f>") && sheet.contains("<f>2*2.55</f>"), "{sheet}");
    assert!(part(&wb, "xl/tables/table1.xml").expect("table").contains(r#"ref="A1:D6""#));
    let f = formulas(&out);
    for want in ["'Chart data'!$A$2:$A$6", "'Chart data'!$B$2:$B$6", "'Chart data'!$D$2:$D$6", "'Chart data'!$B$1"] {
        assert!(f.contains(&want.to_string()), "{want}: {f:?}");
    }
}

#[test]
fn removed_series_and_points_clear_their_cells() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    s.execute("chart.deleteSeries", &json!({"id": id, "at": 2})).expect("delete series");
    s.execute("chart.deleteRows", &json!({"id": id, "at": 3})).expect("delete row");
    let out = export(&s);
    let wb = embedded(&out);
    rest_unchanged(&book, &wb);
    let c = cells(&wb, SHEET);
    for gone in ["D1", "D2", "D4", "D5", "A5", "B5", "C5"] {
        assert_eq!(cell(&c, gone), "", "{gone}");
    }
    assert_eq!([cell(&c, "A4"), cell(&c, "C4"), cell(&c, "F1"), cell(&c, "F2")], ["Q3", "1.9", "Total", "15.3"]);
    let table = part(&wb, "xl/tables/table1.xml").expect("table");
    assert!(table.contains(r#"ref="A1:C4""#) && table.contains(r#"<tableColumns count="3">"#) && !table.contains("Series 3"), "{table}");
    // D5 held a formula: the calculation chain goes.
    assert!(part(&wb, "xl/calcChain.xml").is_none());
    let f = formulas(&out);
    assert!(f.contains(&"'Chart data'!$C$2:$C$4".to_string()) && !f.iter().any(|f| f.contains("$D$")), "{f:?}");
}

#[test]
fn a_middle_series_removed_moves_the_next_ones_left() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    s.execute("chart.deleteSeries", &json!({"id": id, "at": 0})).expect("delete series");
    let wb = embedded(&export(&s));
    rest_unchanged(&book, &wb);
    let c = cells(&wb, SHEET);
    assert_eq!([cell(&c, "B1"), cell(&c, "B2"), cell(&c, "C1"), cell(&c, "C5"), cell(&c, "D1")], ["Series 2", "2.6", "Series 3", "5.1", ""]);
    let table = part(&wb, "xl/tables/table1.xml").expect("table");
    assert!(table.contains(r#"ref="A1:C5""#) && table.contains(r#"name="Series 2""#) && table.contains(r#"name="Series 3""#), "{table}");
}

#[test]
fn rows_and_columns_switched_in_place() {
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    s.execute("chart.switchRowColumn", &json!({"id": id})).expect("switch");
    let out = export(&s);
    let wb = embedded(&out);
    rest_unchanged(&book, &wb);
    let c = cells(&wb, SHEET);
    assert_eq!([cell(&c, "B1"), cell(&c, "E1"), cell(&c, "A2"), cell(&c, "A4"), cell(&c, "E4")], ["Q1", "Q4", "Series 1", "Series 3", "5.1"]);
    assert_eq!([cell(&c, "A5"), cell(&c, "B5")], ["", ""]);
    let table = part(&wb, "xl/tables/table1.xml").expect("table");
    assert!(table.contains(r#"ref="A1:E4""#) && table.contains(r#"name="Q1""#) && table.contains(r#"<tableColumn id="5" name="Q4"/>"#), "{table}");
    assert!(formulas(&out).contains(&"'Chart data'!$E$2:$E$4".to_string()));
}

/// Each value the exported chart part caches is in the workbook cell its formula names.
fn assert_in_step(out: &[u8], what: &str) {
    let wb = embedded(out);
    let c = cells(&wb, SHEET);
    let xml = part(out, "ppt/charts/chart1.xml").expect("chart");
    for r in xml.split("<c:f>").skip(1) {
        let (f, rest) = r.split_once("</c:f>").expect("formula");
        let cache = rest.split("</c:strCache>").next().and_then(|c| c.split("</c:numCache>").next()).unwrap_or("");
        let refs = f.rsplit_once('!').map_or(f, |(_, r)| r).replace('$', "");
        let (a, b) = refs.split_once(':').unwrap_or((&refs, &refs));
        let split = |x: &str| {
            let i = x.find(|ch: char| ch.is_ascii_digit()).expect("row");
            (x[..i].to_string(), x[i..].parse::<usize>().expect("row"))
        };
        let ((ca, ra), (cb, rb)) = (split(a), split(b));
        let at: Vec<String> = if ca == cb {
            (ra..=rb).map(|r| format!("{ca}{r}")).collect()
        } else {
            (ca.as_bytes()[0]..=cb.as_bytes()[0]).map(|ch| format!("{}{ra}", ch as char)).collect()
        };
        for (i, cellref) in at.iter().enumerate() {
            let want = cache
                .split(&format!("<c:pt idx=\"{i}\"><c:v>"))
                .nth(1)
                .and_then(|v| v.split("</c:v>").next())
                .map(|v| v.replace("&amp;", "&"))
                .unwrap_or_default();
            assert_eq!(cell(&c, cellref), want, "{what}: {f} [{i}]");
        }
    }
}

fn run_steps(s: &mut Session, id: u32, steps: &[(&str, serde_json::Value)]) {
    for (cmd, p) in steps {
        let mut p = p.clone();
        p["id"] = json!(id);
        s.execute(cmd, &p).expect(cmd);
    }
}

#[test]
fn every_data_edit_keeps_workbook_and_chart_in_step() {
    let (pptx, _) = imported_deck();
    let edits: Vec<Vec<(&str, serde_json::Value)>> = vec![
        vec![("chart.insertRows", json!({"at": 0}))],
        vec![("chart.insertSeries", json!({"at": 1, "name": "New"}))],
        vec![("chart.moveSeries", json!({"from": 0, "to": 2})), ("chart.moveRow", json!({"from": 3, "to": 0}))],
        vec![("chart.paste", json!({"row": 3, "col": 2, "text": "7\t8\n9\t10\n11\t12"}))],
        vec![("chart.setCell", json!({"row": 2, "col": 3, "value": ""})), ("chart.setCell", json!({"row": 0, "col": 1, "value": "Renamed"}))],
        vec![("chart.deleteRows", json!({"at": 0, "count": 3}))],
    ];
    for steps in edits {
        let (mut s, id) = session(&pptx);
        run_steps(&mut s, id, &steps);
        let out = export(&s);
        assert!(part(&out, "ppt/charts/chart1.xml").expect("chart").contains("'Chart data'!"), "{steps:?} rebuilt the workbook");
        assert_in_step(&out, &format!("{steps:?}"));
    }
}

/// A deck with one chart of `kind` made here, its workbook carrying an extra part: a workbook
/// updated in place keeps it, a rebuilt one doesn't.
fn marked_deck(chart: Chart) -> Vec<u8> {
    let mut p = defaults::new_presentation(None);
    let shape = Shape { id: ShapeId(p.alloc_id()), kind: ShapeKind::Chart(Box::new(chart)), ..Default::default() };
    Arc::make_mut(&mut p.slides[0]).shapes.push(shape);
    let out = deckcraft_pptx::export(&p).expect("export");
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in entries(&embedded(&out)) {
        z.start_file(n.as_str(), SimpleFileOptions::default()).expect("start");
        z.write_all(&d).expect("write");
    }
    z.start_file("customXml/marker.xml", SimpleFileOptions::default()).expect("start");
    z.write_all(b"<marker/>").expect("write");
    let book = z.finish().expect("finish").into_inner();
    replace(&out, "ppt/embeddings/workbook1.xlsx", &book)
}

#[test]
fn every_chart_type_grows_and_shrinks_in_place() {
    let mut own_x = Chart::sample(ChartType::Scatter);
    own_x.series[1].x_values = vec![Some(10.0), Some(20.0), Some(30.0), Some(40.0)];
    let mut stock = Chart::sample(ChartType::Line);
    stock.kind = ChartType::Stock;
    let charts = [
        Chart::sample(ChartType::Line),
        Chart::sample(ChartType::Pie),
        Chart::sample(ChartType::Radar),
        Chart::sample(ChartType::Scatter),
        own_x,
        Chart::sample(ChartType::Bubble),
        stock,
    ];
    let edits: Vec<Vec<(&str, serde_json::Value)>> = vec![
        vec![("chart.insertRows", json!({}))],
        vec![("chart.insertSeries", json!({}))],
        vec![("chart.insertSeries", json!({})), ("chart.insertRows", json!({"at": 1, "count": 2}))],
        vec![("chart.deleteRows", json!({"at": 1}))],
        vec![("chart.deleteSeries", json!({"at": 0}))],
        vec![("chart.paste", json!({"row": 1, "col": 1, "text": "1\t2\n3\t4\n5\t6\n7\t8\n9\t10"}))],
    ];
    for chart in charts {
        let kind = chart.kind;
        let pptx = marked_deck(chart);
        for steps in &edits {
            let (mut s, id) = session(&pptx);
            if kind == ChartType::Pie && steps.iter().any(|(c, _)| *c == "chart.deleteSeries") {
                continue;
            }
            run_steps(&mut s, id, steps);
            let out = export(&s);
            let what = format!("{kind:?} {steps:?}");
            assert!(part(&embedded(&out), "customXml/marker.xml").is_some(), "{what}: workbook rebuilt");
            assert_in_step(&out, &what);
        }
    }
}

#[test]
fn a_chart_made_here_gets_a_generated_workbook() {
    let mut p = defaults::new_presentation(None);
    let chart = Shape { id: ShapeId(p.alloc_id()), kind: ShapeKind::Chart(Box::new(Chart::sample(ChartType::Line))), ..Default::default() };
    Arc::make_mut(&mut p.slides[0]).shapes.push(chart);
    let out = deckcraft_pptx::export(&p).expect("export");
    let sheet = part(&embedded(&out), SHEET).expect("sheet");
    assert!(sheet.contains(r#"<c r="B1" t="inlineStr"><is><t xml:space="preserve">Series 1</t></is></c>"#), "{sheet}");
    // Imported back, it can be updated in place like any other.
    let (mut s, id) = session(&out);
    data(&mut s, id, QS, &[("Series 1", &[1.5, 2.8, 3.6, 4.7]), ("Series 2", S2), ("Series 3", S3)]);
    let again = part(&embedded(&export(&s)), SHEET).expect("sheet");
    assert_eq!(again, sheet.replace(r#"<c r="B2"><v>4.2</v></c>"#, r#"<c r="B2"><v>1.5</v></c>"#));
}

#[test]
fn a_series_name_the_table_already_has_rebuilds_the_workbook() {
    // Excel wants table column names unique: the original workbook can't take this one.
    let (pptx, book) = imported_deck();
    let (mut s, id) = session(&pptx);
    data(&mut s, id, QS, &[("Series 1", S1), ("series 1", S2), ("Series 3", S3)]);
    let out = export(&s);
    assert_ne!(embedded(&out), book);
    assert!(part(&out, "ppt/charts/chart1.xml").expect("chart").contains("<c:f>Sheet1!$C$1</c:f>"));
}

#[test]
fn scatter_charts_update_their_values_in_place() {
    let mut p = defaults::new_presentation(None);
    let chart = Shape { id: ShapeId(p.alloc_id()), kind: ShapeKind::Chart(Box::new(Chart::sample(ChartType::Scatter))), ..Default::default() };
    Arc::make_mut(&mut p.slides[0]).shapes.push(chart);
    let first = deckcraft_pptx::export(&p).expect("export");
    let book = embedded(&first);
    let sheet = part(&book, SHEET).expect("sheet");
    let (mut s, id) = session(&first);
    s.execute("chart.options", &json!({"id": id, "title": "Spread"})).expect("options");
    assert_eq!(embedded(&export(&s)), book);
    data(&mut s, id, &["1", "2", "3", "4"], &[("Series 1", S1), ("Series 2", &[2.6, 4.1, 1.9, 7.25]), ("Series 3", S3)]);
    let again = part(&embedded(&export(&s)), SHEET).expect("sheet");
    assert_eq!(again, sheet.replace(r#"<c r="C5"><v>3</v></c>"#, r#"<c r="C5"><v>7.25</v></c>"#));
    // A scatter chart turned into a column chart reads categories, not X values: new workbook.
    s.execute("chart.type", &json!({"id": id, "type": "column"})).expect("type");
    let column = part(&embedded(&export(&s)), SHEET).expect("sheet");
    assert!(column.contains(r#"<c r="A2" t="inlineStr">"#), "{column}");
}
