//! Previews of files the user is reviewing, made by macOS Quick Look or the
//! Windows shell's thumbnail handlers.
//!
//! Read-only: generating a thumbnail never changes the file. Callers must
//! check the path is one DiskPilot listed (see `commands::file_thumbnail`).
//! On Windows a cloud file whose data is not on this PC gets no preview:
//! drawing one would download it.

use std::path::Path;

/// Largest thumbnail edge, in points, the UI may ask for.
pub const MAX_THUMBNAIL_PX: f64 = 512.0;

/// JPEG quality for opaque thumbnails; artefacts are not visible at preview size.
#[cfg(target_os = "macos")]
const JPEG_QUALITY: f64 = 0.82;

/// A thumbnail of the file (JPEG when opaque, otherwise PNG), `px` points square at 2x scale, or the file's
/// icon when macOS has no thumbnail for its type. `None` if neither exists.
#[cfg(target_os = "macos")]
pub fn thumbnail_png(path: &Path, px: f64) -> Option<Vec<u8>> {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSBitmapFormat, NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey,
        NSImageCompressionFactor,
    };

    /// Whether every pixel is fully opaque. Quick Look images always carry an
    /// alpha channel, so the alpha samples themselves are checked (about a
    /// millisecond for a 1024 px image). Photos often come as 16-bit floating
    /// point. Unknown layouts count as transparent.
    fn is_opaque(rep: &NSBitmapImageRep) -> bool {
        if !rep.hasAlpha() {
            return true;
        }
        let fmt = rep.bitmapFormat();
        let float = fmt.contains(NSBitmapFormat::FloatingPointSamples);
        let sample = rep.bitsPerSample() as usize / 8;
        // The bytes of a fully opaque alpha sample, little-endian.
        let one: &[u8] = match (sample, float) {
            (1, false) => &[0xFF],
            (2, false) => &[0xFF, 0xFF],
            (2, true) => &[0x00, 0x3C],             // 1.0 as a half float
            (4, true) => &[0x00, 0x00, 0x80, 0x3F], // 1.0 as a float
            _ => return false,
        };
        if rep.isPlanar() || rep.samplesPerPixel() != 4 {
            return false;
        }
        let (row, high) = (rep.bytesPerRow(), rep.pixelsHigh());
        let data = rep.bitmapData();
        if data.is_null() || row <= 0 || high <= 0 {
            return false;
        }
        let pixel = sample * 4;
        let at = if fmt.contains(NSBitmapFormat::AlphaFirst) {
            0
        } else {
            3 * sample
        };
        let wide = (rep.pixelsWide().max(0) as usize).min(row as usize / pixel);
        // SAFETY: the buffer holds `bytesPerRow * pixelsHigh` bytes and stays
        // alive while `rep` does; it is only read here.
        let bytes = unsafe { std::slice::from_raw_parts(data, (row * high) as usize) };
        bytes.chunks_exact(row as usize).all(|line| {
            line[..wide * pixel]
                .chunks_exact(pixel)
                .all(|px| &px[at..at + sample] == one)
        })
    }
    use objc2_foundation::{NSDictionary, NSError, NSNumber, NSSize, NSString, NSURL};
    use objc2_quick_look_thumbnailing::{
        QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes as Types,
        QLThumbnailGenerator, QLThumbnailRepresentation,
    };

    let px = px.clamp(16.0, MAX_THUMBNAIL_PX);
    let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
    // SAFETY: a valid file URL, a positive size and scale, and known flags.
    let request = unsafe {
        QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
            QLThumbnailGenerationRequest::alloc(),
            &url,
            NSSize::new(px, px),
            2.0,
            Types::Icon | Types::Thumbnail,
        )
    };

    let (tx, rx) = mpsc::sync_channel::<Option<Vec<u8>>>(1);
    let handler = RcBlock::new(
        move |rep: *mut QLThumbnailRepresentation, _err: *mut NSError| {
            // SAFETY: Quick Look passes either null or a valid representation
            // that stays alive for the duration of this call.
            let png = unsafe { rep.as_ref() }.and_then(|rep| {
                objc2::rc::autoreleasepool(|_| {
                    // SAFETY: a finished representation always has a CGImage.
                    let cg = unsafe { rep.CGImage() };
                    let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
                    // Opaque images (photos, video frames, documents) as JPEG:
                    // several times faster to encode and about a tenth of the
                    // bytes to send to the window. Icons and images with
                    // transparency stay PNG.
                    let (kind, props) = if !is_opaque(&bitmap) {
                        (NSBitmapImageFileType::PNG, NSDictionary::new())
                    } else {
                        // SAFETY: NSImageCompressionFactor is an AppKit constant.
                        let key: &NSBitmapImageRepPropertyKey = unsafe { NSImageCompressionFactor };
                        let quality = NSNumber::new_f64(JPEG_QUALITY);
                        let value: &AnyObject = &quality;
                        (
                            NSBitmapImageFileType::JPEG,
                            NSDictionary::from_slices(&[key], &[value]),
                        )
                    };
                    // SAFETY: the properties are valid for the chosen type.
                    unsafe { bitmap.representationUsingType_properties(kind, &props) }
                        .map(|data| data.to_vec())
                })
            });
            let _ = tx.try_send(png);
        },
    );
    // SAFETY: the request and handler are valid; the handler is called once.
    unsafe {
        QLThumbnailGenerator::sharedGenerator()
            .generateBestRepresentationForRequest_completionHandler(&request, &handler);
    }
    rx.recv_timeout(Duration::from_secs(10)).ok().flatten()
}

