use anyhow::{bail, Context, Result};
use clap::{ArgAction, Parser};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use regex::Regex;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use which::which;

/// Enhance video (optional), change speed, and show a modern progress UI.
/// By default (no enhancement flags and --speed 1.0), it stream-copies both audio and video (no re-encode).
#[derive(Parser, Debug)]
#[command(
    name = "video_enhancer",
    version,
    about = "Enhance video (optional), change speed, and show a modern progress UI"
)]
struct Cli {
    /// Input video file
    #[arg(short = 'i', long)]
    input: PathBuf,

    /// Output file (default: <input>_enhanced_speed<S>.mp4)
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,

    /// Playback speed factor (1.0 means unchanged)
    #[arg(short = 's', long, default_value = "1.0")]
    speed: f64,

    /// x264 CRF (used only if we re-encode video)
    #[arg(long, default_value = "17")]
    crf: u8,

    /// x264 preset (used only if we re-encode video)
    #[arg(long, default_value = "slow")]
    preset: String,

    /// Denoise 0..100 (50 = unchanged; <=50 off; >50 more denoise)
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    denoise: Option<u8>,

    /// Sharpen 0..100 (50 = unchanged; <50 blur; >50 sharpen)
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    sharpen: Option<u8>,

    /// Contrast 0..100 (50 = unchanged)
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    contrast: Option<u8>,

    /// Saturation 0..100 (50 = unchanged)
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    saturation: Option<u8>,

    /// Brightness 0..100 (50 = unchanged; 0 darkest; 100 lightest)
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    brightness: Option<u8>,

    /// Show raw ffmpeg logs (useful for debugging)
    #[arg(long, action=ArgAction::SetTrue)]
    verbose: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    ensure_in_path("ffmpeg")?;
    ensure_in_path("ffprobe")?;
    if !cli.input.exists() {
        bail!("Input not found: {}", cli.input.display());
    }
    if cli.speed <= 0.0 {
        bail!("Speed must be > 0.0");
    }

    let out = cli
        .output
        .clone()
        .unwrap_or_else(|| default_output(&cli.input, cli.speed));

    let dur_s = probe_duration_seconds(&cli.input)?;
    let target_s = if (cli.speed - 1.0).abs() < 0.000_5 {
        dur_s
    } else {
        dur_s / cli.speed
    };

    // Build filters from flags (empty => stream copy)
    let v_filters = build_video_filters(
        cli.speed,
        cli.denoise,
        cli.sharpen,
        cli.contrast,
        cli.saturation,
        cli.brightness,
    );
    let touches_video = !v_filters.is_empty();

    let (a_filters_opt, a_codec_when_touch) = build_audio_filters(cli.speed);

    // ---------- UI ----------
    let mp = MultiProgress::new();

