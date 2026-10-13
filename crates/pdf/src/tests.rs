use std::sync::Arc;

use super::*;
use deckcraft_color::Rgba;
use deckcraft_model::style::{ColorRef, Effects, Fill as ShapeFill, Gradient, GradientShape, GradientStop, PictureFill, Shadow};
use deckcraft_model::text::{Hyperlink, TextBody};
use deckcraft_model::{Shape, ShapeId, ShapeKind, Xfrm};

fn deck(n: usize) -> Presentation {
    let mut p = deckcraft_model::defaults::new_presentation(None);
    let layout = p.slides[0].layout;
    while p.slides.len() < n {
        let s = deckcraft_model::defaults::new_slide(&mut p, layout);
        p.slides.push(Arc::new(s));
    }
    for (i, s) in p.slides.iter_mut().enumerate() {
        let s = Arc::make_mut(s);
        if let Some(sh) = s.shapes.first_mut() {
            sh.text = Some(TextBody::from_text(&format!("Slide title {i}")));
        }
        s.notes = TextBody::from_text("Speaker notes that wrap across the page width to test line breaking of long notes.");
    }
    p
}

fn opts(layout: PageLayout) -> PdfOptions {
    PdfOptions { layout, dpi: 48.0, ..Default::default() }
}

fn add(p: &mut Presentation, slide: usize, sh: Shape) {
    Arc::make_mut(&mut p.slides[slide]).shapes.push(sh);
}

fn rect(id: u32, x: f64, y: f64, fill: ShapeFill) -> Shape {
    Shape { id: ShapeId(id), xfrm: Some(Xfrm::new(x, y, 200.0, 100.0)), fill: Some(fill), ..Default::default() }
}

fn shadow() -> Effects {
    Effects {
        outer_shadow: Some(Shadow {
            color: ColorRef::rgb(Rgba::BLACK),
            blur: 8.0,
            dist: 6.0,
            dir: 45.0,
            inner: false,
            sx: 1.0,
            sy: 1.0,
            kx: 0.0,
            ky: 0.0,
            align: String::new(),
            rotate_with_shape: false,
        }),
        ..Default::default()
    }
}

/// A PDF with readable (uncompressed) page contents and text maps.
fn plain(p: &Presentation, o: &PdfOptions) -> Vec<u8> {
    export_with(p, o, false).expect("export")
}

fn count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

fn pages(pdf: &[u8]) -> usize {
    count(pdf, b"/Type/Page") - count(pdf, b"/Type/Pages")
}

/// (width, height) of every image XObject, soft masks included.
fn images(pdf: &[u8]) -> Vec<(u32, u32)> {
    let num = |d: &[u8], key: &[u8]| -> u32 {
        let Some(at) = d.windows(key.len()).position(|w| w == key) else { return 0 };
        d[at + key.len()..]
            .iter()
            .skip_while(|c| c.is_ascii_whitespace())
            .take_while(|c| c.is_ascii_digit())
            .fold(0, |n, c| n * 10 + (c - b'0') as u32)
    };
    let key = b"/Subtype/Image";
    let mut out = vec![];
    for (at, _) in pdf.windows(key.len()).enumerate().filter(|(_, w)| *w == key) {
        let start = pdf[..at].windows(3).rposition(|w| w == b"obj").unwrap_or(0);
        let end = at + pdf[at..].windows(6).position(|w| w == b"stream").unwrap_or(0);
        let d = &pdf[start..end];
        out.push((num(d, b"/Width"), num(d, b"/Height")));
    }
    out
}

#[test]
fn exports_slides_notes_and_handouts() {
    let p = deck(5);
    for vector in [true, false] {
        for layout in [PageLayout::Slides, PageLayout::Notes, PageLayout::Handouts { per_page: 3 }, PageLayout::Handouts { per_page: 6 }] {
            let bytes = export(&p, &PdfOptions { vector, ..opts(layout) }).expect("export");
            assert!(bytes.starts_with(b"%PDF-"), "{layout:?}");
            assert!(bytes.len() > 1000);
        }
    }
}

#[test]
fn empty_range_is_an_error() {
    let p = deck(1);
    let o = PdfOptions { slides: Some(vec![7]), ..opts(PageLayout::Slides) };
    assert!(matches!(export(&p, &o), Err(PdfError::NoPages)));
}

