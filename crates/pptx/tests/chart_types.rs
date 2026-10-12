//! Every chart type and variant of the gallery (the LibreOffice Impress set, plus PowerPoint's
//! names for them) survives a PPTX round trip: same type, variant and data, a workbook for Edit
//! Data, and the very same part on a second export. Decks are built here, in code.

use std::io::Read;
use std::sync::Arc;

use deckcraft_engine::{DocState, Session};
use deckcraft_geom::Xfrm;
use deckcraft_model::chart::{Chart, ChartPreset, ChartType};
use deckcraft_model::{Presentation, Shape, ShapeId, ShapeKind, defaults};
use serde_json::json;

/// The chart a gallery entry inserts, titled with its label.
fn preset_chart(p: &ChartPreset) -> Chart {
    let mut c = Chart::sample(p.kind);
    p.apply(&mut c);
    c.title = Some(p.label.to_string());
    c
}

/// A deck with one slide per chart.
fn deck(charts: &[Chart]) -> Presentation {
    let mut p = defaults::new_presentation(None);
    let layout = defaults::layout_of_kind(&p, deckcraft_model::LayoutType::Blank).unwrap_or_default();
    p.slides.clear();
    for c in charts {
        let mut s = defaults::new_slide(&mut p, layout);
        let id = ShapeId(p.alloc_id());
        s.shapes.push(Shape {
            id,
            name: c.title.clone().unwrap_or_default(),
            xfrm: Some(Xfrm::new(60.0, 40.0, 840.0, 460.0)),
            kind: ShapeKind::Chart(Box::new(c.clone())),
            ..Default::default()
        });
        p.slides.push(Arc::new(s));
    }
    p
}

fn charts_of(p: &Presentation) -> Vec<Chart> {
    p.slides
        .iter()
        .flat_map(|s| s.shapes.iter())
        .filter_map(|sh| if let ShapeKind::Chart(c) = &sh.kind { Some(c.as_ref().clone()) } else { None })
        .collect()
}

/// Zip entries under `prefix`, sorted by name.
fn entries(zip_bytes: &[u8], prefix: &str) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).expect("zip");
    let mut out = vec![];
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("entry");
        if f.name().starts_with(prefix) {
            let mut data = vec![];
            f.read_to_end(&mut data).expect("read");
            out.push((f.name().to_string(), data));
        }
    }
    out.sort();
    out
}

/// The same type, variant and data.
fn assert_same_chart(id: &str, a: &Chart, b: &Chart) {
    assert_eq!(a.kind, b.kind, "{id}");
    assert_eq!(a.title, b.title, "{id}");
    assert_eq!(a.variant.stepped, b.variant.stepped, "{id}");
    assert_eq!(a.variant.three_d.map(|t| (t.shape, t.deep)), b.variant.three_d.map(|t| (t.shape, t.deep)), "{id}");
    assert_eq!(a.kind.exploded(), b.variant.explosion > 0, "{id}");
    assert_eq!(a.series.len(), b.series.len(), "{id}");
    for (x, y) in a.series.iter().zip(&b.series) {
        assert_eq!(x.name, y.name, "{id}");
        assert_eq!(x.values, y.values, "{id}");
        assert_eq!(x.smooth, y.smooth, "{id}: {}", x.name);
        assert_eq!(x.secondary_axis, y.secondary_axis, "{id}: {}", x.name);
        assert_eq!(x.kind, y.kind, "{id}: {}", x.name);
        if a.kind.is_xy() {
            assert_eq!(x.x_values, y.x_values, "{id}");
        }
        if a.kind == ChartType::Bubble {
            assert_eq!(x.sizes, y.sizes, "{id}");
        }
    }
    if !a.kind.is_xy() {
        assert_eq!(a.categories, b.categories, "{id}");
    }
}

#[test]
fn every_gallery_type_round_trips() {
    let charts: Vec<Chart> = ChartPreset::GALLERY.iter().map(preset_chart).collect();
    let p = deck(&charts);
    let first = deckcraft_pptx::export(&p).expect("export");
    let q = deckcraft_pptx::import(&first).expect("import");
    let back = charts_of(&q);
    assert_eq!(back.len(), charts.len());
    for ((preset, a), b) in ChartPreset::GALLERY.iter().zip(&charts).zip(&back) {
        assert_same_chart(preset.id, a, b);
        assert!(preset.matches(b), "{}: read back as {:?} {:?}", preset.id, b.kind, b.variant);
        assert!(b.workbook.is_some(), "{}: no workbook", preset.id);
        assert!(b.source.is_some(), "{}: no formulas", preset.id);
    }
    // Every chart has its workbook; a second export writes the same parts and workbooks.
    assert_eq!(entries(&first, "ppt/embeddings/").len(), charts.len());
    let second = deckcraft_pptx::export(&q).expect("export");
    assert_eq!(entries(&first, "ppt/charts/chart"), entries(&second, "ppt/charts/chart"));
    assert_eq!(entries(&first, "ppt/embeddings/"), entries(&second, "ppt/embeddings/"));
    let r = deckcraft_pptx::import(&second).expect("import");
    for (preset, (b, c)) in ChartPreset::GALLERY.iter().zip(back.iter().zip(charts_of(&r))) {
        let (mut b, mut c) = (b.clone(), c);
        for x in [&mut b, &mut c] {
            x.raw = None;
            x.workbook = None;
        }
        assert_eq!(b, c, "{}", preset.id);
    }
}

/// Imported charts change type and variant like new ones, and keep their workbook when their
/// data still fits it.
#[test]
fn changing_the_type_of_an_imported_chart_keeps_its_workbook() {
    let p = deck(&[Chart::sample(ChartType::Column)]);
    let first = deckcraft_pptx::export(&p).expect("export");
    let book = entries(&first, "ppt/embeddings/");
    let q = deckcraft_pptx::import(&first).expect("import");
    let id = q.slides.iter().flat_map(|s| s.shapes.iter()).find(|s| matches!(s.kind, ShapeKind::Chart(_))).map(|s| s.id.0).expect("chart");
    for (preset, kind) in [("columnCylinder", ChartType::Column), ("percentBar3d", ChartType::PercentBar), ("lineStepped", ChartType::Line)] {
        let mut s = Session::new();
        s.add_document(DocState::new(q.clone(), None, "Imported".into()));
        s.execute("chart.type", &json!({"id": id, "type": preset})).expect("type");
        let doc = s.doc().expect("doc").doc.clone();
        let out = deckcraft_pptx::export(&doc).expect("export");
        assert_eq!(entries(&out, "ppt/embeddings/"), book, "{preset}");
        let c = charts_of(&deckcraft_pptx::import(&out).expect("import")).remove(0);
        assert_eq!(c.kind, kind, "{preset}");
        assert!(ChartPreset::find(preset).expect("preset").matches(&c), "{preset}: {:?}", c.variant);
    }
}

/// Odd data never makes an invalid part: no series, a stock chart short of prices, a pie of pie
/// with one point, empty and missing values.
#[test]
fn odd_data_still_round_trips() {
    let mut charts = vec![];
    for p in ChartPreset::GALLERY {
        let mut c = preset_chart(p);
        c.series.truncate(1);
        if let Some(s) = c.series.first_mut() {
            s.values.truncate(1);
            s.values.push(None);
        }
        charts.push(c);
        let mut empty = preset_chart(p);
        empty.series.clear();
        charts.push(empty);
    }
    let p = deck(&charts);
    let bytes = deckcraft_pptx::export(&p).expect("export");
    let q = deckcraft_pptx::import(&bytes).expect("import");
    assert!(!charts_of(&q).is_empty());
}
