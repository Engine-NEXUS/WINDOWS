//! Windows.Media.Ocr — zero-RAM local screen text extraction (Feature 86).
//!
//! OS-native OCR: no model download, no GPU, no cloud, ~15-50ms per pass.
//! Used by the screen-analysis fallback when no Gemini key is configured:
//! the user still gets a real "what's on my screen" answer (extracted text
//! in the sidebar + a spoken summary line) with zero API cost, fully
//! offline.
//!
//! Pipeline: GDI capture (existing sidebar_backdrop helper) → PNG bytes →
//! InMemoryRandomAccessStream → BitmapDecoder → SoftwareBitmap →
//! OcrEngine.RecognizeAsync → joined line text + line rects.
//!
//! Fail-open everywhere: no engine / no language pack / decode fail →
//! None (caller falls back to the UIA count).

use windows::Foundation::IAsyncOperation;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapDecoder, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

/// True when an OcrEngine can be created on this machine (language pack
/// present). Cheap; call once per screen-analysis turn.
pub fn is_available() -> bool {
    engine().is_some()
}

fn engine() -> Option<OcrEngine> {
    // Try user-profile languages first, then explicit en-us.
    if let Ok(e) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Some(e);
    }
    let lang = Language::CreateLanguage(&windows::core::HSTRING::from("en-US")).ok()?;
    OcrEngine::TryCreateFromLanguage(&lang).ok()
}

/// Capture the primary monitor and OCR all visible text.
/// Returns (full_text, line_rects) — rects are PHYSICAL px (x, y, w, h).
/// None on any failure (capture, engine, decode).
pub fn capture_screen_text() -> Option<(String, Vec<(i32, i32, i32, i32)>)> {
    let (mw, mh) = crate::screen::primary_monitor_size()?;
    if mw <= 0 || mh <= 0 {
        return None;
    }
    let bgra = crate::sidebar_backdrop::capture_region_bgra_public(0, 0, mw, mh)?;
    // BGRA (BitBlt, bottom-up rows? capture_region_bgra is top-down DIB)
    // → RGBA PNG in memory (image crate, already a dependency).
    let mut rgba: Vec<u8> = Vec::with_capacity(bgra.len());
    for px in bgra.chunks_exact(4) {
        rgba.push(px[2]); // R
        rgba.push(px[1]); // G
        rgba.push(px[0]); // B
        rgba.push(255);
    }
    let img = image::RgbaImage::from_raw(mw as u32, mh as u32, rgba)?;
    let mut png: Vec<u8> = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;

    // PNG bytes → SoftwareBitmap via BitmapDecoder over an in-memory stream.
    let stream = InMemoryRandomAccessStream::new().ok()?;
    let writer = DataWriter::CreateDataWriter(&stream).ok()?;
    writer.WriteBytes(&png).ok()?;
    writer.StoreAsync().ok()?.get().ok()?;
    writer.FlushAsync().ok()?.get().ok()?;
    writer.DetachStream().ok()?;
    stream.Seek(0).ok()?;

    let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.get().ok()?;
    let bitmap: SoftwareBitmap = decoder.GetSoftwareBitmapAsync().ok()?.get().ok()?;

    // OCR.
    let ocr_engine = engine()?;
    let result = ocr_engine.RecognizeAsync(&bitmap).ok()?.get().ok()?;

    let lines = result.Lines().ok()?;
    let line_count = lines.Size().ok()?;
    let mut lines_out: Vec<String> = Vec::new();
    let mut rects: Vec<(i32, i32, i32, i32)> = Vec::new();
    for i in 0..line_count {
        let line = match lines.GetAt(i) {
            Ok(l) => l,
            Err(_) => continue,
        };
        let text = match line.Text() {
            Ok(t) => t.to_string(),
            Err(_) => continue,
        };
        if text.trim().is_empty() {
            continue;
        }
        // Line bounding rect: union of word rects (physical px).
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = 0i32;
        let mut max_y = 0i32;
        if let Ok(words) = line.Words() {
            if let Ok(word_count) = words.Size() {
                for j in 0..word_count {
                    let word = match words.GetAt(j) {
                        Ok(w) => w,
                        Err(_) => continue,
                    };
                    let b = match word.BoundingRect() {
                        Ok(bb) => bb,
                        Err(_) => continue,
                    };
                    let (x, y) = (b.X as i32, b.Y as i32);
                    let (w, h) = (b.Width as i32, b.Height as i32);
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x + w);
                    max_y = max_y.max(y + h);
                }
            }
        }
        if min_x == i32::MAX {
            lines_out.push(text);
            rects.push((0, 0, 0, 0));
            continue;
        }
        lines_out.push(text);
        rects.push((min_x, min_y, max_x - min_x, max_y - min_y));
    }
    if lines_out.is_empty() {
        return None;
    }
    Some((lines_out.join("\n"), rects))
}

/// OCR text only (no rects) — convenience wrapper.
pub fn capture_screen_text_only() -> Option<String> {
    capture_screen_text().map(|(t, _)| t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVE probe (needs a real screen + language pack): run explicitly
    /// with `cargo test --lib ocr -- --ignored`. Ignored by default so
    /// headless CI never runs it.
    #[test]
    #[ignore]
    fn test_ocr_probe_live_screen() {
        let out = capture_screen_text();
        match out {
            Some((text, rects)) => {
                let first: String = text.lines().next().unwrap_or("").to_string();
                println!("OCR OK: {} lines, {} rects; first line: {:?}", text.lines().count(), rects.len(), first);
                assert!(!text.trim().is_empty());
                assert_eq!(rects.len(), text.lines().count());
            }
            None => panic!("OCR returned None — engine/language pack/capture failed on this machine"),
        }
    }
}
