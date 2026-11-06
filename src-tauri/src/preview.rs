//! Previews of files the user is reviewing, made by macOS Quick Look.
//!
//! Read-only: generating a thumbnail never changes the file. Callers must
//! check the path is one DiskPilot listed (see `commands::file_thumbnail`).

use std::path::Path;

/// Largest thumbnail edge, in points, the UI may ask for.
pub const MAX_THUMBNAIL_PX: f64 = 512.0;

/// A PNG thumbnail of the file, `px` points square at 2x scale, or the file's
/// icon when macOS has no thumbnail for its type. `None` if neither exists.
#[cfg(target_os = "macos")]
pub fn thumbnail_png(path: &Path, px: f64) -> Option<Vec<u8>> {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    use objc2_foundation::{NSDictionary, NSError, NSSize, NSString, NSURL};
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
                    // SAFETY: an empty properties dictionary is valid for PNG output.
                    unsafe {
                        bitmap.representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &NSDictionary::new(),
                        )
                    }
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

#[cfg(not(target_os = "macos"))]
pub fn thumbnail_png(_path: &Path, _px: f64) -> Option<Vec<u8>> {
    None
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn makes_a_png_for_a_text_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "Hello from DiskPilot\n".repeat(50)).unwrap();
        let png = thumbnail_png(&file, 128.0).expect("a thumbnail or icon");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
