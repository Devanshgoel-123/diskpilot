//! The menu bar glyph: DiskPilot's goggles inside a ring that fills with the
//! share of the startup disk in use, so the icon itself shows how full the
//! disk is. Drawn in code as a macOS template image (black with alpha), so
//! the system tints it for light and dark menu bars.

/// 18 pt at 2x.
pub const SIZE: u32 = 36;
/// Supersamples per axis for anti-aliasing.
const SAMPLES: u32 = 4;
/// Opacity of the unfilled part of the ring.
const TRACK: f32 = 0.3;

const CENTER: f32 = SIZE as f32 / 2.0;
const RING_OUTER: f32 = 16.5;
const RING_INNER: f32 = 13.0;
const RING_MID: f32 = (RING_OUTER + RING_INNER) / 2.0;
const CAP: f32 = (RING_OUTER - RING_INNER) / 2.0;
const LENS_Y: f32 = 16.5;
const LENS_DX: f32 = 5.4;
const LENS_OUTER: f32 = 4.4;
const LENS_INNER: f32 = 2.5;

fn within(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// Angle clockwise from 12 o'clock, in turns (0..1).
fn turns(x: f32, y: f32) -> f32 {
    let a = (x - CENTER).atan2(CENTER - y) / std::f32::consts::TAU;
    if a < 0.0 {
        a + 1.0
    } else {
        a
    }
}

/// Point on the ring's centre line at `t` turns.
fn ring_point(t: f32) -> (f32, f32) {
    let a = t * std::f32::consts::TAU;
    (CENTER + RING_MID * a.sin(), CENTER - RING_MID * a.cos())
}

/// Opacity of one sample point.
fn coverage(x: f32, y: f32, used: f32) -> f32 {
    let lens =
        |cx: f32| within(x, y, cx, LENS_Y, LENS_OUTER) && !within(x, y, cx, LENS_Y, LENS_INNER);
    let goggles = lens(CENTER - LENS_DX)
        || lens(CENTER + LENS_DX)
        // The bridge between the lenses.
        || ((x - CENTER).abs() <= 1.2 && (y - (LENS_Y - 1.2)).abs() <= 0.9);
    if goggles {
        return 1.0;
    }

    let d2 = (x - CENTER).powi(2) + (y - CENTER).powi(2);
    let on_ring = (RING_INNER * RING_INNER..=RING_OUTER * RING_OUTER).contains(&d2);
    if used > 0.0 {
        let (sx, sy) = ring_point(0.0);
        let (ex, ey) = ring_point(used);
        let caps = within(x, y, sx, sy, CAP) || within(x, y, ex, ey, CAP);
        if (on_ring && turns(x, y) <= used) || caps {
            return 1.0;
        }
    }
    if on_ring {
        TRACK
    } else {
        0.0
    }
}

/// RGBA pixels for a disk with `used` (0..1) of its space taken.
pub fn render(used: f64) -> Vec<u8> {
    let used = if used.is_finite() {
        used.clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let n = (SAMPLES * SAMPLES) as f32;
    let mut out = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for py in 0..SIZE {
        for px in 0..SIZE {
            let mut sum = 0.0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = px as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    sum += coverage(x, y, used);
                }
            }
            out.extend_from_slice(&[0, 0, 0, (sum / n * 255.0).round() as u8]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(img: &[u8], x: u32, y: u32) -> u8 {
        img[((y * SIZE + x) * 4 + 3) as usize]
    }

    /// A point on the ring's centre line at `t` turns.
    fn ring_px(t: f32) -> (u32, u32) {
        let (x, y) = ring_point(t);
        (x as u32, y as u32)
    }

    #[test]
    fn ring_fills_with_the_share_in_use() {
        let img = render(0.5);
        assert_eq!(img.len(), (SIZE * SIZE * 4) as usize);
        let (x, y) = ring_px(0.25);
        assert_eq!(alpha(&img, x, y), 255, "used part is solid");
        let (x, y) = ring_px(0.75);
        assert_eq!(
            alpha(&img, x, y),
            (TRACK * 255.0).round() as u8,
            "free part is faint"
        );
        let (x, y) = ring_px(0.75);
        assert_eq!(alpha(&render(1.0), x, y), 255);
    }

    #[test]
    fn corners_are_clear_and_bad_input_draws_an_empty_ring() {
        let img = render(0.4);
        assert_eq!(alpha(&img, 0, 0), 0);
        assert_eq!(alpha(&img, SIZE - 1, SIZE - 1), 0);
        assert_eq!(render(f64::NAN), render(0.0));
        assert_eq!(render(7.0), render(1.0));
    }

    /// Writes previews for eyeballing: `cargo test tray_icon -- --ignored`.
    #[test]
    #[ignore]
    fn write_previews() {
        let dir = std::env::temp_dir();
        for used in [0.0, 0.35, 0.72, 0.93] {
            let path = dir.join(format!("tray-{}.rgba", (used * 100.0) as u32));
            std::fs::write(&path, render(used)).unwrap();
            println!("{}", path.display());
        }
    }
}
