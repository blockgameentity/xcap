use image::RgbaImage;
use screencapturekit::{
    screenshot_manager::SCScreenshotManager,
    shareable_content::{SCDisplay, SCShareableContent, SCWindow},
    stream::{
        configuration::{PixelFormat, SCStreamConfiguration},
        content_filter::SCContentFilter,
    },
};

use crate::error::{XCapError, XCapResult};

fn sc_error(e: impl std::fmt::Display) -> XCapError {
    XCapError::ScreenCaptureKit(e.to_string())
}

fn find_sc_display(display_id: u32) -> XCapResult<SCDisplay> {
    let content = SCShareableContent::get().map_err(sc_error)?;
    content
        .displays()
        .into_iter()
        .find(|d| d.display_id() == display_id)
        .ok_or_else(|| XCapError::new(format!("Display {} not found in SCShareableContent", display_id)))
}

fn find_sc_window(window_id: u32) -> XCapResult<SCWindow> {
    let content = SCShareableContent::get().map_err(sc_error)?;
    content
        .windows()
        .into_iter()
        .find(|w| w.window_id() == window_id)
        .ok_or_else(|| XCapError::new(format!("Window {} not found in SCShareableContent", window_id)))
}

fn cg_image_to_rgba(cg_image: &screencapturekit::screenshot_manager::CGImage) -> XCapResult<RgbaImage> {
    let width = cg_image.width() as u32;
    let height = cg_image.height() as u32;
    let rgba = cg_image.rgba_data().map_err(sc_error)?;

    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| XCapError::new("RgbaImage::from_raw failed"))
}

pub fn capture_display(display_id: u32, width: u32, height: u32) -> XCapResult<RgbaImage> {
    let display = find_sc_display(display_id)?;

    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build();

    let config = SCStreamConfiguration::new()
        .with_width(width)
        .with_height(height)
        .with_pixel_format(PixelFormat::BGRA)
        .with_shows_cursor(true);

    let cg_image = SCScreenshotManager::capture_image(&filter, &config).map_err(sc_error)?;
    cg_image_to_rgba(&cg_image)
}

pub fn capture_display_region(
    display_id: u32,
    region_x: u32,
    region_y: u32,
    region_width: u32,
    region_height: u32,
    scale_factor: f32,
) -> XCapResult<RgbaImage> {
    let display = find_sc_display(display_id)?;

    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build();

    // Capture at full display resolution, then crop
    let full_width = (display.width() as f32 * scale_factor) as u32;
    let full_height = (display.height() as f32 * scale_factor) as u32;

    let config = SCStreamConfiguration::new()
        .with_width(full_width)
        .with_height(full_height)
        .with_pixel_format(PixelFormat::BGRA)
        .with_shows_cursor(true);

    let cg_image = SCScreenshotManager::capture_image(&filter, &config).map_err(sc_error)?;
    let full_image = cg_image_to_rgba(&cg_image)?;

    // Crop to region (coordinates are in logical pixels, scale to physical)
    let px = (region_x as f32 * scale_factor) as u32;
    let py = (region_y as f32 * scale_factor) as u32;
    let pw = (region_width as f32 * scale_factor) as u32;
    let ph = (region_height as f32 * scale_factor) as u32;

    let cropped = image::imageops::crop_imm(&full_image, px, py, pw, ph).to_image();
    Ok(cropped)
}

pub fn capture_window(window_id: u32) -> XCapResult<RgbaImage> {
    let window = find_sc_window(window_id)?;

    let filter = SCContentFilter::create()
        .with_window(&window)
        .build();

    let frame = window.frame();
    let width = frame.width.max(1.0) as u32;
    let height = frame.height.max(1.0) as u32;

    let config = SCStreamConfiguration::new()
        .with_width(width)
        .with_height(height)
        .with_pixel_format(PixelFormat::BGRA)
        .with_shows_cursor(false);

    let cg_image = SCScreenshotManager::capture_image(&filter, &config).map_err(sc_error)?;
    cg_image_to_rgba(&cg_image)
}
