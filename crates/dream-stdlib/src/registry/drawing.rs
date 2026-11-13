use super::{StdPackage, WebAssets};

const WEB_ASSETS: WebAssets = WebAssets {
    name: "drawing browser host",
    env: "DREAM_DRAWING_WEB",
    installed_dir: "drawing",
    development_dir: "drawing-web",
    files: &[
        "dream_drawing.js",
        "dream_drawing_bg.wasm",
        "NotoSans-OFL.txt",
    ],
    build_hint: "run python scripts/build-drawing-web.py, or set DREAM_DRAWING_WEB to the built host directory",
};

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.drawing",
    deps: &["system", "system.collections"],
    core_language: false,
    web_assets: Some(&WEB_ASSETS),
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/drawing/effects.dream",
            include_str!("../system/drawing/effects.dream"),
        ),
        (
            "<std>/system/drawing/blend_mode.dream",
            include_str!("../system/drawing/blend_mode.dream"),
        ),
        (
            "<std>/system/drawing/gradient_stop.dream",
            include_str!("../system/drawing/gradient_stop.dream"),
        ),
        (
            "<std>/system/drawing/stroke_cap.dream",
            include_str!("../system/drawing/stroke_cap.dream"),
        ),
        (
            "<std>/system/drawing/stroke_join.dream",
            include_str!("../system/drawing/stroke_join.dream"),
        ),
        (
            "<std>/system/drawing/stroke_style.dream",
            include_str!("../system/drawing/stroke_style.dream"),
        ),
        (
            "<std>/system/drawing/backdrop.dream",
            include_str!("../system/drawing/backdrop.dream"),
        ),
        (
            "<std>/system/drawing/canvas.dream",
            include_str!("../system/drawing/canvas.dream"),
        ),
        (
            "<std>/system/drawing/color.dream",
            include_str!("../system/drawing/color.dream"),
        ),
        (
            "<std>/system/drawing/drawing_backend.dream",
            include_str!("../system/drawing/drawing_backend.dream"),
        ),
        (
            "<std>/system/drawing/drawing_error.dream",
            include_str!("../system/drawing/drawing_error.dream"),
        ),
        (
            "<std>/system/drawing/corner_radii.dream",
            include_str!("../system/drawing/corner_radii.dream"),
        ),
        (
            "<std>/system/drawing/text_metrics.dream",
            include_str!("../system/drawing/text_metrics.dream"),
        ),
        (
            "<std>/system/drawing/drawing_host.dream",
            include_str!("../system/drawing/drawing_host.dream"),
        ),
        (
            "<std>/system/drawing/drawing_path.dream",
            include_str!("../system/drawing/drawing_path.dream"),
        ),
        (
            "<std>/system/drawing/drawing_presentation.dream",
            include_str!("../system/drawing/drawing_presentation.dream"),
        ),
        (
            "<std>/system/drawing/drawing_resource.dream",
            include_str!("../system/drawing/drawing_resource.dream"),
        ),
        (
            "<std>/system/drawing/drawing_surface.dream",
            include_str!("../system/drawing/drawing_surface.dream"),
        ),
        (
            "<std>/system/drawing/fonts.dream",
            include_str!("../system/drawing/fonts.dream"),
        ),
        (
            "<std>/system/drawing/image.dream",
            include_str!("../system/drawing/image.dream"),
        ),
        (
            "<std>/system/drawing/paint.dream",
            include_str!("../system/drawing/paint.dream"),
        ),
        (
            "<std>/system/drawing/picture.dream",
            include_str!("../system/drawing/picture.dream"),
        ),
        (
            "<std>/system/drawing/rect.dream",
            include_str!("../system/drawing/rect.dream"),
        ),
        (
            "<std>/system/drawing/text_alignment.dream",
            include_str!("../system/drawing/text_alignment.dream"),
        ),
        (
            "<std>/system/drawing/text_layout.dream",
            include_str!("../system/drawing/text_layout.dream"),
        ),
        (
            "<std>/system/drawing/text_style.dream",
            include_str!("../system/drawing/text_style.dream"),
        ),
    ],
};
