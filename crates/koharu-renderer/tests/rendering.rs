use std::{collections::BTreeMap, io::Cursor, sync::Arc};

use koharu_rasterizer::{PREPARED_RASTER_TILE_DIMENSION, RasterOptions, Rasterizer};
use koharu_renderer::{ImageKind, LayerKind, Renderer};
use koharu_scene::{
    AssetInput, AssetMetadata, AssetRole, At, Origin, PageDraft, RasterLayer, RasterLayerKind,
    Session,
};

fn png(width: u32, height: u32, color: [u8; 4]) -> Arc<[u8]> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba(color));
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner().into()
}

fn png_image(image: image::RgbaImage) -> Arc<[u8]> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner().into()
}

#[tokio::test]
async fn public_renderer_returns_a_complete_immutable_frame() {
    let mut session = Session::memory().await.unwrap();
    let mut page = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            page = Some(edit.add_page(PageDraft::new("page", 320.0, 240.0), At::End)?);
            Ok(())
        })
        .unwrap();
    let snapshot = session.commit(patch).await.unwrap().snapshot;
    let page = page.unwrap();

    let renderer = Renderer::new().unwrap();
    let frame = renderer.render(&snapshot, page).await.unwrap();

    assert_eq!(frame.page(), page);
    assert_eq!(frame.revision(), snapshot.revision());
    assert_eq!(frame.size(), (320, 240));
    assert_eq!(frame.origin(), (0, 0));
    assert!(frame.layers().is_empty());
    assert!(frame.layer(page).is_none());
    assert!(frame
        .diagnostics()
        .iter()
        .any(|diagnostic| matches!(diagnostic, koharu_renderer::RenderDiagnostic::MissingAsset { entity, role } if *entity == page && role == "source")));
}

#[tokio::test]
async fn native_rasterization_preserves_pixels_across_prepared_tile_edges() {
    let width = PREPARED_RASTER_TILE_DIMENSION + 1;
    let red = [230, 20, 30, 255];
    let blue = [10, 40, 220, 255];
    let mut source_image = image::RgbaImage::from_pixel(width, 1, image::Rgba(red));
    source_image.put_pixel(width - 1, 0, image::Rgba(blue));
    let source = AssetRole::new("source").unwrap();
    let mut session = Session::memory().await.unwrap();
    let mut page = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            let created = edit.add_page(PageDraft::new("page", width as f64, 1.0), At::End)?;
            edit.set_asset(
                created,
                &source,
                AssetInput::new(
                    png_image(source_image),
                    "image/png",
                    AssetMetadata {
                        width: Some(width),
                        height: Some(1),
                        attributes: BTreeMap::new(),
                    },
                ),
            )?;
            page = Some(created);
            Ok(())
        })
        .unwrap();
    let snapshot = session.commit(patch).await.unwrap().snapshot;
    let frame = Renderer::new()
        .unwrap()
        .render(&snapshot, page.unwrap())
        .await
        .unwrap();
    let manifest = frame.prepared().manifest().unwrap();
    assert_eq!(manifest.resources.len(), 1);
    let koharu_rasterizer::PreparedContent::Raster(prepared) = &manifest.frame.layers[0].content
    else {
        panic!("source layer must remain raster");
    };
    assert_eq!(prepared.tiles.len(), 2);

    let raster = Rasterizer::new()
        .unwrap()
        .rasterize(&frame.raster_frame().unwrap(), RasterOptions::default())
        .unwrap();

    assert_eq!(raster.image.get_pixel(width - 2, 0).0, red);
    assert_eq!(raster.image.get_pixel(width - 1, 0).0, blue);
}

#[tokio::test]
async fn discarded_image_nodes_are_rebuilt_for_a_reopened_presentation() {
    let mut session = Session::memory().await.unwrap();
    let source = AssetRole::new("source").unwrap();
    let color = [12, 34, 56, 255];
    let mut page = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            let created = edit.add_page(PageDraft::new("page", 4.0, 4.0), At::End)?;
            edit.set_asset(
                created,
                &source,
                AssetInput::new(
                    png(4, 4, color),
                    "image/png",
                    AssetMetadata {
                        width: Some(4),
                        height: Some(4),
                        attributes: BTreeMap::new(),
                    },
                ),
            )?;
            page = Some(created);
            Ok(())
        })
        .unwrap();
    let snapshot = session.commit(patch).await.unwrap().snapshot;
    let page = page.unwrap();
    let renderer = Renderer::new().unwrap();

    renderer.render(&snapshot, page).await.unwrap();
    renderer.discard_retained_nodes();
    let reopened = renderer.render(&snapshot, page).await.unwrap();
    let raster = Rasterizer::new()
        .unwrap()
        .rasterize(&reopened.raster_frame().unwrap(), RasterOptions::default())
        .unwrap();

    assert_eq!(reopened.stats().rebuilt_layers, 1);
    assert!(raster.image.pixels().all(|pixel| pixel.0 == color));
}

