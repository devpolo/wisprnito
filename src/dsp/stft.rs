use num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::sync::Arc;

pub struct Stft {
    fft_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    /// Per-sample normalization: 1 / (fft_size * COLA_sum)
    norm: f32,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    // Analysis state: incoming samples accumulate here
    analysis_buf: VecDeque<f32>,
    // Synthesis state: overlap-add accumulator (length = fft_size)
    synthesis_accum: Vec<f32>,
    // Samples ready to be consumed by the caller
    output_ready: VecDeque<f32>,
    // Scratch buffers (pre-allocated, reused every frame)
    time_buf: Vec<f32>,
    freq_buf: Vec<Complex<f32>>,
    scratch_fwd: Vec<Complex<f32>>,
    scratch_inv: Vec<Complex<f32>>,
}

impl Stft {
    pub fn new(fft_size: usize, hop_size: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);

        let scratch_fwd = forward.make_scratch_vec();
        let scratch_inv = inverse.make_scratch_vec();
        let freq_buf = forward.make_output_vec();

        let window = hann_window(fft_size);
        let norm = cola_norm(&window, hop_size, fft_size);

        // Pre-fill analysis buffer so the first output frame aligns with t=0.
        // Without pre-fill, the first fft_size-hop_size input samples are silently consumed
        // before any output arrives. Pre-filling with zeros gives the expected latency of
        // exactly fft_size/2 samples (group delay of the windowed analysis).
        let mut analysis_buf = VecDeque::with_capacity(fft_size * 2);
        for _ in 0..fft_size - hop_size {
            analysis_buf.push_back(0.0f32);
        }

        Self {
            fft_size,
            hop_size,
            window,
            norm,
            forward,
            inverse,
            analysis_buf,
            synthesis_accum: vec![0.0f32; fft_size],
            output_ready: VecDeque::new(),
            time_buf: vec![0.0f32; fft_size],
            freq_buf,
            scratch_fwd,
            scratch_inv,
        }
    }

    /// Feed incoming audio samples into the analysis buffer.
    pub fn push_samples(&mut self, samples: &[f32]) {
        self.analysis_buf.extend(samples.iter().copied());
    }

    /// Returns true if enough samples are buffered to produce the next analysis frame.
    pub fn has_analysis_frame(&self) -> bool {
        self.analysis_buf.len() >= self.fft_size
    }

    /// Extract the next windowed analysis frame and return its spectrum.
    /// Advances the analysis buffer by `hop_size` samples.
    /// Returns `None` if fewer than `fft_size` samples are available.
    pub fn pop_analysis_frame(&mut self) -> Option<Vec<Complex<f32>>> {
        if self.analysis_buf.len() < self.fft_size {
            return None;
        }

        // Window and copy fft_size samples (without removing the overlap)
        for (i, &s) in self.analysis_buf.iter().take(self.fft_size).enumerate() {
            self.time_buf[i] = s * self.window[i];
        }

        // Advance by hop_size (the overlap remains in analysis_buf)
        for _ in 0..self.hop_size {
            self.analysis_buf.pop_front();
        }

        self.forward
            .process_with_scratch(
                &mut self.time_buf,
                &mut self.freq_buf,
                &mut self.scratch_fwd,
            )
            .expect("forward FFT failed");

        Some(self.freq_buf.clone())
    }

    /// Accept a processed synthesis frame, overlap-add it into the accumulator,
    /// and make `hop_size` samples available via `drain_output`.
    pub fn push_synthesis_frame(&mut self, frame: &[Complex<f32>]) {
        self.freq_buf.copy_from_slice(frame);
        // DC and Nyquist bins must be real-valued for the real IFFT.
        // Any phase modification from the vocoder/jitter must be discarded here.
        let last = self.freq_buf.len() - 1;
        self.freq_buf[0] = Complex::new(self.freq_buf[0].norm(), 0.0);
        self.freq_buf[last] = Complex::new(self.freq_buf[last].norm(), 0.0);

        self.inverse
            .process_with_scratch(
                &mut self.freq_buf,
                &mut self.time_buf,
                &mut self.scratch_inv,
            )
            .expect("inverse FFT failed");

        // Overlap-add with synthesis window and normalization
        for i in 0..self.fft_size {
            self.synthesis_accum[i] += self.time_buf[i] * self.window[i] * self.norm;
        }

        // Drain the leading hop_size samples — they are fully accumulated
        for i in 0..self.hop_size {
            self.output_ready.push_back(self.synthesis_accum[i]);
        }

        // Shift accumulator left by hop_size, clear the tail
        self.synthesis_accum.copy_within(self.hop_size.., 0);
        let tail_start = self.fft_size - self.hop_size;
        self.synthesis_accum[tail_start..].fill(0.0);
    }

    /// Return all currently available output samples.
    pub fn drain_output(&mut self) -> Vec<f32> {
        self.output_ready.drain(..).collect()
    }

    pub fn output_available(&self) -> usize {
        self.output_ready.len()
    }
}

fn hann_window(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / len as f32).cos()))
        .collect()
}

/// Compute the per-sample normalization factor for overlap-add.
///
/// For analysis window `w` and synthesis window `w` with hop `H`:
///   norm = 1 / (fft_size * Σ w[i]²)  summed at the center sample over all overlapping frames.
fn cola_norm(window: &[f32], hop_size: usize, fft_size: usize) -> f32 {
    let center = fft_size / 2;
    let num_overlaps = (fft_size + hop_size - 1) / hop_size;
    let mut sum = 0.0f32;
    for k in 0..num_overlaps {
        let start = k * hop_size;
        if center >= start && center < start + fft_size {
            let idx = center - start;
            sum += window[idx] * window[idx];
        }
    }
    if sum > 1e-10 {
        1.0 / (fft_size as f32 * sum)
    } else {
        1.0 / fft_size as f32
    }
}
