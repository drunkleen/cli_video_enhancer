use crate::cli::{default_output, AppConfig};
use crate::filters::validate_percent_range;
use anyhow::{bail, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Input};
use std::path::PathBuf;

pub fn interactive_config() -> Result<AppConfig> {
    println!("Interactive Video Enhancer");
    println!("Press Enter to accept defaults or leave options unset.\n");

    let theme = ColorfulTheme::default();
    let input = loop {
        let raw: String = Input::with_theme(&theme)
            .with_prompt("Input video file path")
            .interact_text()?;
        let path = PathBuf::from(raw.trim());
        if path.exists() {
            break path;
        } else {
            println!("Path not found, please try again.");
        }
    };

    let speed: f64 = Input::with_theme(&theme)
        .with_prompt("Playback speed (1.0 = unchanged)")
        .default(1.0)
        .interact_text()?;
    if speed <= 0.0 {
        bail!("Speed must be > 0.0");
    }

    let default_out = default_output(&input, speed);
    let out_prompt = format!(
        "Output file path [{}]",
        default_out.as_os_str().to_string_lossy()
    );
    let raw_out: String = Input::with_theme(&theme)
        .with_prompt(out_prompt)
        .allow_empty(true)
        .interact_text()?;
    let output = if raw_out.trim().is_empty() {
        default_out
    } else {
        PathBuf::from(raw_out.trim())
    };

    let denoise = prompt_optional_pct(&theme, "Denoise (0-100, blank=skip)")?;
    let scale_height = prompt_optional_scale(&theme)?;
    let sharpen = prompt_optional_pct(&theme, "Sharpen (0-100, blank=skip)")?;
    let contrast = prompt_optional_pct(&theme, "Contrast (0-100, blank=skip)")?;
    let saturation = prompt_optional_pct(&theme, "Saturation (0-100, blank=skip)")?;
    let brightness = prompt_optional_pct(&theme, "Brightness (0-100, blank=skip)")?;

    let crf: u8 = Input::with_theme(&theme)
        .with_prompt("CRF (17 default, used if re-encoding)")
        .default(17)
        .interact_text()?;
    let preset: String = Input::with_theme(&theme)
        .with_prompt("x264 preset (slow default)")
        .default("slow".into())
        .interact_text()?;

    let threads: u16 = Input::with_theme(&theme)
        .with_prompt("Threads (0 = ffmpeg auto)")
        .default(0)
        .interact_text()?;

    let verbose = Confirm::with_theme(&theme)
        .with_prompt("Show ffmpeg logs?")
        .default(false)
        .interact()?;

    let ffmpeg_path = prompt_optional_path(&theme, "Custom ffmpeg path (blank = PATH)")?;
    let ffprobe_path = prompt_optional_path(&theme, "Custom ffprobe path (blank = PATH)")?;

    Ok(AppConfig {
        input,
        output,
        speed,
        crf,
        preset,
        denoise,
        scale: scale_height,
        sharpen,
        contrast,
        saturation,
        brightness,
        verbose,
        threads,
        ffmpeg: ffmpeg_path,
        ffprobe: ffprobe_path,
    })
}

fn prompt_optional_pct(theme: &ColorfulTheme, prompt: &str) -> Result<Option<u8>> {
    loop {
        let raw: String = Input::with_theme(theme)
            .with_prompt(prompt)
            .allow_empty(true)
            .interact_text()?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        match validate_percent_range(trimmed) {
            Ok(val) => return Ok(Some(val)),
            Err(err) => {
                println!("Invalid value: {err}. Please enter 0-100 or leave blank.");
            }
        }
    }
}

fn prompt_optional_path(theme: &ColorfulTheme, prompt: &str) -> Result<Option<PathBuf>> {
    loop {
        let raw: String = Input::with_theme(theme)
            .with_prompt(prompt)
            .allow_empty(true)
            .interact_text()?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let path = PathBuf::from(trimmed);
        if path.exists() {
            return Ok(Some(path));
        } else {
            println!("Path not found. Leave blank to skip or enter a valid file path.");
        }
    }
}

fn prompt_optional_scale(theme: &ColorfulTheme) -> Result<Option<u32>> {
    loop {
        let raw: String = Input::with_theme(theme)
            .with_prompt("Output height (e.g., 720 or 480; blank=keep source)")
            .allow_empty(true)
            .interact_text()?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        match crate::filters::validate_scale_height(trimmed) {
            Ok(h) => return Ok(Some(h)),
            Err(err) => println!("{err}. Please enter an even integer or leave blank."),
        }
    }
}