#[tokio::test]
async fn rasterized_png_contains_source_and_cleanup_pixels() {
    let mut session = Session::memory().await.unwrap();
    let source = AssetRole::new("source").unwrap();
    let mut cleanup = image::RgbaImage::new(4, 4);
    cleanup.put_pixel(2, 1, image::Rgba([240, 30, 20, 255]));
    let mut page = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            let created = edit.add_page(PageDraft::new("page", 4.0, 4.0), At::End)?;
            edit.set_asset(
                created,
                &source,
                AssetInput::new(
                    png(4, 4, [10, 20, 200, 255]),
                    "image/png",
                    AssetMetadata {
                        width: Some(4),
                        height: Some(4),
                        attributes: BTreeMap::new(),
                    },
                ),
            )?;
            let cleanup_layer = edit.add_entity(created, At::Start)?;
            edit.set(
                cleanup_layer,
                &RasterLayer {
                    origin: Origin::User,
                    name: "Cleanup".to_owned(),
                    kind: RasterLayerKind::Cleanup,
                },
            )?;
            edit.set_asset(
                cleanup_layer,
                &source,
                AssetInput::new(
                    png_image(cleanup),
                    "image/png",
                    AssetMetadata {
                        width: Some(4),
                        height: Some(4),
                        attributes: BTreeMap::new(),
                    },
                ),
            )?;
            page = Some(created);
            Ok(())
        })
        .unwrap();
    let snapshot = session.commit(patch).await.unwrap().snapshot;
    let renderer = Renderer::new().unwrap();
    let frame = renderer.render(&snapshot, page.unwrap()).await.unwrap();
    let raster = Rasterizer::new()
        .unwrap()
        .rasterize(&frame.raster_frame().unwrap(), RasterOptions::default())
        .unwrap();

    assert!(frame.layers().iter().any(|layer| matches!(
        layer.kind(),
        LayerKind::Image(metadata) if metadata.kind == ImageKind::Source
    )));
    assert!(frame.layers().iter().any(|layer| matches!(
        layer.kind(),
        LayerKind::Image(metadata) if metadata.kind == ImageKind::Cleanup
    )));
    assert_eq!(raster.image.get_pixel(0, 0).0, [10, 20, 200, 255]);
    assert_eq!(raster.image.get_pixel(2, 1).0, [240, 30, 20, 255]);
}

#[test]
fn public_layer_kinds_remain_export_metadata() {
    let _ = LayerKind::Image(koharu_renderer::ImageMetadata {
        name: None,
        kind: ImageKind::Source,
    });
}

/// Renders `text` as a translated auto-fit paragraph inside a `width` x `height`
/// frame and reports what the renderer chose.
struct ThaiFit {
    font_size: f32,
    estimated_lines: f32,
    overflow: bool,
    below_readable: bool,
    fonts: Vec<String>,
}

async fn fit_text(text: &str, language: &str, width: f64, height: f64) -> ThaiFit {
    use koharu_scene::{
        Authored, Geometry, LanguageTag, SourceText, TextLayout, TextLayoutKind, Translation,
    };

    let mut session = Session::memory().await.unwrap();
    let mut ids = None;
    let patch = session
        .snapshot()
        .patch(|edit| {
            let page = edit.add_page(PageDraft::new("page", 600.0, 600.0), At::End)?;
            let content = edit.add_text_content(page, At::End)?;
            edit.set(
                content,
                &SourceText {
                    text: Authored::user("元".to_owned()),
                    language: None,
                },
            )?;
            edit.set(
                content,
                &Translation {
                    text: Authored::user(text.to_owned()),
                    language: Some(LanguageTag::new(language).unwrap()),
                },
            )?;
            let layer = edit.add_text_layer(
                page,
                At::End,
                content,
                &TextLayout {
                    origin: Origin::User,
                    kind: TextLayoutKind::Paragraph,
                    angle_degrees: Some(0.0),
                },
            )?;
            edit.set(layer, &Geometry::rectangle(20.0, 20.0, width, height))?;
            ids = Some((page, layer));
            Ok(())
        })
        .unwrap();
    let snapshot = session.commit(patch).await.unwrap().snapshot;
    let (page, layer) = ids.unwrap();
    let frame = Renderer::new()
        .unwrap()
        .render(&snapshot, page)
        .await
        .unwrap();
    let layer_ref = frame.layer(layer).expect("text layer rendered");
    let LayerKind::Text(meta) = layer_ref.kind() else {
        panic!("expected text layer");
    };
    let diagnostics = frame.diagnostics();
    ThaiFit {
        font_size: meta.font_size,
        // Thai line pitch is about 1.2em in the default metrics.
        estimated_lines: meta.layout_bounds.height / (meta.font_size * 1.2),
        overflow: diagnostics
            .iter()
            .any(|d| matches!(d, koharu_renderer::RenderDiagnostic::TextOverflow { entity, .. } if *entity == layer)),
        below_readable: diagnostics.iter().any(|d| {
            matches!(d, koharu_renderer::RenderDiagnostic::TextBelowReadableSize { entity, .. } if *entity == layer)
        }),
        fonts: meta.post_script_fonts.clone(),
    }
}

