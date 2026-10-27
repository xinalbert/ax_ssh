//! Pure terminal foreground adjustment shared by both frontend adapters.

const MIN_TEXT_BRIGHTNESS: f64 = 0.60;
const MAX_TEXT_BRIGHTNESS: f64 = 1.20;
const DIM_TEXT_BRIGHTNESS_FACTOR: f64 = 0.70;

/// Adjust sRGB HSL lightness after resolving inverse and before applying hidden.
///
/// Brightness is clamped to 60–120%; nonfinite values preserve normal intensity.
/// Faint text combines the chosen brightness with the existing 70% dim factor.
/// Surfaces and alpha are deliberately outside this foreground-only operation.
pub fn adjust_terminal_text_rgb(color: [u8; 3], brightness: f64, dim: bool) -> [u8; 3] {
    let brightness = if brightness.is_finite() {
        brightness.clamp(MIN_TEXT_BRIGHTNESS, MAX_TEXT_BRIGHTNESS)
    } else {
        1.0
    };
    let factor = if dim {
        brightness * DIM_TEXT_BRIGHTNESS_FACTOR
    } else {
        brightness
    };
    if factor == 1.0 {
        return color;
    }

    let red = f64::from(color[0]) / 255.0;
    let green = f64::from(color[1]) / 255.0;
    let blue = f64::from(color[2]) / 255.0;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let lightness = (maximum + minimum) / 2.0;
    let delta = maximum - minimum;
    let (hue, saturation) = if delta == 0.0 {
        (0.0, 0.0)
    } else {
        let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
        let hue = if maximum == red {
            ((green - blue) / delta).rem_euclid(6.0)
        } else if maximum == green {
            (blue - red) / delta + 2.0
        } else {
            (red - green) / delta + 4.0
        } / 6.0;
        (hue, saturation)
    };
    hsl_to_rgb(hue, saturation, (lightness * factor).clamp(0.0, 1.0))
}

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> [u8; 3] {
    let channel = |offset: f64| {
        let value = if saturation == 0.0 {
            lightness
        } else {
            let q = if lightness < 0.5 {
                lightness * (1.0 + saturation)
            } else {
                lightness + saturation - lightness * saturation
            };
            let p = 2.0 * lightness - q;
            hue_channel(p, q, hue + offset)
        };
        (value * 255.0).round().clamp(0.0, 255.0) as u8
    };
    [channel(1.0 / 3.0), channel(0.0), channel(-1.0 / 3.0)]
}

fn hue_channel(p: f64, q: f64, hue: f64) -> f64 {
    let hue = hue.rem_euclid(1.0);
    if hue < 1.0 / 6.0 {
        p + (q - p) * 6.0 * hue
    } else if hue < 0.5 {
        q
    } else if hue < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - hue) * 6.0
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_lightness_preserves_hue_clamps_and_combines_dim() {
        assert_eq!(
            adjust_terminal_text_rgb([100, 100, 100], 0.60, false),
            [60; 3]
        );
        assert_eq!(
            adjust_terminal_text_rgb([100, 100, 100], 1.20, true),
            [84; 3]
        );
        assert_eq!(
            adjust_terminal_text_rgb([255, 0, 0], 0.60, false),
            [153, 0, 0]
        );
        assert_eq!(
            adjust_terminal_text_rgb([255, 0, 0], 1.20, false),
            [255, 51, 51]
        );
        for color in [[0; 3], [255; 3], [23, 127, 201]] {
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                assert_eq!(adjust_terminal_text_rgb(color, invalid, false), color);
            }
            for (input, clamped) in [(-1.0, 0.60), (5.0, 1.20)] {
                assert_eq!(
                    adjust_terminal_text_rgb(color, input, false),
                    adjust_terminal_text_rgb(color, clamped, false)
                );
            }
        }
    }
}
