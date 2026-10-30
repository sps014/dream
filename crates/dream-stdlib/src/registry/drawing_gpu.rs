use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.drawing.gpu",
    deps: &["system.drawing", "system.gpu"],
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
