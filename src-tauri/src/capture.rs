use crate::error::{AppError, AppResult};
use image::{ImageEncoder, RgbaImage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitorBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

pub struct FrozenCapture {
    pub generation: u64,
    pub image: RgbaImage,
    pub png: Arc<Vec<u8>>,
    pub image_token: String,
    pub bounds: MonitorBounds,
}

pub fn encode_png(image: &RgbaImage) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(&mut bytes, image::codecs::png::CompressionType::Fast, image::codecs::png::FilterType::Adaptive)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgba8)
        .map_err(|_| AppError::new("image_encode", "截图编码失败，请重新截图。", true))?;
    Ok(bytes)
}

pub fn pixel_rect(selection: &Selection, image_width: u32, image_height: u32) -> AppResult<(u32, u32, u32, u32)> {
    let values = [selection.x, selection.y, selection.width, selection.height, selection.viewport_width, selection.viewport_height];
    if values.iter().any(|n| !n.is_finite()) || selection.width <= 0.0 || selection.height <= 0.0
        || selection.viewport_width <= 0.0 || selection.viewport_height <= 0.0
        || selection.x < 0.0 || selection.y < 0.0
        || selection.x + selection.width > selection.viewport_width + 1.0
        || selection.y + selection.height > selection.viewport_height + 1.0
    {
        return Err(AppError::new("selection", "选区无效，请重新框选公式。", false));
    }
    let sx = image_width as f64 / selection.viewport_width;
    let sy = image_height as f64 / selection.viewport_height;
    let left = (selection.x * sx).floor().clamp(0.0, image_width as f64) as u32;
    let top = (selection.y * sy).floor().clamp(0.0, image_height as f64) as u32;
    let right = ((selection.x + selection.width) * sx).ceil().clamp(0.0, image_width as f64) as u32;
    let bottom = ((selection.y + selection.height) * sy).ceil().clamp(0.0, image_height as f64) as u32;
    let (width, height) = (right.saturating_sub(left), bottom.saturating_sub(top));
    if width < 8 || height < 8 {
        return Err(AppError::new("selection_small", "选区太小，请完整框住公式。", false));
    }
    if width as u64 * height as u64 > 16_000_000 {
        return Err(AppError::new("selection_large", "选区过大，请只框选需要识别的公式。", false));
    }
    Ok((left, top, width, height))
}

pub fn crop(capture: &FrozenCapture, selection: &Selection) -> AppResult<(Vec<u8>, u32, u32)> {
    let (x, y, width, height) = pixel_rect(selection, capture.image.width(), capture.image.height())?;
    let image = image::imageops::crop_imm(&capture.image, x, y, width, height).to_image();
    let png = encode_png(&image)?;
    if png.len() > 10 * 1024 * 1024 {
        return Err(AppError::new("selection_large", "截图超过 10 MiB，请缩小选区。", false));
    }
    Ok((png, width, height))
}

pub fn take_screenshot(point: (i32, i32)) -> AppResult<RgbaImage> {
    crate::platform::ensure_capture_permission()?;
    let (x, y) = point;
    let monitor = xcap::Monitor::from_point(x, y)
        .map_err(|_| AppError::new("monitor", "无法找到鼠标所在的显示器，请重试。", true))?;
    if monitor.width().unwrap_or(0) as u64 * monitor.height().unwrap_or(0) as u64 > 64_000_000 {
        return Err(AppError::new("display_large", "显示器分辨率过大，请降低分辨率后重试。", false));
    }
    let image = monitor.capture_image()
        .map_err(|_| AppError::new("capture_failed", "无法截取屏幕。Mac 请检查屏幕录制权限；受保护的窗口可能无法截取。", true))?;
    if image.width() == 0 || image.height() == 0 {
        return Err(AppError::new("empty_capture", "没有获得有效截图，请检查屏幕权限后重试。", true));
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_css_to_native_pixels_independently_per_axis() {
        let s = Selection { x: 10.2, y: 20.4, width: 100.2, height: 50.2, viewport_width: 1536.0, viewport_height: 864.0 };
        assert_eq!(pixel_rect(&s, 3840, 2160).unwrap(), (25, 51, 251, 126));
        assert!(pixel_rect(&Selection { x: f64::NAN, ..s.clone() }, 3840, 2160).is_err());
        assert!(pixel_rect(&Selection { x: -3.0, ..s.clone() }, 3840, 2160).is_err());
        assert!(pixel_rect(&Selection { width: 0.0, ..s }, 3840, 2160).is_err());
    }
}
