use serde_json::{Value, json};
use vizcraft_diagram::base64;

use crate::Session;

/// A `w` by `h` PNG: the left half red, the right half blue, opaque unless `alpha`.
fn png(w: u32, h: u32, alpha: bool) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, _| {
        let a = if alpha { 128 } else { 255 };
        if x < w / 2 { image::Rgba([255, 0, 0, a]) } else { image::Rgba([0, 0, 255, a]) }
    });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

/// A busy opaque PNG that compresses badly as PNG.
fn photo(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        let v = (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)).rotate_left(x % 13) as u8;
        image::Rgb([v, ((x + y) % 256) as u8, (x * y % 251) as u8])
    });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([20, 200, 40]));
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).encode_image(&img).unwrap();
    out
}

fn insert(s: &mut Session, bytes: &[u8], extra: Value) -> Value {
    let mut p = json!({"bytes": base64::encode(bytes), "x": 100, "y": 100});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    s.run("picture.insert", &p).unwrap()
}

/// The centre pixel of the picture shape `id`, as the canvas rasterises it, unpremultiplied.
fn centre(s: &Session, id: &str) -> [u8; 4] {
    let page = s.doc.page().unwrap();
    let sh = page.shape(id).unwrap();
    let img = sh.image.as_ref().unwrap();
    let asset = s.doc.image(&img.asset).unwrap();
    let px = super::raster(asset, img, sh.w, sh.h, 20, 20).unwrap();
    let i = (10 * 20 + 10) * 4;
    let p = [px[i], px[i + 1], px[i + 2], px[i + 3]];
    let a = u32::from(p[3]).max(1);
    let un = |c: u8| ((u32::from(c) * 255 + a / 2) / a).min(255) as u8;
    [un(p[0]), un(p[1]), un(p[2]), p[3]]
}

#[test]
fn a_png_inserted_from_bytes_is_stored_once_and_round_trips() {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let bytes = png(40, 30, false);
    let a = insert(&mut s, &bytes, json!({}));
    let b = insert(&mut s, &bytes, json!({"x": 300}));
    assert_eq!(a["asset"], b["asset"]);
    assert_eq!(b["reused"], true);
    assert_eq!(s.doc.images.len(), 1);
    let sh = s.doc.page().unwrap().shape(a["id"].as_str().unwrap()).unwrap().clone();
    assert_eq!((sh.w, sh.h), (40.0, 30.0));
    assert_eq!(sh.style.line.a, 0, "a transparent outline");
    let saved = crate::io::save_bytes("p.vizcraft", &s.doc).unwrap();
    let back = crate::io::load_bytes("p.vizcraft", &saved).unwrap();
    assert_eq!(back.images, s.doc.images);
    assert_eq!(back.page().unwrap().shape(&sh.id).unwrap().image, sh.image);
}

#[test]
fn a_large_picture_is_scaled_to_a_quarter_of_the_frame_and_jpeg_and_svg_read_their_size() {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let f = s.doc.page().unwrap().frame();
    let big = insert(&mut s, &png(4000, 1000, false), json!({}));
    let sh = s.doc.page().unwrap().shape(big["id"].as_str().unwrap()).unwrap();
    assert!(sh.w <= f.width() / 2.0 + 1e-6 && sh.h <= f.height() / 2.0 + 1e-6);
    assert!((sh.w / sh.h - 4.0).abs() < 1e-6);
    let j = insert(&mut s, &jpeg(48, 24), json!({}));
    let sh = s.doc.page().unwrap().shape(j["id"].as_str().unwrap()).unwrap();
    assert_eq!((sh.w, sh.h), (48.0, 24.0));
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><rect width="80" height="40" fill="#00ff00"/></svg>"##;
    let v = insert(&mut s, svg, json!({}));
    let asset = s.doc.image(v["asset"].as_str().unwrap()).unwrap();
    assert_eq!(asset.media_type, "image/svg+xml");
    assert_eq!(asset.bytes, svg.to_vec(), "SVG is kept as text");
    let c = centre(&s, v["id"].as_str().unwrap());
    assert!(c[1] > 200 && c[0] < 50, "{c:?}");
}

#[test]
fn crop_and_adjust_round_trip_and_change_the_pixels() {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let r = insert(&mut s, &png(40, 20, false), json!({"w": 40, "h": 20}));
    let id = r["id"].as_str().unwrap().to_string();
    // The centre sits on the boundary; the right half's centre is blue after a crop to it.
    s.run("picture.crop", &json!({"id": id, "crop": {"x": 0.5, "y": 0, "w": 0.5, "h": 1}})).unwrap();
    let c = centre(&s, &id);
    assert!(c[2] > 200 && c[0] < 30, "{c:?}");
    s.run("picture.crop", &json!({"id": id, "crop": {"x": 0, "y": 0, "w": 0.5, "h": 1}})).unwrap();
    let red = centre(&s, &id);
    assert!(red[0] > 200 && red[2] < 30, "{red:?}");
    s.run("picture.adjust", &json!({"id": id, "recolor": "grayscale"})).unwrap();
    let g = centre(&s, &id);
    assert!(g[0] == g[1] && g[1] == g[2] && g[0] < 100, "{g:?}");
    s.run("picture.adjust", &json!({"id": id, "brightness": 0.5})).unwrap();
    let bright = centre(&s, &id);
    assert!(bright[0] > g[0] + 60, "{bright:?} vs {g:?}");
    s.run("picture.adjust", &json!({"id": id, "transparency": 0.5})).unwrap();
    assert!((i32::from(centre(&s, &id)[3]) - 128).abs() <= 2);
    let saved = crate::io::save_bytes("p.vizcraft", &s.doc).unwrap();
    let back = crate::io::load_bytes("p.vizcraft", &saved).unwrap();
    let img = back.page().unwrap().shape(&id).unwrap().image.clone().unwrap();
    assert_eq!(Some(&img), s.doc.page().unwrap().shape(&id).unwrap().image.as_ref());
    assert_eq!(img.crop.w, 0.5);
    assert_eq!(img.adjust.brightness, 0.5);
    s.run("picture.reset", &json!({"id": id})).unwrap();
    let img = s.doc.page().unwrap().shape(&id).unwrap().image.clone().unwrap();
    assert!(img.adjust.is_neutral() && img.crop == vizcraft_diagram::CropRect::FULL);
    assert!(s.run("picture.adjust", &json!({"id": id, "recolor": "plaid"})).is_err());
    assert!(s.run("picture.crop", &json!({"id": id, "crop": {"x": 1, "w": 0}})).is_err());
}

