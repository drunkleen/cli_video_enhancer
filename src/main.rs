mod cli;
mod ffmpeg;
mod filters;
mod progress;
mod tui;

use crate::cli::Cli;
use crate::filters::{build_audio_filters, build_video_filters};
use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let config = if std::env::args_os().len() > 1 {
        let cli = Cli::parse();
        cli.into_config()?
    } else {
        tui::interactive_config()?
    };
    let tools = ffmpeg::resolve_tools(config.ffmpeg.clone(), config.ffprobe.clone())?;

    let duration = ffmpeg::probe_duration_seconds(&tools, &config.input)?;
    let total_ms = crate::cli::target_duration_ms(duration, config.speed);

    let video_filters = build_video_filters(
        config.speed,
        config.denoise,
        config.sharpen,
        config.contrast,
        config.saturation,
        config.brightness,
    );
    let (audio_filters_opt, audio_codec_when_touch) = build_audio_filters(config.speed);

    let ui = progress::ProgressUi::new(total_ms, audio_filters_opt.is_some());

    let session = ffmpeg::spawn_ffmpeg(
        &tools,
        &config,
        &video_filters,
        audio_filters_opt.as_deref(),
        &audio_codec_when_touch,
    )?;

    let progress_handle = progress::pump_progress(session.stdout, ui);
    ffmpeg::wait_for_completion(session.child)?;
    progress_handle
        .join()
        .expect("progress thread panicked")?;

    Ok(())
}
