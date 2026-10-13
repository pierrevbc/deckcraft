//! Chart formatting survives a PPTX round trip: fonts, element boxes, legend, axes, gridlines,
//! data labels, series looks, points and series options are written into the chart part and read
//! back as they were. Decks are built here with the chart format commands.

use std::io::{Cursor, Read};

use deckcraft_engine::{DocState, Session};
use deckcraft_model::chart::Chart;
use deckcraft_model::{Presentation, ShapeKind};
use serde_json::{Value, json};

/// A new deck with one chart of `kind`, formatted by `cmds`.
fn formatted(kind: &str, cmds: &[(&str, Value)]) -> Session {
    let mut s = Session::with_new();
    let r = s.execute("insert.chart", &json!({"type": kind, "rect": [40, 40, 640, 400]})).expect("insert");
    let id = r["id"].as_u64().expect("id");
    s.execute("edit.select", &json!({"ids": [id]})).expect("select");
    for (cmd, p) in cmds {
        s.execute(cmd, p).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    }
    s
}

fn chart_of(p: &Presentation) -> Chart {
    p.slides
        .iter()
        .flat_map(|sl| sl.shapes.iter())
        .find_map(|sh| if let ShapeKind::Chart(c) = &sh.kind { Some((**c).clone()) } else { None })
        .expect("a chart")
}

fn part(pptx: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(Cursor::new(pptx)).expect("zip");
    let mut f = z.by_name(name).expect("part");
    let mut s = String::new();
    f.read_to_string(&mut s).expect("read");
    s
}

/// Export the session's deck, import it again: (chart before, chart after, chart part XML).
fn round_trip(s: &Session) -> (Chart, Chart, String) {
    let before = chart_of(&s.doc().expect("doc").doc);
    let pptx = deckcraft_pptx::export(&s.doc().expect("doc").doc).expect("export");
    let xml = part(&pptx, "ppt/charts/chart1.xml");
    let after = chart_of(&deckcraft_pptx::import(&pptx).expect("import"));
    // Exporting the imported deck gives the same chart again.
    let mut s2 = Session::new();
    s2.add_document(DocState::new(deckcraft_pptx::import(&pptx).expect("import"), None, "again".into()));
    let again = chart_of(&deckcraft_pptx::import(&deckcraft_pptx::export(&s2.doc().expect("doc").doc).expect("export")).expect("import"));
    assert_eq!(again.format, after.format);
    (before, after, xml)
}

/// A number format as it shows: General is no format.
fn shown_format(f: &Option<String>) -> Option<&str> {
    f.as_deref().filter(|f| !f.eq_ignore_ascii_case("general"))
}

/// The formatting of a chart, for comparison: everything but data and import bookkeeping. A
/// series' label number format counts where its labels show (it is kept with them).
fn looks(c: &Chart) -> Value {
    let series: Vec<Value> = c
        .series
        .iter()
        .map(|s| {
            let labels = s.data_labels.unwrap_or(c.data_labels);
            json!({
                "fill": s.fill, "line": s.line, "points": s.point_fills, "labels": s.data_labels, "smooth": s.smooth,
                "marker": s.marker, "format": s.format, "numberFormat": if labels { shown_format(&s.number_format) } else { None },
            })
        })
        .collect();
    // Pie charts have no axes, hence no gridlines to keep.
    let axes = !c.kind.is_pie();
    json!({
        "title": c.title, "legend": c.legend, "labels": c.data_labels, "gridlines": c.gridlines && axes, "axisTitles": c.axis_titles,
        "plotFill": c.plot_fill, "chartFill": c.chart_fill, "gap": c.gap_width, "hole": c.hole_size, "vary": c.vary_colors,
        "min": c.y_min, "max": c.y_max, "format": c.format, "series": series,
        "legendOverlay": c.legend_overlay, "textSize": c.text_size, "titleSize": c.title_size,
        "yAxis": [c.y_major, shown_format(&c.y_format), c.y_hidden],
        "y2Axis": [c.y2_min, c.y2_max, c.y2_major, shown_format(&c.y2_format), c.y2_hidden],
    })
}

