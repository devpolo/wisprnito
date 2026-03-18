use anyhow::Result;
use num_complex::Complex;
use rand::Rng;
use std::f32::consts::PI;

use super::formant::FormantShifter;
use super::params::AnonymizationParams;
use super::phase_vocoder::PhaseVocoder;
use super::resampler::Resampler;
use super::stft::Stft;

/// Full voice anonymization pipeline.
///
/// Processing order:
/// 1. Optional input resampling (if input_rate != processing_rate)
/// 2. STFT analysis
/// 3. Phase vocoder time-stretch by alpha = 2^(pitch_semitones/12)
/// 4. Phase jitter: add small random perturbation to each bin's phase
/// 5. ISTFT synthesis
/// 6. Resample by 1/alpha to achieve net pitch shift
/// 7. Formant shift
pub struct Pipeline {
    params: AnonymizationParams,
    stft: Stft,
    phase_vocoder: PhaseVocoder,
    formant_shifter: FormantShifter,
    pitch_resampler: Option<Resampler>,
    alpha: f32,
    sample_rate: u32,
    fft_size: usize,
}

impl Pipeline {
    pub fn new(
        params: AnonymizationParams,
        sample_rate: u32,
        fft_size: usize,
    ) -> Result<Self> {
        let hop_size = fft_size / 4;
        let alpha = 2.0f32.powf(params.pitch_semitones / 12.0);

        let stft = Stft::new(fft_size, hop_size);
        let phase_vocoder = PhaseVocoder::new(fft_size, hop_size);
        let formant_shifter = FormantShifter::new(fft_size);

        // Pre-build the pitch-shift resampler.
        // After time-stretching by alpha, we resample by 1/alpha to get pitch shift.
        // This means: new_rate = sample_rate / alpha (perceived as pitch shift).
        // Using rubato: ratio = output_rate / input_rate = 1/alpha
        let pitch_resampler = if (alpha - 1.0).abs() > 1e-6 {
            // We express the ratio as integer rates for the Resampler constructor.
            // from_rate = sample_rate, to_rate = sample_rate / alpha
            // But Resampler takes u32, so we scale: from = sample_rate * 1000, to = (sample_rate / alpha) * 1000
            let from_rate = (sample_rate as f32 * 1000.0) as u32;
            let to_rate = (sample_rate as f32 * 1000.0 / alpha) as u32;
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
            sample_rate,
            fft_size,
        })
    }

    /// Process a block of audio samples through the full anonymization pipeline.
    pub fn process_block(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        // Step 1: STFT analysis
        let frames = self.stft.analyze(input);
        if frames.is_empty() {
            return Vec::new();
        }

        // Step 2: Phase vocoder time-stretch
        let stretched_frames = self.phase_vocoder.process(&frames, self.alpha);

        // Step 3: Apply phase jitter
        let jittered_frames = self.apply_phase_jitter(&stretched_frames);

        // Step 4: ISTFT synthesis
        let time_stretched = self.stft.synthesize(&jittered_frames);

        // Step 5: Resample by 1/alpha for net pitch shift
        let pitch_shifted = if let Some(ref mut resampler) = self.pitch_resampler {
            resampler.process(&time_stretched).unwrap_or(time_stretched)
        } else {
            time_stretched
        };

        // Step 6: Formant shift
        self.formant_shifter
            .shift(&pitch_shifted, self.params.formant_ratio)
    }

    /// Apply random phase jitter to each STFT bin.
    fn apply_phase_jitter(
        &self,
        frames: &[Vec<Complex<f32>>],
    ) -> Vec<Vec<Complex<f32>>> {
        let mut rng = rand::rng();
        let jitter = self.params.phase_jitter;

        frames
            .iter()
            .map(|frame| {
                frame
                    .iter()
                    .map(|&bin| {
                        let mag = bin.norm();
                        let phase = bin.arg();
                        let perturbation = jitter * rng.random_range(-PI..PI);
                        Complex::from_polar(mag, phase + perturbation)
                    })
                    .collect()
            })
            .collect()
    }

    /// Approximate latency in samples introduced by the pipeline.
    pub fn latency_samples(&self) -> usize {
        self.fft_size
    }

    /// Get a reference to the current anonymization parameters.
    pub fn params(&self) -> &AnonymizationParams {
        &self.params
    }
}