/// A PNG thumbnail of the file, fitted in `px` points at 2x scale, or the
/// file's icon when the shell has no thumbnail for its type. `None` for a
/// cloud file that is not on this PC.
#[cfg(windows)]
pub fn thumbnail_png(path: &Path, px: f64) -> Option<Vec<u8>> {
    let meta = path.symlink_metadata().ok()?;
    if !crate::platform::is_local(&meta) {
        return None;
    }
    let px = (px.clamp(16.0, MAX_THUMBNAIL_PX) * 2.0).round() as u32;
    crate::winshell::shell_image_png(path.as_os_str(), px, crate::winshell::SIIGBF_RESIZETOFIT)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn thumbnail_png(_path: &Path, _px: f64) -> Option<Vec<u8>> {
    None
}

#[cfg(all(test, any(target_os = "macos", windows)))]
mod tests {
    use super::*;

    #[test]
    fn makes_a_png_for_a_text_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "Hello from DiskPilot\n".repeat(50)).unwrap();
        let img = thumbnail_png(&file, 128.0).expect("a thumbnail or icon");
        let png = img.starts_with(b"\x89PNG\r\n\x1a\n");
        let jpeg = img.starts_with(&[0xFF, 0xD8, 0xFF]);
        assert!(png || jpeg, "neither PNG nor JPEG");
    }

    #[test]
    fn opaque_images_are_jpeg_and_transparent_ones_png() {
        let dir = tempfile::tempdir().unwrap();
        // A tiny opaque BMP and a PNG with a transparent pixel.
        let opaque = dir.path().join("opaque.bmp");
        let mut bmp = vec![b'B', b'M'];
        let px: Vec<u8> = [0x20u8, 0x40, 0x80].repeat(16 * 16);
        let size = 54 + px.len() as u32;
        bmp.extend_from_slice(&size.to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
        bmp.extend_from_slice(&16i32.to_le_bytes());
        bmp.extend_from_slice(&16i32.to_le_bytes());
        bmp.extend_from_slice(&[1, 0, 24, 0, 0, 0, 0, 0]);
        bmp.extend_from_slice(&(px.len() as u32).to_le_bytes());
        bmp.extend_from_slice(&[0; 16]);
        bmp.extend_from_slice(&px);
        std::fs::write(&opaque, bmp).unwrap();
        let img = thumbnail_png(&opaque, 64.0).expect("a thumbnail");
        assert!(
            img.starts_with(&[0xFF, 0xD8, 0xFF]),
            "opaque image should be JPEG"
        );
    }
}