#[test]
fn change_swaps_the_asset_and_drops_the_old_one() {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let r = insert(&mut s, &png(10, 10, false), json!({}));
    let id = r["id"].as_str().unwrap();
    let c = s.run("picture.change", &json!({"id": id, "bytes": base64::encode(&jpeg(8, 8))})).unwrap();
    assert_ne!(c["asset"], r["asset"]);
    assert_eq!(s.doc.images.len(), 1);
    assert_eq!(s.doc.images[0].media_type, "image/jpeg");
}

#[test]
fn compress_shrinks_the_asset() {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let r = insert(&mut s, &photo(600, 400), json!({}));
    let before = s.doc.images[0].bytes.len();
    let out = s.run("picture.compress", &json!({"ids": [r["id"]], "maxDimension": 120, "quality": 70})).unwrap();
    let after = s.doc.images[0].bytes.len();
    assert!(after < before / 4, "{before} -> {after}: {out}");
    assert_eq!(s.doc.images[0].media_type, "image/jpeg");
    assert_eq!(vizcraft_formats::svg::pictures::pixel_size(&s.doc.images[0].bytes, "image/jpeg"), Some((120.0, 80.0)));
    // A picture with transparency stays PNG.
    let mut s = Session::new(vizcraft_diagram::Document::new());
    insert(&mut s, &png(600, 600, true), json!({}));
    let r = s.run("picture.compress", &json!({"all": true, "maxDimension": 50})).unwrap();
    assert_eq!(s.doc.images[0].media_type, "image/png", "{r}");
}

#[test]
fn png_and_pdf_export_draw_the_picture() {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    let r = insert(&mut s, &png(40, 40, false), json!({"x": 100, "y": 100, "w": 40, "h": 40}));
    s.run("picture.crop", &json!({"id": r["id"], "crop": {"x": 0.5, "y": 0, "w": 0.5, "h": 1}})).unwrap();
    let idx = s.doc.active_index();
    let png_bytes = crate::io::export_page_bytes("p.png", None, &s.doc, idx, 1.0).unwrap();
    let img = image::load_from_memory(&png_bytes).unwrap().to_rgba8();
    // The export's canvas starts at the page origin on a fixed board, else at the drawing's
    // bounds less the margin; find the picture's centre either way.
    let canvas = vizcraft_formats::svg::canvas_rect(s.doc.page().unwrap());
    let (x, y) = ((120.0 - canvas.x0) as u32, (120.0 - canvas.y0) as u32);
    let p = img.get_pixel(x, y).0;
    assert!(p[2] > 200 && p[0] < 30, "blue at the centre of the cropped picture: {p:?}");
    let pdf = crate::io::export_bytes("p.pdf", None, &s.doc).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Subtype /Image") || text.contains("/Subtype/Image"), "an image object in the PDF");
}

#[test]
fn junk_pictures_are_errors_not_panics() {
    let mut s = Session::new(vizcraft_diagram::Document::new());
    for p in [
        json!({"bytes": "%%%"}),
        json!({"bytes": base64::encode(b"not a picture at all")}),
        json!({"bytes": base64::encode(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0])}),
        json!({"bytes": base64::encode(&[0xff, 0xd8, 0xff, 0xe0, 0xff])}),
        json!({"bytes": base64::encode(b"<svg"), "mediaType": "image/svg+xml"}),
        json!({"bytes": base64::encode(&png(4, 4, false)), "mediaType": "image/tiff"}),
        json!({"bytes": "A".repeat(23_000_000)}),
        json!({"bytes": "data:image/png;base64"}),
        json!({"path": "/nonexistent/picture.png"}),
        json!({}),
    ] {
        assert!(s.run("picture.insert", &p).is_err(), "{}", p.to_string().chars().take(80).collect::<String>());
    }
    assert!(s.doc.images.is_empty());
    let r = insert(&mut s, &png(4, 4, false), json!({"w": 1e308, "h": -1e308, "x": f64::MAX}));
    assert!(r["id"].is_string());
    for cmd in ["picture.crop", "picture.adjust", "picture.reset", "picture.change", "picture.compress"] {
        let _ = s.run(cmd, &json!({"id": r["id"], "crop": {"x": 1e308, "y": -1e308, "w": "x"}, "brightness": 1e308, "bytes": "%%"}));
        let _ = s.run(cmd, &json!({"id": "nope"}));
    }
}
