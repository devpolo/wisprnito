use num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex, ComplexToReal};
use std::f32::consts::PI;
use std::sync::Arc;

pub struct Stft {
    fft_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    // Pre-allocated scratch buffers
    scratch_forward: Vec<Complex<f32>>,
    scratch_inverse: Vec<Complex<f32>>,
    time_buf: Vec<f32>,
    freq_buf: Vec<Complex<f32>>,
}

impl Stft {
    pub fn new(fft_size: usize, hop_size: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);

        let scratch_forward = forward.make_scratch_vec();
        let scratch_inverse = inverse.make_scratch_vec();
        let time_buf = vec![0.0f32; fft_size];
        let freq_buf = forward.make_output_vec();

        let window = hann_window(fft_size);

        Self {
            fft_size,
            hop_size,
            window,
            forward,
            inverse,
            scratch_forward,
            scratch_inverse,
            time_buf,
            freq_buf,
        }
    }

    /// Analyze input signal into overlapping STFT frames.
    /// Returns a vector of complex spectrum frames (each of length fft_size/2 + 1).
    pub fn analyze(&mut self, input: &[f32]) -> Vec<Vec<Complex<f32>>> {
        let mut frames = Vec::new();
        let num_samples = input.len();
        let mut offset = 0usize;

        while offset + self.fft_size <= num_samples {
            // Apply window
            for i in 0..self.fft_size {
                self.time_buf[i] = input[offset + i] * self.window[i];
            }

            // Forward FFT
            self.forward
                .process_with_scratch(&mut self.time_buf, &mut self.freq_buf, &mut self.scratch_forward)
                .expect("forward FFT failed");

            frames.push(self.freq_buf.clone());

            offset += self.hop_size;
        }

        frames
    }

    /// Synthesize time-domain signal from STFT frames using overlap-add.
    pub fn synthesize(&mut self, frames: &[Vec<Complex<f32>>]) -> Vec<f32> {
        if frames.is_empty() {
            return Vec::new();
        }

        let output_len = self.fft_size + (frames.len() - 1) * self.hop_size;
        let mut output = vec![0.0f32; output_len];

        // Compute the synthesis normalization factor for the overlap-add.
        // For Hann window with 75% overlap (hop = fft_size/4), the sum-of-squares = 1.5.
        // General COLA normalization:
        let window_sum = cola_normalization(&self.window, self.hop_size);

        for (frame_idx, frame) in frames.iter().enumerate() {
            let out_offset = frame_idx * self.hop_size;

            // Copy spectrum into working buffer
            self.freq_buf.copy_from_slice(frame);

            // Inverse FFT
            self.inverse
                .process_with_scratch(&mut self.freq_buf, &mut self.time_buf, &mut self.scratch_inverse)
                .expect("inverse FFT failed");

            // Apply synthesis window and normalize, then overlap-add
            let norm = 1.0 / (self.fft_size as f32 * window_sum);
            for i in 0..self.fft_size {
                output[out_offset + i] += self.time_buf[i] * self.window[i] * norm;
            }
        }

        output
    }

    pub fn fft_size(&self) -> usize {
        self.fft_size
    }

    pub fn hop_size(&self) -> usize {
        self.hop_size
    }

    pub fn freq_bins(&self) -> usize {
        self.fft_size / 2 + 1
    }
}

/// Generate a Hann window of the given length.
fn hann_window(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let phase = 2.0 * PI * i as f32 / len as f32;
            0.5 * (1.0 - phase.cos())
        })
        .collect()
}

/// Compute the COLA normalization factor: sum of squared window values at each hop position.
fn cola_normalization(window: &[f32], hop_size: usize) -> f32 {
    let fft_size = window.len();
    let num_overlaps = (fft_size + hop_size - 1) / hop_size;
    // Compute the sum of squared window values that overlap at a single output sample (center)
    let center = fft_size / 2;
    let mut sum = 0.0f32;
    for k in 0..num_overlaps {
        let start = k * hop_size;
        if center >= start && center < start + fft_size {
            let idx = center - start;
            sum += window[idx] * window[idx];
        }
    }
    sum
}
