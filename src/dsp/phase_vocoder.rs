use num_complex::Complex;
use std::f32::consts::PI;

/// Phase vocoder — processes one STFT frame at a time, maintaining phase
/// accumulator state across calls so block boundaries don't cause clicks.
pub struct PhaseVocoder {
    freq_bins: usize,
    /// Expected phase advance per bin per analysis hop: 2π·k·H/N
    omega: Vec<f32>,
    /// Per-bin phase accumulator for synthesis output
    phase_accum: Vec<f32>,
    /// Phase of the previous analysis frame
    prev_phase: Vec<f32>,
    initialized: bool,
}

impl PhaseVocoder {
    pub fn new(fft_size: usize, hop_size: usize) -> Self {
        let freq_bins = fft_size / 2 + 1;
        let omega: Vec<f32> = (0..freq_bins)
            .map(|k| 2.0 * PI * k as f32 * hop_size as f32 / fft_size as f32)
            .collect();

        Self {
            freq_bins,
            omega,
            phase_accum: vec![0.0f32; freq_bins],
            prev_phase: vec![0.0f32; freq_bins],
            initialized: false,
        }
    }

    /// Process a single STFT frame, returning the phase-corrected synthesis frame.
    ///
    /// The synthesis hop equals the analysis hop (no time stretch). Pitch shift is
    /// achieved by resampling the ISTFT output. The vocoder's job here is solely to
    /// maintain per-bin phase coherence across hops, which prevents phasiness artifacts.
    pub fn process_frame(&mut self, frame: &[Complex<f32>]) -> Vec<Complex<f32>> {
        let mut out = vec![Complex::new(0.0f32, 0.0f32); self.freq_bins];

        if !self.initialized {
            // Seed accumulators from the first frame's phases
            for k in 0..self.freq_bins {
                self.phase_accum[k] = frame[k].arg();
                self.prev_phase[k] = frame[k].arg();
            }
            self.initialized = true;
            for k in 0..self.freq_bins {
                out[k] = Complex::from_polar(frame[k].norm(), self.phase_accum[k]);
            }
            return out;
        }

        for k in 0..self.freq_bins {
            let mag = frame[k].norm();
            let phase = frame[k].arg();

            // Deviation from the expected phase advance
            let dp = phase - self.prev_phase[k];
            let deviation = dp - self.omega[k];
            // Wrap deviation to [-π, π]
            let wrapped = deviation - (2.0 * PI) * ((deviation + PI) / (2.0 * PI)).floor();
            let inst_freq = self.omega[k] + wrapped;

            // Advance synthesis accumulator by the analysis hop (synthesis hop == analysis hop)
            self.phase_accum[k] += inst_freq;
            self.prev_phase[k] = phase;

            out[k] = Complex::from_polar(mag, self.phase_accum[k]);
        }

        out
    }

    pub fn reset(&mut self) {
        self.phase_accum.fill(0.0);
        self.prev_phase.fill(0.0);
        self.initialized = false;
    }
}