#[test]
fn column_chart_formatting_round_trips() {
    let s = formatted(
        "column",
        &[
            (
                "chart.format.element",
                json!({"element": "chartArea", "fill": "#FFF8EE", "line": {"color": "accent2", "width": 2, "dash": "dash"}, "font": {"size": 11, "color": "#203864", "font": "Liberation Sans"}, "roundedCorners": true}),
            ),
            (
                "chart.format.element",
                json!({"element": "title", "font": {"size": 24, "bold": true, "italic": true, "color": "#C00000"}, "fill": "#FFFFFF", "line": {"color": "#C00000", "width": 1}}),
            ),
            (
                "chart.format.element",
                json!({"element": "plotArea", "fill": {"gradient": {"stops": [[0, "#ffffff"], [1, "accent1"]], "angle": 90}}, "line": {"color": "#999999", "width": 0.75}}),
            ),
            ("chart.format.legend", json!({"position": "topRight", "overlay": true})),
            (
                "chart.format.element",
                json!({"element": "legend", "font": {"size": 9, "italic": true}, "fill": {"scheme": "bg1", "lumMod": 95000}, "line": "none"}),
            ),
            (
                "chart.format.axis",
                json!({"axis": "value", "min": 0, "max": 6, "majorUnit": 1, "minorUnit": 0.25, "numFmt": "0.0", "majorTick": "out", "minorTick": "in", "minorGridlines": true, "title": "Revenue", "crosses": "max", "labelPosition": "high"}),
            ),
            ("chart.format.element", json!({"element": "valueAxis", "line": {"color": "#404040", "width": 1}, "font": {"size": 10, "bold": true}})),
            ("chart.format.element", json!({"element": "majorGridlines", "line": {"color": "#BFBFBF", "width": 0.5, "dash": "sysDash"}})),
            ("chart.format.element", json!({"element": "minorGridlines", "line": {"color": "#EEEEEE", "width": 0.25}})),
            ("chart.format.element", json!({"element": "valueAxisTitle", "font": {"size": 12, "color": "accent1"}})),
            (
                "chart.format.axis",
                json!({"axis": "category", "rotation": -45, "title": "Quarter", "reverse": true, "majorGridlines": true, "crosses": 2.5}),
            ),
            ("chart.format.element", json!({"element": "categoryMajorGridlines", "line": {"color": "#DDDDDD", "width": 0.75}})),
            ("chart.format.labels", json!({"show": true, "position": "insideEnd", "numFmt": "0.0", "category": true, "separator": " - "})),
            ("chart.format.element", json!({"element": "dataLabels", "font": {"size": 8, "color": "#FFFFFF"}, "fill": "accent6"})),
            (
                "chart.format.series",
                json!({"series": 0, "fill": {"gradient": {"stops": [[0, "#9DC3E6"], [0.5, "accent1"], [1, "#1F4E79"]], "angle": 45}}}),
            ),
            ("chart.format.series", json!({"series": 1, "line": {"color": "#000000", "width": 1.5, "dash": "lgDashDot"}, "invertIfNegative": true})),
            ("chart.format.series", json!({"series": 2, "fill": "none", "line": {"color": "accent3", "width": 2}})),
            ("chart.format.point", json!({"series": 0, "point": 2, "fill": "#FF0000"})),
            ("chart.format.options", json!({"gapWidth": 60, "overlap": 25})),
        ],
    );
    let (before, after, xml) = round_trip(&s);
    assert_eq!(looks(&after), looks(&before), "{xml}");
    // A few checks on the XML itself, in schema order.
    for frag in [
        r#"<c:roundedCorners val="1"/>"#,
        r#"<c:legendPos val="tr"/><c:overlay val="1"/>"#,
        r#"<c:orientation val="maxMin"/>"#,
        r#"<a:prstDash val="lgDashDot"/>"#,
        r#"<c:numFmt formatCode="0.0" sourceLinked="0"/>"#,
        r#"<c:crossesAt val="2.5"/>"#,
        r#"<c:crosses val="max"/>"#,
        r#"<c:majorUnit val="1"/><c:minorUnit val="0.25"/>"#,
        r#"<c:dLblPos val="inEnd"/>"#,
        r#"<c:separator> - </c:separator>"#,
        r#"<a:bodyPr rot="-2700000" vert="horz"/>"#,
        r#"<c:overlap val="25"/>"#,
        r#"<c:gapWidth val="60"/>"#,
    ] {
        assert!(xml.contains(frag), "{frag} missing: {xml}");
    }
    let order = [
        "<c:axId",
        "<c:scaling>",
        "<c:delete",
        "<c:axPos",
        "<c:majorGridlines",
        "<c:minorGridlines",
        "<c:title>",
        "<c:numFmt",
        "<c:majorTickMark",
        "<c:minorTickMark",
        "<c:tickLblPos",
        "<c:spPr>",
        "<c:txPr>",
        "<c:crossAx",
        "<c:crosses",
    ];
    let val_ax = &xml[xml.find("<c:valAx>").expect("valAx")..];
    let mut at = 0;
    for tag in order {
        let rest = val_ax.get(at..).unwrap_or("");
        let i = rest.find(tag).unwrap_or_else(|| panic!("{tag} missing or out of order in {val_ax}"));
        // Skip over this element's own content (gridlines and titles hold `c:spPr`, `c:txPr`).
        at += i + tag.len();
    }
}

