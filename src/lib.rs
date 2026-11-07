#![forbid(unsafe_code)]

//! Low-click loop-point search and WAV rendering.

use serde::Serialize;
use std::error::Error as StdError;
use std::fmt;
use std::path::Path;

/// An in-memory 16-bit PCM mono or stereo recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioBuffer {
    sample_rate: u32,
    channels: u16,
    samples: Vec<i16>,
}

impl AudioBuffer {
    /// Construct a validated interleaved audio buffer.
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<i16>) -> Result<Self, LoopError> {
        if sample_rate == 0 {
            return Err(LoopError::InvalidAudio("sample rate must be positive"));
        }
        if !(1..=2).contains(&channels) {
            return Err(LoopError::InvalidAudio(
                "only mono and stereo audio are supported",
            ));
        }
        if samples.is_empty() {
            return Err(LoopError::InvalidAudio("audio contains no samples"));
        }
        if samples.len() % usize::from(channels) != 0 {
            return Err(LoopError::InvalidAudio(
                "sample count is not divisible by channel count",
            ));
        }
        Ok(Self {
            sample_rate,
            channels,
            samples,
        })
    }

    /// Recording sample rate in hertz.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of interleaved channels.
    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.channels
    }

    /// Number of audio frames.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// Interleaved PCM samples.
    #[must_use]
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }
}

/// Loop-search controls expressed in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchConfig {
    pub target_ms: u64,
    pub tolerance_ms: u64,
    pub start_search_ms: u64,
    pub compare_ms: u64,
    pub step_ms: u64,
    pub candidates: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            target_ms: 2_000,
            tolerance_ms: 100,
            start_search_ms: 150,
            compare_ms: 12,
            step_ms: 2,
            candidates: 5,
        }
    }
}

/// A scored loop region. Lower scores indicate a smoother predicted wrap.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LoopPlan {
    pub start_frame: usize,
    pub end_frame: usize,
    pub frames: usize,
    pub duration_ms: f64,
    pub score: f64,
    pub boundary_delta: f64,
    pub window_delta: f64,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Errors returned by WAV I/O and loop search.
#[derive(Debug)]
pub enum LoopError {
    Wav(hound::Error),
    Io(std::io::Error),
    InvalidAudio(&'static str),
    InvalidConfig(&'static str),
    NoCandidate,
}

impl fmt::Display for LoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wav(error) => write!(formatter, "WAV error: {error}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::InvalidAudio(message) => write!(formatter, "invalid audio: {message}"),
            Self::InvalidConfig(message) => write!(formatter, "invalid search config: {message}"),
            Self::NoCandidate => formatter.write_str("no valid loop candidate fits the recording"),
        }
    }
}

impl StdError for LoopError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Wav(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::InvalidAudio(_) | Self::InvalidConfig(_) | Self::NoCandidate => None,
        }
    }
}

impl From<hound::Error> for LoopError {
    fn from(error: hound::Error) -> Self {
        Self::Wav(error)
    }
}

impl From<std::io::Error> for LoopError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Read a mono or stereo 16-bit PCM WAV file.
pub fn read_wav(path: impl AsRef<Path>) -> Result<AudioBuffer, LoopError> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(LoopError::InvalidAudio(
            "input must be 16-bit integer PCM WAV",
        ));
    }
    let samples = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
    AudioBuffer::new(spec.sample_rate, spec.channels, samples)
}

/// Find the best distinct loop regions, ordered from lowest to highest score.
pub fn find_loop_candidates(
    audio: &AudioBuffer,
    config: SearchConfig,
) -> Result<Vec<LoopPlan>, LoopError> {
    validate_config(config)?;
    let rate = u64::from(audio.sample_rate);
    let target = ms_to_frames(config.target_ms, rate)?;
    let tolerance = ms_to_frames(config.tolerance_ms, rate)?;
    let start_limit = ms_to_frames(config.start_search_ms, rate)?;
    let compare = ms_to_frames(config.compare_ms, rate)?;
    let step = ms_to_frames(config.step_ms, rate)?.max(1);
    let total = audio.frame_count();

    if compare < 2 {
        return Err(LoopError::InvalidConfig(
            "compare window must contain at least two frames",
        ));
    }
    if total <= compare.saturating_mul(2) || target <= compare {
        return Err(LoopError::NoCandidate);
    }

    let max_start = start_limit.min(total.saturating_sub(target + compare));
    let mut plans = Vec::new();

    for start in (0..=max_start).step_by(step) {
        let nominal_end = start.saturating_add(target);
        let min_end = nominal_end
            .saturating_sub(tolerance)
            .max(start + compare + 1);
        let max_end = nominal_end.saturating_add(tolerance).min(total);
        if min_end > max_end {
            continue;
        }
        for end in (min_end..=max_end).step_by(step) {
            let (score, boundary_delta, window_delta) = seam_score(audio, start, end, compare);
            plans.push(LoopPlan {
                start_frame: start,
                end_frame: end,
                frames: end - start,
                duration_ms: frames_to_ms(end - start, audio.sample_rate),
                score,
                boundary_delta,
                window_delta,
                sample_rate: audio.sample_rate,
                channels: audio.channels,
            });
        }
    }

    plans.sort_by(|left, right| {
        left.score
            .total_cmp(&right.score)
            .then_with(|| left.start_frame.cmp(&right.start_frame))
            .then_with(|| left.end_frame.cmp(&right.end_frame))
    });
    plans.dedup_by(|left, right| {
        left.start_frame == right.start_frame && left.end_frame == right.end_frame
    });
    plans.truncate(config.candidates);
    if plans.is_empty() {
        return Err(LoopError::NoCandidate);
    }
    Ok(plans)
}