#[test]
fn hidden_slides_skipped_unless_asked() {
    let mut p = deck(2);
    Arc::make_mut(&mut p.slides[1]).hidden = true;
    for vector in [true, false] {
        let a = plain(&p, &PdfOptions { vector, ..opts(PageLayout::Slides) });
        let b = plain(&p, &PdfOptions { vector, include_hidden: true, ..opts(PageLayout::Slides) });
        assert_eq!(pages(&a), 1, "vector {vector}");
        assert_eq!(pages(&b), 2, "vector {vector}");
    }
}

#[test]
fn vector_slides_are_paths_and_text_without_a_page_image() {
    let mut p = deck(1);
    add(&mut p, 0, rect(900, 40.0, 300.0, ShapeFill::solid(ColorRef::rgb(Rgba::rgb(0x20, 0x60, 0xC0)))));
    let grad = Gradient {
        stops: vec![
            GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::rgb(255, 0, 0)) },
            GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgba(0, 0, 255, 128)) },
        ],
        shape: GradientShape::Linear { angle: 30.0, scaled: false },
        rotate_with_shape: true,
    };
    add(&mut p, 0, rect(901, 300.0, 300.0, ShapeFill::Gradient(grad)));
    let pdf = plain(&p, &opts(PageLayout::Slides));
    assert!(images(&pdf).is_empty(), "{:?}", images(&pdf));
    // Paths (filled), real text and a shading for the gradient.
    assert!(count(&pdf, b"\nf\n") + count(&pdf, b" f\n") > 0);
    assert!(count(&pdf, b"BT") > 0 && count(&pdf, b" Tf") > 0);
    assert!(count(&pdf, b"/ShadingType 2") > 0);
    // The raster mode draws the slide as one image the size of the page.
    let raster = plain(&p, &PdfOptions { vector: false, ..opts(PageLayout::Slides) });
    assert!(images(&raster).iter().any(|(w, _)| *w >= 600), "{:?}", images(&raster));
}

#[test]
fn vector_text_is_extractable() {
    let p = deck(1);
    let pdf = plain(&p, &opts(PageLayout::Slides));
    // The title's characters are mapped back to Unicode (UTF-16 hex in the ToUnicode map).
    assert!(count(&pdf, b"begincmap") > 0);
    for c in "Slide title".chars().filter(|c| !c.is_whitespace()) {
        let hex = format!("<{:04X}>", c as u32);
        assert!(count(&pdf, hex.as_bytes()) > 0, "{c}");
    }
    // No invisible layer: the text is drawn once.
    assert_eq!(count(&pdf, b"3 Tr"), 0);
}

#[test]
fn vector_pdf_is_much_smaller_than_raster() {
    let mut p = deck(5);
    add(&mut p, 1, rect(900, 40.0, 300.0, ShapeFill::solid(ColorRef::rgb(Rgba::rgb(0x20, 0x60, 0xC0)))));
    let o = PdfOptions { dpi: 200.0, ..Default::default() };
    let vector = export(&p, &o).expect("vector");
    let raster = export(&p, &PdfOptions { vector: false, ..o }).expect("raster");
    assert!(vector.len() * 5 < raster.len(), "vector {} raster {}", vector.len(), raster.len());
}

#[test]
fn a_shadow_rasterizes_that_shape_only() {
    let mut p = deck(1);
    let mut sh = rect(900, 100.0, 300.0, ShapeFill::solid(ColorRef::rgb(Rgba::rgb(0xF0, 0x80, 0x20))));
    sh.effects = Some(shadow());
    sh.text = Some(TextBody::from_text("Shadowed"));
    add(&mut p, 0, sh);
    let pdf = plain(&p, &PdfOptions { dpi: 72.0, ..opts(PageLayout::Slides) });
    let imgs = images(&pdf);
    // One local raster (and its soft mask), far smaller than the 960 × 540 page.
    assert!(!imgs.is_empty() && imgs.len() <= 2, "{imgs:?}");
    assert!(imgs.iter().all(|(w, h)| *w > 200 && *w < 400 && *h < 250), "{imgs:?}");
    // The shape's text stays text.
    assert!(count(&pdf, format!("<{:04X}>", 'S' as u32).as_bytes()) > 0);
    assert!(count(&pdf, format!("<{:04X}>", 'w' as u32).as_bytes()) > 0);
}

