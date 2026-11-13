use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.drawing.gpu",
    deps: &["system.drawing", "system.gpu"],
    core_language: false,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/drawing/gpu_drawing_presentation.dream",
            include_str!("../system/drawing/gpu_drawing_presentation.dream"),
        ),
        (
            "<std>/system/drawing/drawing_present.dream",
            include_str!("../system/drawing/drawing_present.dream"),
        ),
        (
            "<std>/system/drawing/drawing_surface_gpu.dream",
            include_str!("../system/drawing/drawing_surface_gpu.dream"),
        ),
    ],
};
