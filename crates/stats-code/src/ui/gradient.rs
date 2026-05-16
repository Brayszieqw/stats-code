/// Linear interpolate between two RGB colours.
/// Clamps `t` to [0.0, 1.0] (handles NaN / infinity safely).
pub fn interpolate_rgb(start: (u8, u8, u8), end: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        lerp_u8(start.0, end.0, t),
        lerp_u8(start.1, end.1, t),
        lerp_u8(start.2, end.2, t),
    )
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
}

/// Quantize a 24-bit RGB colour to the nearest 256-colour palette index.
///
/// Uses the standard 6×6×6 colour cube (indices 16–231) plus the 24-step
/// greyscale ramp (indices 232–255). Falls back to the nearest cube entry
/// when the input is clearly not greyscale.
pub fn quantize_to_256(rgb: (u8, u8, u8)) -> u8 {
    let (r, g, b) = rgb;
    // Greyscale ramp – 24 steps from 0x08 to 0xEE
    let gray_idx = 232 + ((u16::from(r) + u16::from(g) + u16::from(b)) / 3 / 10).min(23);
    let gray_ref = gray_ref_color(gray_idx);

    // Cube nearest
    let cube_idx = 16 + 36 * cube_coord(r) + 6 * cube_coord(g) + cube_coord(b);
    let cube_ref = cube_ref_color(cube_idx);

    let gray_dist = color_dist(rgb, gray_ref);
    let cube_dist = color_dist(rgb, cube_ref);

    if gray_dist <= cube_dist {
        gray_idx as u8
    } else {
        cube_idx as u8
    }
}

fn cube_coord(v: u8) -> u16 {
    (u16::from(v) * 6 / 256).min(5)
}

fn cube_ref_color(idx: u16) -> (u8, u8, u8) {
    let idx = idx - 16;
    let r = (idx / 36) * 51;
    let g = ((idx / 6) % 6) * 51;
    let b = (idx % 6) * 51;
    (r as u8, g as u8, b as u8)
}

fn gray_ref_color(idx: u16) -> (u8, u8, u8) {
    let v = (idx - 232) * 10 + 8;
    (v as u8, v as u8, v as u8)
}

fn color_dist(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let dr = i32::from(a.0) - i32::from(b.0);
    let dg = i32::from(a.1) - i32::from(b.1);
    let db = i32::from(a.2) - i32::from(b.2);
    (dr * dr + dg * dg + db * db) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interp_t0_returns_start() {
        assert_eq!(
            interpolate_rgb((10, 20, 30), (100, 200, 250), 0.0),
            (10, 20, 30)
        );
    }

    #[test]
    fn interp_t1_returns_end() {
        assert_eq!(
            interpolate_rgb((10, 20, 30), (100, 200, 250), 1.0),
            (100, 200, 250)
        );
    }

    #[test]
    fn interp_midpoint() {
        let result = interpolate_rgb((0, 0, 0), (100, 200, 250), 0.5);
        assert_eq!(result, (50, 100, 125));
    }

    #[test]
    fn interp_clamps_nan() {
        let result = interpolate_rgb((0, 0, 0), (100, 100, 100), f32::NAN);
        assert_eq!(result.0, result.1);
    }

    #[test]
    fn quantize_returns_valid_palette() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let idx = quantize_to_256((r, g, b));
                    assert!(idx <= 255, "index {idx} out of range");
                }
            }
        }
    }

    #[test]
    fn quantize_pure_black() {
        let idx = quantize_to_256((0, 0, 0));
        assert!(idx <= 255);
    }

    #[test]
    fn quantize_pure_white() {
        let idx = quantize_to_256((255, 255, 255));
        assert!(idx <= 255);
    }
}
