//! UI-agnostic state for the built-in image viewer.
//!
//! Decodes a `FileKind::Image` file into an RGBA pixel buffer via the
//! `image` crate. GUI-only: the TUI keeps handing images off to the OS
//! default application (see `.claude/plans/inherited-painting-lark.md`
//! Phase 6's "Order and dependency reasoning" for why terminal cell grids
//! can't show real pixels without a graphics protocol this pass doesn't
//! implement). Lives in `src/app/` rather than `src/ui/gui/` because
//! decoding is pure business logic with no rendering dependency, matching
//! `EditorState`'s placement.

use std::path::PathBuf;

use crate::support::error::{CastermError, Result};

/// A decoded image ready for GPU upload: raw RGBA8 bytes, tightly packed
/// row-major, plus its pixel dimensions.
pub struct ImageState {
    path: PathBuf,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl ImageState {
    /// Decode `path` into an RGBA8 buffer. Any read or decode failure is
    /// propagated as a clear error — no silent fallback to a blank image.
    pub fn load(path: PathBuf) -> Result<Self> {
        let img = image::open(&path)
            .map_err(|e| CastermError::Gui(format!("failed to open image {path:?}: {e}")))?
            .to_rgba8();
        let (width, height) = img.dimensions();
        Ok(Self {
            path,
            width,
            height,
            rgba: img.into_raw(),
        })
    }

    /// Unused until a future window-title/status-bar treatment surfaces the
    /// open file's path (no such treatment exists yet for the editor's
    /// `EditorState::path` either, on the GUI side) — kept as public API
    /// now rather than deferred, matching `EditorState`'s shape.
    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Tightly packed row-major RGBA8 bytes, `width * height * 4` long.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Compute a fit-to-window (letterboxed, aspect-ratio-preserving)
/// destination rectangle for an image of `img_w x img_h` inside an
/// available area of `avail_w x avail_h`, both in the same units (pixels).
/// Returns `(x, y, w, h)` of the destination quad, offset within the
/// available area so the image is centered. No zoom/pan — MVP always fits
/// the whole image to the window, matching the plan's Phase 6 scope.
pub fn fit_to_window(img_w: u32, img_h: u32, avail_w: f32, avail_h: f32) -> (f32, f32, f32, f32) {
    if img_w == 0 || img_h == 0 || avail_w <= 0.0 || avail_h <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let img_w = img_w as f32;
    let img_h = img_h as f32;
    let scale = (avail_w / img_w).min(avail_h / img_h);
    let w = img_w * scale;
    let h = img_h * scale;
    let x = (avail_w - w) / 2.0;
    let y = (avail_h - h) / 2.0;
    (x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_to_window_scales_down_wider_image_to_available_width() {
        // 200x100 image (2:1) into a 100x100 area: width-constrained.
        let (x, y, w, h) = fit_to_window(200, 100, 100.0, 100.0);
        assert_eq!(w, 100.0);
        assert_eq!(h, 50.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 25.0);
    }

    #[test]
    fn fit_to_window_scales_down_taller_image_to_available_height() {
        // 100x200 image (1:2) into a 100x100 area: height-constrained.
        let (x, y, w, h) = fit_to_window(100, 200, 100.0, 100.0);
        assert_eq!(w, 50.0);
        assert_eq!(h, 100.0);
        assert_eq!(x, 25.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn fit_to_window_upscales_small_image_to_fill_available_area() {
        let (x, y, w, h) = fit_to_window(10, 10, 200.0, 200.0);
        assert_eq!((x, y, w, h), (0.0, 0.0, 200.0, 200.0));
    }

    #[test]
    fn fit_to_window_handles_zero_dimensions_without_panicking() {
        assert_eq!(fit_to_window(0, 10, 100.0, 100.0), (0.0, 0.0, 0.0, 0.0));
        assert_eq!(fit_to_window(10, 0, 100.0, 100.0), (0.0, 0.0, 0.0, 0.0));
        assert_eq!(fit_to_window(10, 10, 0.0, 100.0), (0.0, 0.0, 0.0, 0.0));
    }
}