const THAI_SAMPLES: &[(&str, &str)] = &[
    ("short", "ไม่ต้องห่วงหรอก"),
    (
        "medium-spaced",
        "ฉันไม่เคยคิดเลยว่าวันนี้ จะมาถึงเร็วขนาดนี้ ขอบคุณทุกคนมากนะ",
    ),
    (
        "long-unspaced",
        "เมื่อวานนี้ฉันไปที่โรงเรียนและได้พบกับเพื่อนเก่าที่ไม่ได้เจอกันมานานหลายปีเขาดูแข็งแรงดีมากเลย",
    ),
];

#[tokio::test]
async fn thai_bubble_text_report() {
    for (name, text) in THAI_SAMPLES {
        for (w, h) in [(120.0, 160.0), (160.0, 100.0)] {
            let fit = fit_text(text, "th", w, h).await;
            eprintln!(
                "THAI {name:14} box {w:>4}x{h:<4} size {:>6.2} lines~{:>4.1} overflow {} below_min {} fonts {:?}",
                fit.font_size, fit.estimated_lines, fit.overflow, fit.below_readable, fit.fonts
            );
        }
    }
    let en = fit_text(
        "I never thought this day would come so fast. Thank you all!",
        "en",
        120.0,
        160.0,
    )
    .await;
    eprintln!(
        "ENGLISH control size {:.2} overflow {} fonts {:?}",
        en.font_size, en.overflow, en.fonts
    );
}

#[tokio::test]
async fn thai_text_uses_a_thai_capable_font() {
    let fit = fit_text(THAI_SAMPLES[1].1, "th", 160.0, 200.0).await;
    assert!(
        fit.fonts.iter().any(|font| {
            let font = font.to_ascii_lowercase();
            [
                "thai",
                "leelawadee",
                "tahoma",
                "sarabun",
                "angsana",
                "browallia",
                "cordia",
                "norasi",
                "loma",
            ]
            .iter()
            .any(|name| font.contains(name))
        }),
        "no Thai-capable font among {:?}",
        fit.fonts
    );
}

#[tokio::test]
async fn thai_text_in_a_bubble_stays_readable_without_overflow() {
    for (name, text) in THAI_SAMPLES {
        let fit = fit_text(text, "th", 160.0, 200.0).await;
        assert!(
            !fit.overflow,
            "{name}: overflowed at size {:.2}",
            fit.font_size
        );
        assert!(
            fit.font_size >= 12.0,
            "{name}: font shrank to {:.2}px",
            fit.font_size
        );
    }
}

/// Traditional Thai faces (Angsana/Browallia/Cordia families) have a tiny x-height, so
/// Thai text set in them looks about half the size of the requested point size.
#[tokio::test]
async fn thai_text_is_not_set_in_a_small_x_height_face_when_a_normal_one_exists() {
    let fit = fit_text(THAI_SAMPLES[1].1, "th", 160.0, 200.0).await;
    let position = |names: &[&str]| {
        fit.fonts.iter().position(|font| {
            let font = font.to_ascii_lowercase();
            names.iter().any(|name| font.contains(name))
        })
    };
    let small = position(&[
        "angsana",
        "browallia",
        "cordia",
        "dillenia",
        "eucrosia",
        "freesia",
        "iris",
        "jasmine",
        "kodchiang",
        "lily",
        "norasi",
    ]);
    let normal = position(&[
        "leelawadee",
        "notosansthai",
        "tahoma",
        "sarabun",
        "thonburi",
        "garuda",
        "loma",
    ]);
    if let (Some(small), Some(normal)) = (small, normal) {
        assert!(
            normal < small,
            "small-x-height face ahead of a normal one: {:?}",
            fit.fonts
        );
    }
    assert!(normal.is_some(), "no regular Thai face in {:?}", fit.fonts);
}
