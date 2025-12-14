use crate::filters::{validate_percent_range, validate_scale_height};
use anyhow::{bail, Result};
use clap::{ArgAction, Parser, ValueHint};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "video_enhancer",
    version,
    about = "Enhance video (optional), change speed, and show a modern progress UI"
)]
pub struct Cli {
    /// Input video file
    #[arg(short = 'i', long, value_hint = ValueHint::FilePath)]
    pub input: PathBuf,

    /// Output file (default: <input>_enhanced_speed<S>.mp4)
    #[arg(short = 'o', long, value_hint = ValueHint::FilePath)]
    pub output: Option<PathBuf>,

    /// Playback speed factor (1.0 means unchanged)
    #[arg(short = 's', long, default_value = "1.0")]
    pub speed: f64,

    /// x264 CRF (used only if we re-encode video)
    #[arg(long, default_value = "17")]
    pub crf: u8,

    /// x264 preset (used only if we re-encode video)
    #[arg(long, default_value = "slow")]
    pub preset: String,

    /// Denoise 0..100 (50 = unchanged; <=50 off; >50 more denoise)
    #[arg(long, value_parser = validate_percent_range)]
    pub denoise: Option<u8>,

    /// Output height (e.g., 720, 480). Width auto-calculated to keep aspect. Must be even.
    #[arg(long, value_parser = validate_scale_height)]
    pub scale: Option<u32>,

    /// Sharpen 0..100 (50 = unchanged; <50 blur; >50 sharpen)
    #[arg(long, value_parser = validate_percent_range)]
    pub sharpen: Option<u8>,

    /// Contrast 0..100 (50 = unchanged)
    #[arg(long, value_parser = validate_percent_range)]
    pub contrast: Option<u8>,

    /// Saturation 0..100 (50 = unchanged)
    #[arg(long, value_parser = validate_percent_range)]
    pub saturation: Option<u8>,

    /// Brightness 0..100 (50 = unchanged; 0 darkest; 100 lightest)
    #[arg(long, value_parser = validate_percent_range)]
    pub brightness: Option<u8>,

    /// Show raw ffmpeg logs (useful for debugging)
    #[arg(long, action = ArgAction::SetTrue)]
    pub verbose: bool,

    /// Threads to allow ffmpeg (0 = auto/max)
    #[arg(long, default_value = "0")]
    pub threads: u16,

    /// Path to ffmpeg binary (overrides PATH lookup)
    #[arg(long, value_hint = ValueHint::ExecutablePath)]
    pub ffmpeg: Option<PathBuf>,

    /// Path to ffprobe binary (overrides PATH lookup)
    #[arg(long, value_hint = ValueHint::ExecutablePath)]
    pub ffprobe: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub speed: f64,
    pub crf: u8,
    pub preset: String,
    pub denoise: Option<u8>,
    pub scale: Option<u32>,
    pub sharpen: Option<u8>,
    pub contrast: Option<u8>,
    pub saturation: Option<u8>,
    pub brightness: Option<u8>,
    pub verbose: bool,
    pub threads: u16,
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
}

impl Cli {
    pub fn into_config(self) -> Result<AppConfig> {
        if self.speed <= 0.0 {
            bail!("Speed must be > 0.0");
        }
        if !self.input.exists() {
            bail!("Input not found: {}", self.input.display());
        }
        let output = self
            .output
            .clone()
            .unwrap_or_else(|| default_output(&self.input, self.speed));

        Ok(AppConfig {
            input: self.input,
            output,
            speed: self.speed,
            crf: self.crf,
            preset: self.preset,
            denoise: self.denoise,
            scale: self.scale,
            sharpen: self.sharpen,
            contrast: self.contrast,
            saturation: self.saturation,
            brightness: self.brightness,
            verbose: self.verbose,
            threads: self.threads,
            ffmpeg: self.ffmpeg,
            ffprobe: self.ffprobe,
        })
    }
}

pub fn default_output(input: &Path, speed: f64) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("output");
    let parent = input.parent().unwrap_or(Path::new("."));
    parent.join(format!("{stem}_enhanced_speed{speed}.mp4"))
}

pub fn target_duration_ms(original_seconds: f64, speed: f64) -> u64 {
    let target_seconds = if (speed - 1.0).abs() < 0.000_5 {
        original_seconds
    } else {
        original_seconds / speed
    };
    (target_seconds * 1000.0).max(1.0) as u64
}