/// What upstream's chart reader reads (legend overlay, value axes' number formats, units and
/// deletion, a combo chart's secondary axis, text sizes, the chart's language) is what the format
/// commands set and the writer writes.
#[test]
fn upstream_chart_fields_round_trip() {
    let s = formatted(
        "columnLineSecondary",
        &[
            ("chart.format.legend", json!({"position": "left", "overlay": true})),
            ("chart.format.element", json!({"element": "chartArea", "font": {"size": 12}})),
            ("chart.format.element", json!({"element": "title", "font": {"size": 20, "bold": true}})),
            ("chart.format.element", json!({"element": "legend", "font": {"size": 9}})),
            ("chart.format.axis", json!({"axis": "value", "numFmt": "#,##0", "majorUnit": 2})),
            ("chart.format.axis", json!({"axis": "secondary", "min": 0, "max": 80, "majorUnit": 20, "numFmt": "0.0", "visible": false})),
            ("chart.format.labels", json!({"show": true, "numFmt": "0.0"})),
        ],
    );
    let (before, after, xml) = round_trip(&s);
    let mut diffs = vec![];
    json_diff(&looks(&before), &looks(&after), "", &mut diffs);
    assert!(
        diffs.is_empty(),
        "{diffs:?}
{xml}"
    );
    assert_eq!((after.text_size, after.title_size, after.legend_overlay), (Some(12.0), Some(20.0), true));
    assert_eq!((after.y_format.as_deref(), after.y_major), (Some("#,##0"), Some(2.0)));
    assert_eq!(
        (after.y2_min, after.y2_max, after.y2_major, after.y2_format.as_deref(), after.y2_hidden),
        (Some(0.0), Some(80.0), Some(20.0), Some("0.0"), true)
    );
    assert!(after.series.iter().any(|s| s.secondary_axis) && after.series.iter().all(|s| s.number_format.as_deref() == Some("0.0")), "{xml}");
    assert_eq!(after.format.legend_text.size, Some(9.0));
    for frag in
        [r#"<c:legendPos val="l"/><c:overlay val="1"/>"#, r##"<c:numFmt formatCode="#,##0" sourceLinked="0"/>"##, r#"<c:majorUnit val="20"/>"#]
    {
        assert!(xml.contains(frag), "{frag} missing: {xml}");
    }
    // The chart's language comes back too.
    let mut p: Presentation = (*s.doc().expect("doc").doc).clone();
    for sl in &mut p.slides {
        for sh in &mut std::sync::Arc::make_mut(sl).shapes {
            if let ShapeKind::Chart(c) = &mut sh.kind {
                c.lang = Some("ru-RU".into());
            }
        }
    }
    let back = chart_of(&deckcraft_pptx::import(&deckcraft_pptx::export(&p).expect("export")).expect("import"));
    assert_eq!(back.lang.as_deref(), Some("ru-RU"));
}

#[test]
fn line_pie_and_bar_formatting_round_trips() {
    let line = formatted(
        "line",
        &[
            (
                "chart.format.series",
                json!({"series": 0, "smooth": true, "marker": "diamond", "markerSize": 11, "markerFill": "#FFFFFF", "markerLine": {"color": "accent1", "width": 2}, "line": {"width": 3.5}}),
            ),
            (
                "chart.format.series",
                json!({"series": 1, "line": {"color": "#7F7F7F", "width": 2, "dash": "dash"}, "marker": "square", "markerSize": 8}),
            ),
            ("chart.format.labels", json!({"show": true, "series": 0})),
            ("chart.format.labels", json!({"position": "above", "numFmt": "$#,##0.00"})),
            ("chart.format.axis", json!({"axis": "value", "logBase": 10, "visible": false})),
            ("chart.format.legend", json!({"position": "top"})),
        ],
    );
    let (before, after, xml) = round_trip(&line);
    assert_eq!(looks(&after), looks(&before), "{xml}");
    assert!(xml.contains(r#"<c:logBase val="10"/>"#) && xml.contains(r#"<c:symbol val="diamond"/><c:size val="11"/>"#), "{xml}");

    let pie = formatted(
        "pie",
        &[
            ("chart.format.options", json!({"firstSliceAngle": 90})),
            (
                "chart.format.point",
                json!({"point": 3, "explosion": 18, "fill": {"gradient": {"stops": [[0, "#FFD966"], [1, "#BF9000"]], "angle": 45}}}),
            ),
            ("chart.format.point", json!({"point": 1, "explosion": 10})),
            ("chart.format.series", json!({"series": 0, "line": {"color": "#FFFFFF", "width": 3}, "explosion": 4})),
            (
                "chart.format.labels",
                json!({"show": true, "value": false, "category": true, "percent": true, "position": "bestFit", "numFmt": "0.0%"}),
            ),
            ("chart.format.legend", json!({"position": "left"})),
        ],
    );
    let (before, after, xml) = round_trip(&pie);
    assert_eq!(looks(&after), looks(&before), "{xml}");
    assert!(xml.contains(r#"<c:firstSliceAng val="90"/>"#) && xml.contains(r#"<c:explosion val="18"/>"#), "{xml}");

    let bar = formatted(
        "bar",
        &[
            (
                "chart.data",
                json!({"categories": ["N", "S", "E", "W"], "series": [{"name": "A", "values": [3, -1, 2, -0.5]}, {"name": "B", "values": [2, 1, -2, 3]}]}),
            ),
            ("chart.format.axis", json!({"axis": "category", "reverse": true, "labelPosition": "low"})),
            ("chart.format.axis", json!({"axis": "value", "numFmt": "0.0;(0.0)", "majorTick": "cross"})),
            ("chart.format.series", json!({"series": 0, "invertIfNegative": true})),
            // Quick layout 6 hides data labels: show them after it.
            ("chart.quickLayout", json!({"layout": 6})),
            ("chart.format.labels", json!({"show": true, "position": "outsideEnd"})),
        ],
    );
    let (before, after, xml) = round_trip(&bar);
    assert_eq!(looks(&after), looks(&before), "{xml}");
    assert!(xml.contains(r#"<c:invertIfNegative val="1"/>"#), "{xml}");
}

#[test]
fn styles_and_palettes_write_their_colours() {
    let s = formatted("column", &[("chart.colors", json!({"palette": "monochrome2"}))]);
    let (_, after, xml) = round_trip(&s);
    // The palette is written as series colours, theme-linked.
    assert_eq!(xml.matches(r#"<a:schemeClr val="accent2">"#).count() + xml.matches(r#"<a:schemeClr val="accent2"/>"#).count(), 3, "{xml}");
    assert!(after.series.iter().all(|s| s.fill.is_some()));
    for style in 1..=8 {
        let s = formatted("column", &[("chart.style", json!({"style": style}))]);
        let (before, after, xml) = round_trip(&s);
        assert_eq!(looks(&after), looks(&before), "style {style}: {xml}");
    }
    let s = formatted("doughnut", &[("chart.colors", json!({"palette": "colorful2"})), ("chart.format.options", json!({"holeSize": 65}))]);
    let (_, after, xml) = round_trip(&s);
    assert_eq!(after.series[0].point_fills.len(), 4, "{xml}");
    assert!((after.hole_size - 0.65).abs() < 1e-9);
}

#[test]
fn scatter_x_axis_bounds_round_trip() {
    let s = formatted(
        "scatter",
        &[("chart.format.axis", json!({"axis": "category", "min": 0.5, "max": 5, "majorUnit": 0.5, "numFmt": "0.0", "majorTick": "out"}))],
    );
    let (before, after, xml) = round_trip(&s);
    assert_eq!(after.format.cat_axis, before.format.cat_axis, "{xml}");
    assert_eq!((after.format.cat_axis.min, after.format.cat_axis.max), (Some(0.5), Some(5.0)));
}

/// Label positions a chart type refuses are left out, so PowerPoint opens the file.
#[test]
fn label_positions_follow_the_chart_type() {
    let s = formatted("area", &[("chart.format.labels", json!({"show": true, "position": "outsideEnd"}))]);
    let (_, _, xml) = round_trip(&s);
    assert!(!xml.contains("dLblPos"), "{xml}");
    let s = formatted("stackedColumn", &[("chart.format.labels", json!({"show": true, "position": "outsideEnd"}))]);
    let (_, _, xml) = round_trip(&s);
    assert!(!xml.contains("dLblPos"), "{xml}");
    let s = formatted("line", &[("chart.format.labels", json!({"show": true, "position": "below"}))]);
    let (_, _, xml) = round_trip(&s);
    assert!(xml.contains(r#"<c:dLblPos val="b"/>"#), "{xml}");
}

/// Every chart family keeps its formatting through a PPTX, and its type and variant too: stock
/// charts (with volume), radar with markers, scatter with lines and X bounds, pie of pie,
/// exploded doughnut, 3-D columns, column-line combo on a secondary axis, bubbles.
#[test]
fn every_chart_family_keeps_its_formatting() {
    let gradient = json!({"gradient": {"stops": [[0, "#9DC3E6"], [1, "#1F4E79"]], "angle": 90}});
    let cases: Vec<(&str, Vec<(&str, Value)>)> = vec![
        (
            "stockOhlc",
            vec![
                ("chart.format.axis", json!({"axis": "value", "numFmt": "$0", "majorTick": "out", "minorGridlines": true})),
                ("chart.format.axis", json!({"axis": "category", "rotation": -45})),
            ],
        ),
        (
            "stockVohlc",
            vec![
                ("chart.format.series", json!({"series": 0, "fill": gradient})),
                ("chart.format.axis", json!({"axis": "value", "min": 0, "max": 400})),
            ],
        ),
        (
            "radarMarkers",
            vec![
                (
                    "chart.format.series",
                    json!({"series": 0, "line": {"color": "#7F7F7F", "width": 2, "dash": "dash"}, "marker": "diamond", "markerSize": 9, "markerFill": "#FFFFFF"}),
                ),
                ("chart.format.axis", json!({"axis": "value", "numFmt": "0.0"})),
            ],
        ),
        (
            "scatterLines",
            vec![
                ("chart.format.series", json!({"series": 1, "line": {"color": "accent4", "width": 3, "dash": "sysDot"}})),
                ("chart.format.axis", json!({"axis": "category", "min": 0, "max": 5, "majorTick": "cross"})),
                ("chart.format.axis", json!({"axis": "value", "reverse": true, "logBase": 10})),
            ],
        ),
        (
            "pieOfPie",
            vec![
                ("chart.format.point", json!({"point": 1, "fill": "#FF0000"})),
                ("chart.format.series", json!({"series": 0, "line": {"color": "#FFFFFF", "width": 2}})),
                ("chart.format.labels", json!({"percent": true, "position": "outsideEnd"})),
            ],
        ),
        (
            "explodedDoughnut",
            vec![
                ("chart.format.options", json!({"firstSliceAngle": 45, "holeSize": 40})),
                ("chart.format.point", json!({"point": 2, "explosion": 30})),
            ],
        ),
        (
            "column3d",
            vec![
                ("chart.format.series", json!({"series": 1, "fill": "accent6", "line": {"color": "#000000", "width": 1}})),
                ("chart.format.options", json!({"gapWidth": 80})),
                ("chart.format.axis", json!({"axis": "value", "min": 0, "max": 8, "majorUnit": 2})),
            ],
        ),
        (
            "columnLineSecondary",
            vec![
                ("chart.format.series", json!({"series": 0, "fill": gradient, "invertIfNegative": true})),
                (
                    "chart.format.series",
                    json!({"series": 2, "line": {"color": "#00B050", "width": 3, "dash": "lgDash"}, "marker": "square", "markerSize": 8}),
                ),
                ("chart.format.options", json!({"overlap": 20, "gapWidth": 100})),
                ("chart.format.axis", json!({"axis": "category", "reverse": true})),
            ],
        ),
        ("bubble", vec![("chart.format.series", json!({"series": 0, "fill": "#FFC000", "line": {"color": "#7F6000", "width": 1}}))]),
    ];
    for (kind, extra) in cases {
        let mut cmds = vec![
            ("chart.format.element", json!({"element": "title", "font": {"size": 20, "bold": true, "color": "#C00000"}})),
            ("chart.format.legend", json!({"position": "left"})),
            ("chart.format.element", json!({"element": "legend", "font": {"italic": true}})),
            ("chart.format.element", json!({"element": "chartArea", "fill": "#FFF8EE", "line": {"color": "accent2", "width": 1.5, "dash": "dash"}})),
            ("chart.format.labels", json!({"show": true, "numFmt": "0.0"})),
        ];
        cmds.extend(extra);
        let s = formatted(kind, &cmds);
        let (before, after, xml) = round_trip(&s);
        let mut diffs = vec![];
        json_diff(&looks(&before), &looks(&after), "", &mut diffs);
        assert!(
            diffs.is_empty(),
            "{kind}: {diffs:?}
{xml}"
        );
        assert_eq!((after.kind, &after.variant), (before.kind, &before.variant), "{kind}: {xml}");
        let axes = |c: &Chart| c.series.iter().map(|s| (s.kind, s.secondary_axis)).collect::<Vec<_>>();
        assert_eq!(axes(&after), axes(&before), "{kind}: {xml}");
    }
}

/// The paths where two JSON values differ, with both sides.
fn json_diff(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                json_diff(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), &format!("{path}.{k}"), out);
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                json_diff(p, q, &format!("{path}[{i}]"), out);
            }
        }
        _ if a != b => out.push(format!("{path}: {a} -> {b}")),
        _ => {}
    }
}