#[test]
fn pattern_fills_rasterize_their_area_only() {
    let mut p = deck(1);
    let pat = deckcraft_model::style::PatternFill {
        preset: "dkHorz".into(),
        fg: ColorRef::rgb(Rgba::rgb(0x20, 0x20, 0x20)),
        bg: ColorRef::rgb(Rgba::rgb(0xF0, 0xF0, 0xF0)),
    };
    add(&mut p, 0, rect(900, 100.0, 300.0, ShapeFill::Pattern(pat)));
    let pdf = plain(&p, &PdfOptions { dpi: 72.0, ..opts(PageLayout::Slides) });
    let imgs = images(&pdf);
    assert!(!imgs.is_empty() && imgs.iter().all(|(w, h)| *w <= 201 && *h <= 101), "{imgs:?}");
}

#[test]
fn jpeg_pictures_are_embedded_as_they_are() {
    let mut p = deck(1);
    let px: Vec<u8> = (0..64 * 48).flat_map(|i| [(i % 64 * 4) as u8, (i / 64 * 5) as u8, 128, 255]).collect();
    let jpeg = deckcraft_render::Image { width: 64, height: 48, pixels: px }.to_jpeg(90);
    assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    let media = p.add_media("photo.jpg", "image/jpeg", jpeg.clone());
    let pic = Shape {
        id: ShapeId(900),
        xfrm: Some(Xfrm::new(100.0, 100.0, 320.0, 240.0)),
        kind: ShapeKind::Picture { fill: PictureFill { media, crop: [0.1, 0.0, 0.0, 0.1], ..Default::default() } },
        ..Default::default()
    };
    add(&mut p, 0, pic);
    let pdf = plain(&p, &opts(PageLayout::Slides));
    assert_eq!(count(&pdf, b"/DCTDecode"), 1);
    assert_eq!(images(&pdf), vec![(64, 48)]);
    // The file itself, byte for byte.
    assert_eq!(count(&pdf, &jpeg[..64]), 1);
    // Cropped: drawn under a clip.
    assert!(count(&pdf, b"W n") + count(&pdf, b"W\nn") > 0);
}

#[test]
fn links_and_bookmarks_survive_vector_export() {
    let mut p = deck(3);
    let mut sh = rect(900, 40.0, 300.0, ShapeFill::solid(ColorRef::rgb(Rgba::rgb(0x20, 0x60, 0xC0))));
    sh.click = Some(Hyperlink {
        action: deckcraft_model::text::Action::Url { url: "https://example.org/deck".into() },
        tooltip: String::new(),
        highlight_click: false,
    });
    add(&mut p, 0, sh);
    let mut next = rect(901, 300.0, 300.0, ShapeFill::solid(ColorRef::rgb(Rgba::rgb(0x20, 0xA0, 0x60))));
    next.click = Some(Hyperlink { action: deckcraft_model::text::Action::LastSlide, tooltip: String::new(), highlight_click: false });
    add(&mut p, 0, next);
    for layout in [PageLayout::Slides, PageLayout::Handouts { per_page: 6 }] {
        let pdf = plain(&p, &opts(layout));
        assert_eq!(count(&pdf, b"https://example.org/deck"), 1, "{layout:?}");
        assert!(count(&pdf, b"/Subtype/Link") >= 2, "{layout:?}");
        assert!(count(&pdf, b"/Outlines") > 0, "{layout:?}");
        assert!(count(&pdf, b"Slide title 2") > 0, "{layout:?}");
    }
}

#[test]
fn handout_boxes_fit_the_paper() {
    for n in [1, 2, 3, 4, 6, 9] {
        let v = handout_boxes(n, (612.0, 792.0), 16.0 / 9.0);
        assert_eq!(v.len(), n as usize);
        for r in &v {
            assert!(r.x0 >= 0.0 && r.y0 >= 0.0 && r.x1 <= 612.0 && r.y1 <= 792.0, "{n}: {r:?}");
            assert!((r.width() / r.height() - 16.0 / 9.0).abs() < 1e-6);
        }
        for (i, a) in v.iter().enumerate() {
            for b in &v[i + 1..] {
                assert!(a.intersect(*b).area() <= 0.0, "{n}: overlap");
            }
        }
    }
}

