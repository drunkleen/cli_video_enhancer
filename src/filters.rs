pub const BRIGHTNESS_MAX: f64 = 0.25;
pub const CONTRAST_SPAN: f64 = 0.25;
pub const SAT_SPAN: f64 = 0.25;
pub const SHARP_MAX: f64 = 1.0;
pub const DENOISE_LUMA_MAX: f64 = 1.8;
pub const DENOISE_TEMP_MAX: f64 = 9.0;

pub fn validate_scale_height(raw: &str) -> Result<u32, String> {
    let parsed: u32 = raw
        .parse()
        .map_err(|_| format!("`{raw}` must be a positive even integer"))?;
    if parsed == 0 || parsed % 2 != 0 {
        return Err("scale height must be a positive even integer (e.g., 720, 480)".into());
    }
    Ok(parsed)
}

pub fn validate_percent_range(raw: &str) -> Result<u8, String> {
    let parsed: u8 = raw
        .parse()
        .map_err(|_| format!("`{raw}` must be an integer between 0 and 100"))?;
    if parsed > 100 {
        return Err("value must be between 0 and 100".into());
    }
    Ok(parsed)
}

#[inline]
pub fn pct_center_norm(pct: u8) -> f64 {
    (pct as f64 - 50.0) / 50.0
}

pub fn build_video_filters(
    speed: f64,
    denoise: Option<u8>,
    scale_height: Option<u32>,
    sharpen: Option<u8>,
    contrast: Option<u8>,
    saturation: Option<u8>,
    brightness: Option<u8>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(p) = denoise {
        let norm = (pct_center_norm(p)).max(0.0);
        if norm > 0.0 {
            let l = DENOISE_LUMA_MAX * norm;
            let t = DENOISE_TEMP_MAX * norm;
            parts.push(format!("hqdn3d={l:.3}:{l:.3}:{t:.3}:{t:.3}"));
        }
    }

    if let Some(p) = sharpen {
        let amt = pct_center_norm(p) * SHARP_MAX;
        if amt.abs() > 1e-6 {
            parts.push(format!(
                "unsharp=luma_msize_x=7:luma_msize_y=7:luma_amount={amt:.3}"
            ));
        }
    }

    let mut need_eq = false;
    let mut eq_contrast = 1.0;
    let mut eq_saturation = 1.0;
    let mut eq_brightness = 0.0;

    if let Some(p) = contrast {
        let mult = 1.0 + pct_center_norm(p) * CONTRAST_SPAN;
        if (mult - 1.0).abs() > 1e-6 {
            need_eq = true;
            eq_contrast = mult;
        }
    }
    if let Some(p) = saturation {
        let mult = 1.0 + pct_center_norm(p) * SAT_SPAN;
        if (mult - 1.0).abs() > 1e-6 {
            need_eq = true;
            eq_saturation = mult;
        }
    }
    if let Some(p) = brightness {
        let b = pct_center_norm(p) * BRIGHTNESS_MAX;
        if b.abs() > 1e-6 {
            need_eq = true;
            eq_brightness = b;
        }
    }
    if need_eq {
        parts.push(format!(
            "eq=contrast={:.6}:saturation={:.6}:brightness={:.6}",
            eq_contrast, eq_saturation, eq_brightness
        ));
    }

    if let Some(h) = scale_height {
        parts.push(format!("scale=-2:{h}"));
    }

    if (speed - 1.0).abs() > 0.000_5 {
        parts.push(format!("setpts=PTS/{speed}"));
    }

    parts.join(",")
}

