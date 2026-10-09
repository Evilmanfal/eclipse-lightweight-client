//! Non-text clipboard contents for the message bar: copied files and images. Text paste is
//! handled by egui itself.
use std::path::PathBuf;
use windows_sys::Win32::{
    System::{DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW}, Memory::{GlobalLock, GlobalSize, GlobalUnlock}},
    UI::{Input::KeyboardAndMouse::GetAsyncKeyState, Shell::DragQueryFileW},
};

const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIB: u32 = 8;
/// Pasted images are re-encoded as PNG; anything larger than this is refused.
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

pub enum Pasted { Files(Vec<PathBuf>), Image(PathBuf) }

/// True while Ctrl+V or Shift+Insert is held. egui turns those keys into a text-only paste
/// event, so the composer watches the keys to notice files and images too.
pub fn paste_keys_down() -> bool {
    let down = |key: i32| unsafe { GetAsyncKeyState(key) } as u16 & 0x8000 != 0;
    (down(0x11) && down(0x56)) || (down(0x10) && down(0x2D))
}

/// Reads copied files or an image. Returns None when the clipboard holds text (egui pastes that)
/// or nothing usable.
pub fn read() -> Result<Option<Pasted>, String> {
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 { return Err("The clipboard is busy; try pasting again.".into()); }
        let result = read_open();
        CloseClipboard();
        result
    }
}

unsafe fn read_open() -> Result<Option<Pasted>, String> {
    if IsClipboardFormatAvailable(CF_HDROP) != 0 {
        let drop = GetClipboardData(CF_HDROP);
        if drop.is_null() { return Ok(None); }
        let count = DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0).min(10);
        let mut files = vec![];
        for index in 0..count {
            let length = DragQueryFileW(drop, index, std::ptr::null_mut(), 0) as usize;
            let mut buffer = vec![0u16; length + 1];
            DragQueryFileW(drop, index, buffer.as_mut_ptr(), buffer.len() as u32);
            let path = PathBuf::from(String::from_utf16_lossy(&buffer[..length]));
            if path.is_file() { files.push(path); }
        }
        return Ok((!files.is_empty()).then_some(Pasted::Files(files)));
    }
    if IsClipboardFormatAvailable(CF_UNICODETEXT) != 0 { return Ok(None); }
    let name: Vec<u16> = "PNG".encode_utf16().chain([0]).collect();
    let png = RegisterClipboardFormatW(name.as_ptr());
    let image = if png != 0 && IsClipboardFormatAvailable(png) != 0 {
        global_bytes(png).and_then(|bytes| image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()).map(|i| i.to_rgba8())
    } else if IsClipboardFormatAvailable(CF_DIB) != 0 {
        global_bytes(CF_DIB).and_then(|bytes| dib_to_rgba(&bytes))
    } else { None };
    let Some(image) = image else { return Ok(None); };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let path = std::env::temp_dir().join(format!("eclipse-paste-{stamp}.png"));
    image.save_with_format(&path, image::ImageFormat::Png).map_err(|_| "Could not prepare the pasted image.".to_owned())?;
    Ok(Some(Pasted::Image(path)))
}

unsafe fn global_bytes(format: u32) -> Option<Vec<u8>> {
    let handle = GetClipboardData(format);
    if handle.is_null() { return None; }
    let size = GlobalSize(handle);
    if size == 0 || size > MAX_IMAGE_BYTES { return None; }
    let data = GlobalLock(handle) as *const u8;
    if data.is_null() { return None; }
    let bytes = std::slice::from_raw_parts(data, size).to_vec();
    GlobalUnlock(handle);
    Some(bytes)
}

/// Decodes an uncompressed 24- or 32-bit device-independent bitmap (what screenshots put on the clipboard).
fn dib_to_rgba(bytes: &[u8]) -> Option<image::RgbaImage> {
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = u32_at(0)? as usize;
    let width = u32_at(4)? as i32;
    let height = u32_at(8)? as i32;
    let bits = u16::from_le_bytes([*bytes.get(14)?, *bytes.get(15)?]);
    let compression = u32_at(16)?;
    if width <= 0 || height == 0 || width > 8192 || height.unsigned_abs() > 8192 || !matches!(bits, 24 | 32) || !matches!(compression, 0 | 3) { return None; }
    let masks = if compression == 3 && header == 40 { 12 } else { 0 };
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let stride = (w * bits as usize / 8 + 3) & !3;
    let pixels = bytes.get(header + masks..)?;
    if pixels.len() < stride * h { return None; }
    let mut out = image::RgbaImage::new(w as u32, h as u32);
    let mut any_alpha = false;
    for y in 0..h {
        let row = &pixels[(if height > 0 { h - 1 - y } else { y }) * stride..];
        for x in 0..w {
            let px = &row[x * bits as usize / 8..];
            let alpha = if bits == 32 { px[3] } else { 255 };
            any_alpha |= alpha != 0;
            out.put_pixel(x as u32, y as u32, image::Rgba([px[2], px[1], px[0], alpha]));
        }
    }
    // Many apps leave the alpha byte zeroed in 32-bit bitmaps; treat that as opaque.
    if !any_alpha { for p in out.pixels_mut() { p.0[3] = 255; } }
    Some(out)
}

#[cfg(test)] mod tests {
    #[test] fn bottom_up_24_bit_bitmaps_decode_with_row_padding() {
        let mut dib = vec![0u8; 40];
        dib[0] = 40; dib[4] = 1; dib[8] = 2; dib[12] = 1; dib[14] = 24;
        // Bottom row first: blue pixel, then the top row: red pixel; each row padded to 4 bytes.
        dib.extend([255, 0, 0, 0, 0, 0, 255, 0]);
        let image = super::dib_to_rgba(&dib).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(0, 1).0, [0, 0, 255, 255]);
    }
}
