use num_complex::Complex;
use std::f32::consts::PI;

/// Phase vocoder for time-stretching audio in the frequency domain.
pub struct PhaseVocoder {
    hop_size: usize,
    freq_bins: usize,
    /// Phase accumulator per bin for synthesis
    phase_accum: Vec<f32>,
    /// Previous analysis frame phases
    prev_phase: Vec<f32>,
    /// Expected phase advance per bin per hop
    omega: Vec<f32>,
}

impl PhaseVocoder {
    pub fn new(fft_size: usize, hop_size: usize) -> Self {
        let freq_bins = fft_size / 2 + 1;

        // Expected phase advance per bin = 2*pi*bin*hop_size/fft_size
        let omega: Vec<f32> = (0..freq_bins)
            .map(|k| 2.0 * PI * k as f32 * hop_size as f32 / fft_size as f32)
            .collect();

        Self {
            hop_size,
            freq_bins,
            phase_accum: vec![0.0; freq_bins],
            prev_phase: vec![0.0; freq_bins],
            omega,
        }
    }

    /// Time-stretch STFT frames by factor `alpha`.
    ///
    /// alpha > 1 stretches (slower), alpha < 1 compresses (faster).
    /// The output will have approximately `ceil(frames.len() * alpha)` frames.
    pub fn process(
        &mut self,
        frames: &[Vec<Complex<f32>>],
        alpha: f32,
    ) -> Vec<Vec<Complex<f32>>> {
        if frames.is_empty() {
            return Vec::new();
        }

        let num_input = frames.len();
        let num_output = ((num_input as f32) * alpha).ceil() as usize;
        let mut output_frames = Vec::with_capacity(num_output);

        // Reset accumulators
        self.phase_accum.fill(0.0);
        self.prev_phase.fill(0.0);

        // Initialize phase accumulator from first frame
        for k in 0..self.freq_bins {
            self.phase_accum[k] = frames[0][k].arg();
            self.prev_phase[k] = frames[0][k].arg();
        }

        // First output frame uses first input frame magnitudes with accumulated phase
        let mut out_frame = vec![Complex::new(0.0, 0.0); self.freq_bins];
        for k in 0..self.freq_bins {
            let mag = frames[0][k].norm();
            out_frame[k] = Complex::from_polar(mag, self.phase_accum[k]);
        }
        output_frames.push(out_frame);

        // Generate remaining output frames by interpolating input position
        for out_idx in 1..num_output {
            // Map output frame index back to (fractional) input frame index
            let in_pos = out_idx as f32 / alpha;
            let in_idx = in_pos.floor() as usize;
            let frac = in_pos - in_idx as f32;

            // Clamp to valid range
            let idx0 = in_idx.min(num_input - 1);
            let idx1 = (in_idx + 1).min(num_input - 1);

            let mut out_frame = vec![Complex::new(0.0, 0.0); self.freq_bins];

            for k in 0..self.freq_bins {
                // Interpolate magnitude
                let mag0 = frames[idx0][k].norm();
                let mag1 = frames[idx1][k].norm();
                let mag = mag0 + frac * (mag1 - mag0);

                // Compute instantaneous frequency from the analysis frame pair
                let phase0 = frames[idx0][k].arg();
                let phase1 = frames[idx1][k].arg();

                // Phase difference and unwrap
                let dp = phase1 - phase0;
                let expected = self.omega[k];
                let deviation = dp - expected;
                // Wrap deviation to [-pi, pi]
                let wrapped = deviation - (2.0 * PI) * ((deviation + PI) / (2.0 * PI)).floor();
                let inst_freq = expected + wrapped;

                // Advance phase accumulator by the synthesis hop
                // (synthesis hop = analysis hop since we're changing frame count, not hop)
                self.phase_accum[k] += inst_freq;

                out_frame[k] = Complex::from_polar(mag, self.phase_accum[k]);
            }

            output_frames.push(out_frame);
        }

        output_frames
    }

    /// Reset internal state (phase accumulators).
    pub fn reset(&mut self) {
        self.phase_accum.fill(0.0);
        self.prev_phase.fill(0.0);
    }
}