/// Write the selected loop region as a new 16-bit PCM WAV file.
pub fn write_loop(
    path: impl AsRef<Path>,
    audio: &AudioBuffer,
    plan: &LoopPlan,
) -> Result<(), LoopError> {
    if plan.start_frame >= plan.end_frame || plan.end_frame > audio.frame_count() {
        return Err(LoopError::InvalidConfig(
            "loop plan is outside the audio buffer",
        ));
    }
    let spec = hound::WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let channels = usize::from(audio.channels);
    let begin = plan.start_frame * channels;
    let end = plan.end_frame * channels;
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in &audio.samples[begin..end] {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(())
}

fn validate_config(config: SearchConfig) -> Result<(), LoopError> {
    if config.target_ms == 0 {
        return Err(LoopError::InvalidConfig("target duration must be positive"));
    }
    if config.compare_ms == 0 {
        return Err(LoopError::InvalidConfig("compare window must be positive"));
    }
    if config.step_ms == 0 {
        return Err(LoopError::InvalidConfig("step must be positive"));
    }
    if config.candidates == 0 {
        return Err(LoopError::InvalidConfig("candidate count must be positive"));
    }
    Ok(())
}

fn ms_to_frames(milliseconds: u64, sample_rate: u64) -> Result<usize, LoopError> {
    let frames = milliseconds
        .checked_mul(sample_rate)
        .ok_or(LoopError::InvalidConfig("duration is too large"))?
        / 1_000;
    usize::try_from(frames).map_err(|_| LoopError::InvalidConfig("duration is too large"))
}

fn frames_to_ms(frames: usize, sample_rate: u32) -> f64 {
    frames as f64 * 1_000.0 / f64::from(sample_rate)
}

fn seam_score(audio: &AudioBuffer, start: usize, end: usize, compare: usize) -> (f64, f64, f64) {
    const SCALE: f64 = 32_768.0;
    let channels = usize::from(audio.channels);
    let tail_start = end - compare;
    let mut window_sum = 0.0;
    let mut slope_sum = 0.0;
    let mut boundary_sum = 0.0;
    let compared_samples = compare * channels;

    for channel in 0..channels {
        let head = f64::from(audio.samples[start * channels + channel]) / SCALE;
        let tail = f64::from(audio.samples[(end - 1) * channels + channel]) / SCALE;
        boundary_sum += (tail - head).powi(2);

        for offset in 0..compare {
            let head_index = (start + offset) * channels + channel;
            let tail_index = (tail_start + offset) * channels + channel;
            let head_value = f64::from(audio.samples[head_index]) / SCALE;
            let tail_value = f64::from(audio.samples[tail_index]) / SCALE;
            window_sum += (head_value - tail_value).powi(2);

            if offset > 0 {
                let previous_head = f64::from(audio.samples[head_index - channels]) / SCALE;
                let previous_tail = f64::from(audio.samples[tail_index - channels]) / SCALE;
                slope_sum += ((head_value - previous_head) - (tail_value - previous_tail)).powi(2);
            }
        }
    }

    let window_delta = (window_sum / compared_samples as f64).sqrt();
    let slope_denominator = ((compare - 1) * channels) as f64;
    let slope_delta = (slope_sum / slope_denominator).sqrt();
    let boundary_delta = (boundary_sum / channels as f64).sqrt();
    let score = window_delta + 0.5 * slope_delta + 2.0 * boundary_delta;
    (score, boundary_delta, window_delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    fn periodic_audio(channels: u16) -> AudioBuffer {
        let sample_rate = 1_000;
        let frames = 2_500;
        let mut samples = Vec::new();
        for frame in 0..frames {
            let value = ((frame as f64 / 100.0 * TAU).sin() * 20_000.0) as i16;
            for channel in 0..channels {
                samples.push(if channel == 0 { value } else { value / 2 });
            }
        }
        AudioBuffer::new(sample_rate, channels, samples).expect("valid test audio")
    }

    #[test]
    fn periodic_wave_has_a_low_score_loop() {
        let audio = periodic_audio(1);
        let config = SearchConfig {
            target_ms: 1_000,
            tolerance_ms: 20,
            start_search_ms: 20,
            compare_ms: 20,
            step_ms: 1,
            candidates: 3,
        };
        let plans = find_loop_candidates(&audio, config).expect("candidate search");
        assert_eq!(plans.len(), 3);
        assert!(plans[0].score < 0.05, "score was {}", plans[0].score);
        assert!((980.0..=1_020.0).contains(&plans[0].duration_ms));
        assert!(plans.windows(2).all(|pair| pair[0].score <= pair[1].score));
    }

    #[test]
    fn stereo_audio_is_supported() {
        let audio = periodic_audio(2);
        let plans = find_loop_candidates(
            &audio,
            SearchConfig {
                target_ms: 1_000,
                ..SearchConfig::default()
            },
        )
        .expect("stereo candidate search");
        assert_eq!(plans[0].channels, 2);
    }

    #[test]
    fn too_short_audio_has_no_candidate() {
        let audio = AudioBuffer::new(1_000, 1, vec![0; 10]).expect("valid test audio");
        let error = find_loop_candidates(&audio, SearchConfig::default())
            .expect_err("short audio must fail");
        assert!(matches!(error, LoopError::NoCandidate));
    }

    #[test]
    fn invalid_channel_count_is_rejected() {
        let error = AudioBuffer::new(44_100, 3, vec![0; 30]).expect_err("3 channels must fail");
        assert!(matches!(error, LoopError::InvalidAudio(_)));
    }
}
