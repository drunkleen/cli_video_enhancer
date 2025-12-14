use crate::cli::AppConfig;
use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use which::which;

#[derive(Debug, Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Debug)]
pub struct FfmpegSession {
    pub child: Child,
    pub stdout: ChildStdout,
}

pub fn resolve_tools(ffmpeg: Option<PathBuf>, ffprobe: Option<PathBuf>) -> Result<Tools> {
    Ok(Tools {
        ffmpeg: resolve_bin(ffmpeg, "ffmpeg")?,
        ffprobe: resolve_bin(ffprobe, "ffprobe")?,
    })
}

pub fn probe_duration_seconds(tools: &Tools, input: &Path) -> Result<f64> {
    let out = Command::new(&tools.ffprobe)
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

pub fn spawn_ffmpeg(
    tools: &Tools,
    cfg: &AppConfig,
    video_filters: &str,
    audio_filters: Option<&str>,
    audio_codec: &[&str],
) -> Result<FfmpegSession> {
    let mut cmd = Command::new(&tools.ffmpeg);
    if !cfg.verbose {
        cmd.arg("-hide_banner")
            .arg("-nostats")
            .arg("-loglevel")
            .arg("error");
    }
    cmd.arg("-y")
        .arg("-progress")
        .arg("-")
        .arg("-i")
        .arg(&cfg.input);

    if !video_filters.is_empty() {
        cmd.arg("-vf").arg(video_filters);
        cmd.args(["-c:v", "libx264"]);
        cmd.args(["-crf", &cfg.crf.to_string()]);
        cmd.args(["-preset", &cfg.preset]);
        cmd.args(["-pix_fmt", "yuv420p"]);
        cmd.args(["-threads", &cfg.threads.to_string()]);
    } else {
        cmd.args(["-c:v", "copy"]);
        if cfg.threads > 0 {
            cmd.args(["-threads", &cfg.threads.to_string()]);
        }
    }

    if let Some(af) = audio_filters {
        cmd.arg("-af").arg(af);
        cmd.args(audio_codec);
    } else {
        cmd.args(["-c:a", "copy"]);
    }

    cmd.arg(&cfg.output);

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(if cfg.verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .spawn()
        .context("failed to spawn ffmpeg")?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to capture ffmpeg stdout"))?;

    Ok(FfmpegSession { child, stdout })
}

pub fn wait_for_completion(mut child: Child) -> Result<()> {
    let status = child.wait()?;
    if !status.success() {
        bail!("ffmpeg failed with status: {}", status);
    }
    Ok(())
}

fn resolve_bin(bin_opt: Option<PathBuf>, default: &str) -> Result<PathBuf> {
    if let Some(path) = bin_opt {
        if path.is_file() {
            return Ok(path);
        }
        bail!("Provided binary not found: {}", path.display());
    }

    which(default)
        .or_else(|_| {
            if cfg!(windows) {
                let exe = format!("{default}.exe");
                which(&exe)
            } else {
                Err(which::Error::CannotFindBinaryPath)
            }
        })
        .with_context(|| format!("`{default}` not found in PATH"))
}
