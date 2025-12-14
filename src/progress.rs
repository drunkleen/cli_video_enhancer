use anyhow::Result;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use regex::Regex;
use std::io::{BufRead, BufReader, Read};
use std::thread;
use std::time::Duration;

pub struct ProgressUi {
    _multi: MultiProgress,
    spinner: ProgressBar,
    bar: ProgressBar,
    total_ms: u64,
}

impl ProgressUi {
    pub fn new(total_ms: u64, audio_time_stretch: bool) -> Self {
        let multi = MultiProgress::new();

        let spinner = multi.add(ProgressBar::new_spinner());
        spinner.enable_steady_tick(Duration::from_millis(80));
        spinner.set_style(
            ProgressStyle::with_template("{spinner} {msg}")
                .unwrap()
                .tick_strings(&["-", "\\", "|", "/"]),
        );
        spinner.set_message("Preparing.");

        let bar = multi.add(ProgressBar::new(total_ms));
        bar.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}]  [{bar:60.cyan/bright-black}] {percent:>3}%  {pos}/{len}ms  ETA:{eta_precise}\n{wide_msg}"
            )
            .unwrap()
            .progress_chars("#>-"),
        );
        bar.set_message("Building filter graph.");
        if audio_time_stretch {
            bar.set_message("Audio will be time-stretched (atempo)...");
        }

        Self {
            _multi: multi,
            spinner,
            bar,
            total_ms,
        }
    }

    fn update_stage(&self, pos_ms: u64) {
        self.bar.set_position(pos_ms);
        let pct = (pos_ms as f64) / (self.total_ms as f64);
        if pct < 0.10 {
            self.spinner.set_message("Preparing filters.");
            self.bar
                .set_message("Applying selected filters (if any)...");
        } else if pct < 0.65 {
            self.spinner.set_message("Encoding video.");
            self.bar.set_message("Processing frames...");
        } else if pct < 0.95 {
            self.spinner.set_message("Adjusting/encoding audio.");
            self.bar
                .set_message("Applying atempo (if speed != 1.0)...");
        } else {
            self.spinner.set_message("Finalizing and muxing.");
            self.bar
                .set_message("Muxing, writing headers, closing output...");
        }
    }

    fn finish(&self) {
        self.bar.finish_with_message("Done");
        self.spinner.finish_with_message("Completed");
    }
}

pub fn pump_progress<R: Read + Send + 'static>(
    reader: R,
    ui: ProgressUi,
) -> thread::JoinHandle<Result<()>> {
    thread::spawn(move || {
        let re_kv = Regex::new(r"^(\w+)=([\w\-\.:]+)$").unwrap();
        let reader = BufReader::new(reader);

        for line in reader.lines() {
            let line = line?;
            if let Some(caps) = re_kv.captures(&line) {
                let key = &caps[1];
                let val = &caps[2];
                match key {
                    "out_time_ms" => {
                        let ms: u64 = val.parse().unwrap_or(0);
                        let pos_ms = (ms / 1000).min(ui.total_ms);
                        ui.update_stage(pos_ms);
                    }
                    "progress" if val == "end" => {
                        ui.finish();
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })
}