/// A placeable WMF, 2 x 1 inch: a red rectangle over the left half.
fn half_red_wmf() -> Vec<u8> {
    fn rec(func: u16, params: &[i16]) -> Vec<u8> {
        let mut v = ((3 + params.len()) as u32).to_le_bytes().to_vec();
        v.extend(func.to_le_bytes());
        for p in params {
            v.extend(p.to_le_bytes());
        }
        v
    }
    let body = [rec(0x02FC, &[0, 0xFF, 0, 0]), rec(0x012D, &[0]), rec(0x041B, &[1440, 1440, 0, 0]), rec(0, &[])].concat();
    let mut f = 0x9AC6_CDD7u32.to_le_bytes().to_vec();
    f.extend(0u16.to_le_bytes());
    for v in [0i16, 0, 2880, 1440] {
        f.extend(v.to_le_bytes());
    }
    f.extend(1440u16.to_le_bytes());
    f.extend(0u32.to_le_bytes());
    f.extend(0u16.to_le_bytes());
    let total = (f.len() + 18 + body.len()) / 2;
    for v in [1u16, 9, 0x300] {
        f.extend(v.to_le_bytes());
    }
    f.extend((total as u32).to_le_bytes());
    f.extend(16u16.to_le_bytes());
    f.extend(0u32.to_le_bytes());
    f.extend(0u16.to_le_bytes());
    f.extend(body);
    f
}

/// What the renderer draws only as pixels (pattern fills, rectangle path gradients, text shadows,
/// WMF pictures, colour emoji) exports as local images in a vector PDF: nothing panics, nothing is
/// dropped, and the text around stays text.
#[test]
fn pixel_only_drawing_exports_as_local_images() {
    use deckcraft_model::style::{PatternFill, Shadow};
    let mut p = deck(1);
    let pat = PatternFill { preset: "dkVert".into(), fg: ColorRef::rgb(Rgba::rgb(0x20, 0x20, 0x20)), bg: ColorRef::rgb(Rgba::rgb(0xF0, 0xF0, 0xF0)) };
    add(&mut p, 0, rect(900, 40.0, 300.0, ShapeFill::Pattern(pat)));
    let stops = vec![
        GradientStop { pos: 0.0, color: ColorRef::rgb(Rgba::rgb(255, 0, 0)) },
        GradientStop { pos: 1.0, color: ColorRef::rgb(Rgba::rgb(0, 0, 255)) },
    ];
    let path = Gradient { stops, shape: GradientShape::Path { path: "rect".into(), focus: [0.5, 0.5, 0.5, 0.5] }, rotate_with_shape: true };
    add(&mut p, 0, rect(901, 260.0, 300.0, ShapeFill::Gradient(path)));
    let mut text = Shape { id: ShapeId(902), xfrm: Some(Xfrm::new(480.0, 280.0, 440.0, 120.0)), text_box: true, ..Default::default() };
    let mut body = TextBody::from_text("Shade \u{1F600} me");
    body.paragraphs[0].runs[0].props.size = Some(40.0);
    body.paragraphs[0].runs[0].props.shadow =
        Some(Shadow { color: ColorRef::rgb(Rgba { r: 0, g: 0, b: 0, a: 160 }), blur: 4.0, dist: 4.0, ..shadow_of() });
    text.text = Some(body);
    add(&mut p, 0, text);
    let media = p.add_media("image1.wmf", "image/x-wmf", half_red_wmf());
    let pic = Shape {
        id: ShapeId(903),
        xfrm: Some(Xfrm::new(480.0, 420.0, 200.0, 100.0)),
        kind: ShapeKind::Picture { fill: PictureFill { media, ..Default::default() } },
        ..Default::default()
    };
    add(&mut p, 0, pic);
    let pdf = plain(&p, &PdfOptions { dpi: 72.0, ..opts(PageLayout::Slides) });
    let imgs = images(&pdf);
    // Pattern, path gradient, text shadow, metafile (emoji when a colour font is installed),
    // each a local image well under the page size, soft masks included.
    assert!(imgs.len() >= 4, "{imgs:?}");
    assert!(imgs.iter().all(|(w, h)| *w < 600 && *h < 300), "{imgs:?}");
    for c in "Shademe".chars() {
        assert!(count(&pdf, format!("<{:04X}>", c as u32).as_bytes()) > 0, "{c} is text");
    }
}

fn shadow_of() -> Shadow {
    shadow().outer_shadow.expect("shadow")
}
