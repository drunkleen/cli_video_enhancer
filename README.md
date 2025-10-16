# cli_video_enhancer

CLI to **enhance video** (denoise/sharpen/color), **change speed**, and show a **modern progress UI**.
By default (no enhancement flags and `--speed 1.0`) it **stream-copies** video & audio (no re-encode, no quality change).

## Features

* 0–100 controls (50 = unchanged): `--brightness`, `--contrast`, `--saturation`, `--sharpen`, `--denoise`
* Speed control: `-s/--speed` (e.g., `1.25`, `0.75`)

## Requirements

* FFmpeg & FFprobe in `PATH`
* Rust (edition 2021)

## Build

```bash
cargo build --release
```

## Usage

```bash
# No changes (pure copy)
video_enhancer -i in.mp4 -o out.mp4

# Speed up 1.25× (re-encodes as needed)
video_enhancer -i in.mp4 -s 1.25 -o out_fast.mp4

# Color tweaks (50 = unchanged)
video_enhancer -i in.mp4 --brightness 60 --contrast 60 --saturation 55 -o out_pop.mp4

# Detail control
video_enhancer -i in.mp4 --sharpen 75 --denoise 70 -o out_clean_sharp.mp4
```

## Flags (selected)

* `-i, --input <FILE>` (required)
* `-o, --output <FILE>` (default: `<input>_enhanced_speed<S>.mp4`)
* `-s, --speed <FLOAT>` (default: `1.0`)
* `--brightness/--contrast/--saturation/--sharpen/--denoise <0..100>` (50 = unchanged)
* `--crf <INT>` (default: `17`) & `--preset <STRING>` (default: `slow`) — used only when video is re-encoded
* `--verbose`

## Tests

```bash
cargo test
```

## Notes

* Stream copy may fail if output container is incompatible (e.g., VP9/Opus to `.mp4`). Use `.mkv` or re-encode.
