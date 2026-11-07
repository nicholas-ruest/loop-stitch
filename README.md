# loop-stitch

`loop-stitch` finds low-discontinuity loop points in mono or stereo 16-bit PCM WAV audio. Give it a desired duration and it searches nearby start/end pairs, compares the waveform and slope on both sides of the wrap, then ranks or renders the smoothest candidate.

It is useful for ambience beds, game audio, procedural sound, and quick sample preparation when you want a deterministic first pass before editing by ear.

## Install

Rust 1.85 or newer is required.

```console
cargo install --git https://github.com/nicholas-ruest/loop-stitch
```

## Try it

Generate a five-second stereo demo entirely in Rust:

```console
cargo run --example make_demo -- demo.wav
```

Inspect the five best loops near two seconds:

```console
cargo run --release -- analyze demo.wav --target-ms 2000
```

Render the best candidate:

```console
cargo run --release -- render demo.wav demo-loop.wav --target-ms 2000
```

The JSON result includes exact frame boundaries, duration, overall score, boundary delta, and window delta. Lower scores predict a smoother wrap. Use `--tolerance-ms`, `--start-search-ms`, `--compare-ms`, and `--step-ms` to control the search.

Output creation is fail-closed: `render` will not replace a file unless `--force` is supplied.

## How it works

For every search-grid pair around the target duration, `loop-stitch` compares a short window at the beginning of the candidate with the window immediately before its end. The score combines:

- RMS waveform difference across the windows;
- RMS first-derivative difference, which penalizes slope changes; and
- the direct last-sample-to-first-sample jump.

The search is deterministic and channel-aware. Rendering copies the selected frame region without resampling or adding a hidden crossfade.

## Verification

```console
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo +1.85.0 check --all-features
cargo doc --no-deps --all-features
cargo package
```

## Limits

- Input and output are mono or stereo 16-bit integer PCM WAV.
- The score is a local waveform heuristic, not a perceptual audio model; audition the result before shipping it.
- The renderer trims to the chosen boundaries. It does not time-stretch, resample, or crossfade.

## License

MIT
