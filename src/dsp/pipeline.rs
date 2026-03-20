use anyhow::Result;
use num_complex::Complex;
use rand::Rng;
use std::f32::consts::PI;

use super::formant::FormantShifter;
use super::params::AnonymizationParams;
use super::phase_vocoder::PhaseVocoder;
use super::resampler::Resampler;
use super::stft::Stft;

/// Full voice anonymization pipeline (streaming, stateful).
///
/// Processing order per block:
/// 1. Push samples into the STFT analysis buffer.
/// 2. For each available analysis frame:
///    a. Phase vocoder  — maintains phase coherence across hops.
///    b. Phase jitter   — per-bin random phase perturbation (breaks vocoder fingerprint).
///    c. ISTFT          — overlap-add synthesis.
/// 3. Collect synthesized samples.
/// 4. Resample by `alpha` to achieve net pitch shift (no time stretch).
/// 5. Formant shift via LPC envelope manipulation.
pub struct Pipeline {
    params: AnonymizationParams,
    stft: Stft,
    phase_vocoder: PhaseVocoder,
    formant_shifter: FormantShifter,
    pitch_resampler: Option<Resampler>,
    alpha: f32,
}

impl Pipeline {
    pub fn new(
        params: AnonymizationParams,
        sample_rate: u32,
        fft_size: usize,
    ) -> Result<Self> {
        let hop_size = fft_size / 4; // 75% overlap — required for COLA with Hann window
        let alpha = 2.0f32.powf(params.pitch_semitones / 12.0);

        let stft = Stft::new(fft_size, hop_size);
        let phase_vocoder = PhaseVocoder::new(fft_size, hop_size);
        let formant_shifter = FormantShifter::new(fft_size);

        // Resample by `alpha` to pitch-shift the time-domain output.
        // alpha > 1 → pitch up (fewer output samples at same perceived rate).
        let pitch_resampler = if (alpha - 1.0).abs() > 1e-4 {
            let from_rate = (sample_rate as f32 * 1000.0) as u32;
            let to_rate = (sample_rate as f32 * 1000.0 * alpha) as u32;
            Some(Resampler::new(from_rate, to_rate, 1)?)
        } else {
            None
        };

        Ok(Self {
            params,
            stft,
            phase_vocoder,
            formant_shifter,
            pitch_resampler,
            alpha,
        })
    }

    /// Process a block of audio samples through the full anonymization pipeline.
    /// Returns however many output samples are ready (may differ from input length).
    pub fn process_block(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        // Step 1: push samples into the streaming STFT analysis buffer
        self.stft.push_samples(input);

        // Step 2: process every complete analysis frame that's now available
        while self.stft.has_analysis_frame() {
            let frame = match self.stft.pop_analysis_frame() {
                Some(f) => f,
                None => break,
            };

            // 2a. Phase vocoder — per-frame, stateful, no reset between blocks
            let pv_frame = self.phase_vocoder.process_frame(&frame);

            // 2b. Phase jitter
            let jitter_frame = self.apply_phase_jitter(&pv_frame);

            // 2c. Overlap-add synthesis
            self.stft.push_synthesis_frame(&jitter_frame);
        }

        // Step 3: collect all synthesized samples that are ready
        let synthesized = self.stft.drain_output();
        if synthesized.is_empty() {
            return Vec::new();
        }

        // Step 4: resample for pitch shift
        let pitch_shifted = match self.pitch_resampler {
            Some(ref mut r) => r.process(&synthesized).unwrap_or(synthesized),
            None => synthesized,
        };

        // Step 5: formant shift
        self.formant_shifter
            .shift(&pitch_shifted, self.params.formant_ratio)
    }

    fn apply_phase_jitter(&self, frame: &[Complex<f32>]) -> Vec<Complex<f32>> {
        let mut rng = rand::rng();
        let jitter = self.params.phase_jitter;
        frame
            .iter()
            .map(|&bin| {
                let mag = bin.norm();
                let phase = bin.arg();
                let perturbation = jitter * rng.random_range(-PI..PI);
                Complex::from_polar(mag, phase + perturbation)
            })
            .collect()
    }
}