pub fn build_audio_filters(speed: f64) -> (Option<String>, Vec<&'static str>) {
    if (speed - 1.0).abs() < 0.001 {
        (None, vec!["-c:a", "copy"])
    } else {
        let mut s = speed;
        let mut chain: Vec<String> = Vec::new();
        if s > 2.0 {
            while s > 2.0 + 1e-6 {
                chain.push("atempo=2.0".into());
                s /= 2.0;
            }
        } else if s < 0.5 {
            while s < 0.5 - 1e-6 {
                chain.push("atempo=0.5".into());
                s /= 0.5;
            }
        }
        if (s - 1.0).abs() > 1e-3 {
            chain.push(format!("atempo={s:.6}"));
        }
        let af = chain.join(",");
        (Some(af), vec!["-c:a", "aac", "-b:a", "192k"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_center_norm() {
        assert!((pct_center_norm(50) - 0.0).abs() < 1e-9);
        assert!((pct_center_norm(100) - 1.0).abs() < 1e-9);
        assert!((pct_center_norm(0) + 1.0).abs() < 1e-9);
        assert!((pct_center_norm(75) - 0.5).abs() < 1e-9);
        assert!((pct_center_norm(25) + 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_build_video_filters_defaults_empty() {
        let f = build_video_filters(1.0, None, None, None, None, None, None);
        assert!(f.is_empty(), "expected empty filters, got: {}", f);
    }

    #[test]
    fn test_build_video_filters_speed_only() {
        let f = build_video_filters(1.25, None, None, None, None, None, None);
        assert_eq!(f, "setpts=PTS/1.25");
    }

    #[test]
    fn test_brightness_mapping() {
        let f = build_video_filters(1.0, None, None, None, None, None, Some(50));
        assert!(f.is_empty(), "brightness 50 should be identity, got: {f}");

        let f = build_video_filters(1.0, None, None, None, None, None, Some(100));
        assert!(f.contains(&format!("brightness={:.6}", BRIGHTNESS_MAX)));

        let f = build_video_filters(1.0, None, None, None, None, None, Some(0));
        assert!(f.contains(&format!("brightness={:.6}", -BRIGHTNESS_MAX)));
    }

    #[test]
    fn test_contrast_saturation_mapping() {
        let c_mult = 1.0 + 0.5 * CONTRAST_SPAN;
        let s_mult = 1.0 + 0.5 * SAT_SPAN;
        let f = build_video_filters(1.0, None, None, None, Some(75), Some(75), None);
        assert!(f.contains(&format!("contrast={:.6}", c_mult)));
        assert!(f.contains(&format!("saturation={:.6}", s_mult)));
    }

    #[test]
    fn test_sharpen_mapping() {
        let amt = 0.5 * SHARP_MAX;
        let f = build_video_filters(1.0, None, None, Some(75), None, None, None);
        assert!(f.contains(&format!("luma_amount={:.3}", amt)));

        let amt_neg = -0.5 * SHARP_MAX;
        let f2 = build_video_filters(1.0, None, None, Some(25), None, None, None);
        assert!(f2.contains(&format!("luma_amount={:.3}", amt_neg)));
    }

    #[test]
    fn test_denoise_mapping() {
        let f = build_video_filters(1.0, Some(50), None, None, None, None, None);
        assert!(f.is_empty() || !f.contains("hqdn3d"));

        let f2 = build_video_filters(1.0, Some(100), None, None, None, None, None);
        assert!(f2.contains(&format!(
            "hqdn3d={:.3}:{:.3}:{:.3}:{:.3}",
            DENOISE_LUMA_MAX, DENOISE_LUMA_MAX, DENOISE_TEMP_MAX, DENOISE_TEMP_MAX
        )));
    }

    #[test]
    fn test_scale_filter_added() {
        let f = build_video_filters(1.0, None, Some(720), None, None, None, None);
        assert!(f.contains("scale=-2:720"));
    }

    #[test]
    fn test_audio_filters() {
        let (af_none, a_copy) = build_audio_filters(1.0);
        assert!(af_none.is_none());
        assert_eq!(a_copy, vec!["-c:a", "copy"]);

        let (af_some, a_enc) = build_audio_filters(1.25);
        assert!(af_some.unwrap().contains("atempo=1.25"));
        assert_eq!(a_enc, vec!["-c:a", "aac", "-b:a", "192k"]);
    }
}