    let spinner = mp.add(ProgressBar::new_spinner());
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    spinner.set_style(
        ProgressStyle::with_template("{spinner} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    spinner.set_message("🚀 Preparing…");

    let total_ms = (target_s * 1000.0).max(1.0) as u64;
    let bar = mp.add(ProgressBar::new(total_ms));
    bar.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}]  [{bar:60.cyan/bright-black}] {percent:>3}%  {pos}/{len}ms  ETA:{eta_precise}\n{wide_msg}"
        )
        .unwrap()
        .progress_chars("#▱-"),
    );
    bar.set_message("🎞️ Building filter graph…");

    if a_filters_opt.is_some() {
        bar.set_message("🔊 Audio will be time-stretched (atempo)...");
    }

    // ---------- ffmpeg ----------
    let mut cmd = Command::new("ffmpeg");
    if !cli.verbose {
        cmd.arg("-hide_banner")
            .arg("-nostats")
            .arg("-loglevel")
            .arg("error");
    }
    cmd.arg("-y")
        .arg("-progress")
        .arg("-")
        .arg("-i")
        .arg(&cli.input);

    if touches_video {
        cmd.arg("-vf").arg(v_filters);
        cmd.args(["-c:v", "libx264"]);
        cmd.args(["-crf", &cli.crf.to_string()]);
        cmd.args(["-preset", &cli.preset]);
        cmd.args(["-pix_fmt", "yuv420p"]);
    } else {
        cmd.args(["-c:v", "copy"]);
    }

    if let Some(af) = a_filters_opt.clone() {
        cmd.arg("-af").arg(af);
        cmd.args(&a_codec_when_touch);
    } else {
        cmd.args(["-c:a", "copy"]);
    }

    cmd.arg(out.as_os_str());

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(if cli.verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .spawn()
        .context("failed to spawn ffmpeg")?;

    let stdout = child.stdout.take().unwrap();
    let re_kv = Regex::new(r"^(\w+)=([\w\-\.:]+)$").unwrap();

    let spinner_clone = spinner.clone();
    let bar_clone = bar.clone();

    let reader_thread = thread::spawn(move || -> Result<()> {
        spinner_clone.set_message("🔎 Probing input…");
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line?;
            if let Some(caps) = re_kv.captures(&line) {
                let key = &caps[1];
                let val = &caps[2];
                match key {
                    "out_time_ms" => {
                        let ms: u64 = val.parse().unwrap_or(0);
                        let pos_ms = (ms / 1000).min(total_ms);
                        bar_clone.set_position(pos_ms);

                        let pct = (pos_ms as f64) / (total_ms as f64);
                        if pct < 0.10 {
                            spinner_clone.set_message("🧰 Preparing filters…");
                            bar_clone.set_message("🎛️ Applying selected filters (if any)...");
                        } else if pct < 0.65 {
                            spinner_clone.set_message("🎬 Encoding video…");
                            bar_clone.set_message("🖼️ Processing frames...");
                        } else if pct < 0.95 {
                            spinner_clone.set_message("🎧 Adjusting/encoding audio…");
                            bar_clone.set_message("🔊 Applying atempo (if speed ≠ 1.0)...");
                        } else {
                            spinner_clone.set_message("📦 Finalizing and muxing…");
                            bar_clone.set_message("🧩 Muxing, writing headers, closing output...");
                        }
                    }
                    "progress" if val == "end" => {
                        bar_clone.finish_with_message("✅ Done");
                        spinner_clone.finish_with_message("✨ Completed");
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    });

    let status = child.wait()?;
    let _ = reader_thread.join();
    if !status.success() {
        bail!("ffmpeg failed with status: {}", status);
    }
    Ok(())
}

// ---------- helpers & mapping (pub(crate) for tests) ----------

fn ensure_in_path(bin: &str) -> Result<()> {
    which(bin).with_context(|| format!("`{bin}` not found in PATH"))?;
    Ok(())
}

fn default_output(input: &Path, speed: f64) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("output");
    let parent = input.parent().unwrap_or(Path::new("."));
    parent.join(format!("{stem}_enhanced_speed{speed}.mp4"))
}

fn probe_duration_seconds(input: &Path) -> Result<f64> {
    let out = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(input)
        .output()
        .context("failed to run ffprobe")?;
    if !out.status.success() {
        bail!("ffprobe error (status {})", out.status);
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(s.parse::<f64>().context("cannot parse duration")?)
}

pub(crate) const BRIGHTNESS_MAX: f64 = 0.25;
pub(crate) const CONTRAST_SPAN: f64 = 0.25;
pub(crate) const SAT_SPAN: f64 = 0.25;
pub(crate) const SHARP_MAX: f64 = 1.0;
pub(crate) const DENOISE_LUMA_MAX: f64 = 1.8;
pub(crate) const DENOISE_TEMP_MAX: f64 = 9.0;

#[inline]
pub(crate) fn pct_center_norm(pct: u8) -> f64 {
    (pct as f64 - 50.0) / 50.0
}

pub(crate) fn build_video_filters(
    speed: f64,
    denoise: Option<u8>,
    sharpen: Option<u8>,
    contrast: Option<u8>,
    saturation: Option<u8>,
    brightness: Option<u8>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // denoise: only >50 maps to hqdn3d
    if let Some(p) = denoise {
        let norm = (pct_center_norm(p)).max(0.0); // 0..1
        if norm > 0.0 {
            let l = DENOISE_LUMA_MAX * norm;
            let t = DENOISE_TEMP_MAX * norm;
            parts.push(format!("hqdn3d={l:.3}:{l:.3}:{t:.3}:{t:.3}"));
        }
    }

    // sharpen: <50 blur (negative), >50 sharpen (positive)
    if let Some(p) = sharpen {
        let amt = pct_center_norm(p) * SHARP_MAX; // [-1..+1]
        if amt.abs() > 1e-6 {
            parts.push(format!(
                "unsharp=luma_msize_x=7:luma_msize_y=7:luma_amount={amt:.3}"
            ));
        }
    }

    // eq (contrast/saturation/brightness)
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

    if (speed - 1.0).abs() > 0.000_5 {
        parts.push(format!("setpts=PTS/{speed}"));
    }

    parts.join(",")
}

pub(crate) fn build_audio_filters(speed: f64) -> (Option<String>, Vec<&'static str>) {
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
        let f = build_video_filters(1.0, None, None, None, None, None);
        assert!(f.is_empty(), "expected empty filters, got: {}", f);
    }

    #[test]
    fn test_build_video_filters_speed_only() {
        let f = build_video_filters(1.25, None, None, None, None, None);
        assert_eq!(f, "setpts=PTS/1.25");
    }

    #[test]
    fn test_brightness_mapping() {
        let f = build_video_filters(1.0, None, None, None, None, Some(50));
        assert!(f.is_empty(), "brightness 50 should be identity, got: {f}");

        let f = build_video_filters(1.0, None, None, None, None, Some(100));
        assert!(f.contains(&format!("brightness={:.6}", BRIGHTNESS_MAX)));

        let f = build_video_filters(1.0, None, None, None, None, Some(0));
        assert!(f.contains(&format!("brightness={:.6}", -BRIGHTNESS_MAX)));
    }

    #[test]
    fn test_contrast_saturation_mapping() {
        let c_mult = 1.0 + 0.5 * CONTRAST_SPAN;
        let s_mult = 1.0 + 0.5 * SAT_SPAN;
        let f = build_video_filters(1.0, None, None, Some(75), Some(75), None);
        assert!(f.contains(&format!("contrast={:.6}", c_mult)));
        assert!(f.contains(&format!("saturation={:.6}", s_mult)));
    }

    #[test]
    fn test_sharpen_mapping() {
        let amt = 0.5 * SHARP_MAX;
        let f = build_video_filters(1.0, None, Some(75), None, None, None);
        assert!(f.contains(&format!("luma_amount={:.3}", amt)));

        let amt_neg = -0.5 * SHARP_MAX;
        let f2 = build_video_filters(1.0, None, Some(25), None, None, None);
        assert!(f2.contains(&format!("luma_amount={:.3}", amt_neg)));
    }

    #[test]
    fn test_denoise_mapping() {
        let f = build_video_filters(1.0, Some(50), None, None, None, None);
        assert!(f.is_empty() || !f.contains("hqdn3d"));

        let f2 = build_video_filters(1.0, Some(100), None, None, None, None);
        assert!(f2.contains(&format!(
            "hqdn3d={:.3}:{:.3}:{:.3}:{:.3}",
            DENOISE_LUMA_MAX, DENOISE_LUMA_MAX, DENOISE_TEMP_MAX, DENOISE_TEMP_MAX
        )));
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
