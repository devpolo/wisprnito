use num_complex::Complex;
use realfft::RealFftPlanner;
use std::f32::consts::PI;

/// LPC-based formant shifter using spectral envelope manipulation.
pub struct FormantShifter {
    fft_size: usize,
    lpc_order: usize,
    // Pre-allocated buffers
    lpc_spectrum: Vec<f32>,
    shifted_spectrum: Vec<f32>,
    time_buf: Vec<f32>,
    freq_buf: Vec<Complex<f32>>,
    out_freq_buf: Vec<Complex<f32>>,
}

impl FormantShifter {
    pub fn new(fft_size: usize) -> Self {
        let freq_bins = fft_size / 2 + 1;
        Self {
            fft_size,
            lpc_order: 16,
            lpc_spectrum: vec![0.0; freq_bins],
            shifted_spectrum: vec![0.0; freq_bins],
            time_buf: vec![0.0; fft_size],
            freq_buf: vec![Complex::new(0.0, 0.0); freq_bins],
            out_freq_buf: vec![Complex::new(0.0, 0.0); freq_bins],
        }
    }

    /// Shift formants of the input signal by the given ratio.
    ///
    /// ratio > 1 shifts formants up, ratio < 1 shifts formants down.
    pub fn shift(&mut self, signal: &[f32], ratio: f32) -> Vec<f32> {
        if signal.is_empty() {
            return Vec::new();
        }

        let len = signal.len().min(self.fft_size);

        // Compute LPC coefficients of the original signal
        let lpc_coeffs = analyze_lpc(signal, self.lpc_order);

        // Compute original spectral envelope from LPC
        self.compute_lpc_envelope(&lpc_coeffs, &mut self.lpc_spectrum.clone());
        let original_envelope = self.lpc_spectrum.clone();

        // Compute shifted spectral envelope by warping frequency axis
        let freq_bins = self.fft_size / 2 + 1;
        let mut shifted_envelope = vec![0.0f32; freq_bins];
        for k in 0..freq_bins {
            // Map this output bin back to the source bin
            let src_bin = k as f32 / ratio;
            let src_idx = src_bin.floor() as usize;
            let frac = src_bin - src_idx as f32;

            if src_idx + 1 < freq_bins {
                shifted_envelope[k] =
                    original_envelope[src_idx] + frac * (original_envelope[src_idx + 1] - original_envelope[src_idx]);
            } else if src_idx < freq_bins {
                shifted_envelope[k] = original_envelope[src_idx];
            } else {
                shifted_envelope[k] = original_envelope[freq_bins - 1];
            }
        }

        // FFT the signal
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(self.fft_size);
        let inverse = planner.plan_fft_inverse(self.fft_size);

        // Zero-pad or truncate input into time buffer
        self.time_buf.fill(0.0);
        self.time_buf[..len].copy_from_slice(&signal[..len]);

        // Apply pre-emphasis window
        apply_hann(&mut self.time_buf);

        let mut scratch = forward.make_scratch_vec();
        self.freq_buf = forward.make_output_vec();

        forward
            .process_with_scratch(&mut self.time_buf, &mut self.freq_buf, &mut scratch)
            .expect("forward FFT failed");

        // Apply envelope ratio: new_envelope / old_envelope
        for k in 0..freq_bins {
            let gain = if original_envelope[k] > 1e-10 {
                shifted_envelope[k] / original_envelope[k]
            } else {
                1.0
            };
            self.freq_buf[k] *= gain;
        }

        // Inverse FFT
        let mut inv_scratch = inverse.make_scratch_vec();
        self.time_buf = vec![0.0f32; self.fft_size];
        let mut inv_freq = self.freq_buf.clone();

        inverse
            .process_with_scratch(&mut inv_freq, &mut self.time_buf, &mut inv_scratch)
            .expect("inverse FFT failed");

        // Normalize
        let norm = 1.0 / self.fft_size as f32;
        let mut output: Vec<f32> = self.time_buf[..len].iter().map(|&x| x * norm).collect();

        // Remove windowing effect at edges
        output.truncate(signal.len());
        output
    }

    /// Compute spectral envelope magnitude from LPC coefficients using FFT.
    fn compute_lpc_envelope(&self, lpc_coeffs: &[f32], envelope: &mut Vec<f32>) {
        let freq_bins = self.fft_size / 2 + 1;
        envelope.resize(freq_bins, 0.0);

        for k in 0..freq_bins {
            let freq = PI * k as f32 / (freq_bins - 1) as f32;

            // Evaluate A(z) = 1 + a1*z^-1 + a2*z^-2 + ...
            let mut real = 1.0f32;
            let mut imag = 0.0f32;
            for (i, &coeff) in lpc_coeffs.iter().enumerate() {
                let angle = freq * (i + 1) as f32;
                real += coeff * angle.cos();
                imag -= coeff * angle.sin();
            }

            let magnitude = (real * real + imag * imag).sqrt();
            // LPC envelope is 1/|A(z)|
            envelope[k] = if magnitude > 1e-10 {
                1.0 / magnitude
            } else {
                1.0
            };
        }
    }
}

/// Levinson-Durbin LPC analysis.
///
/// Returns `order` LPC coefficients from the input signal.
pub fn analyze_lpc(signal: &[f32], order: usize) -> Vec<f32> {
    let n = signal.len();
    if n == 0 || order == 0 {
        return vec![0.0; order];
    }

    // Compute autocorrelation
    let mut r = vec![0.0f32; order + 1];
    for lag in 0..=order {
        let mut sum = 0.0f32;
        for i in 0..n - lag {
            sum += signal[i] * signal[i + lag];
        }
        r[lag] = sum;
    }

    // Handle silence
    if r[0].abs() < 1e-10 {
        return vec![0.0; order];
    }

    // Levinson-Durbin recursion
    let mut a = vec![0.0f32; order];
    let mut a_prev = vec![0.0f32; order];
    let mut error = r[0];

    for i in 0..order {
        // Compute reflection coefficient
        let mut sum = 0.0f32;
        for j in 0..i {
            sum += a_prev[j] * r[i - j];
        }
        let k = -(r[i + 1] + sum) / error;

        // Update coefficients
        a[i] = k;
        for j in 0..i {
            a[j] = a_prev[j] + k * a_prev[i - 1 - j];
        }

        error *= 1.0 - k * k;
        if error <= 0.0 {
            break;
        }

        a_prev[..=i].copy_from_slice(&a[..=i]);
    }

    a
}

fn apply_hann(buf: &mut [f32]) {
    let len = buf.len();
    for i in 0..len {
        let w = 0.5 * (1.0 - (2.0 * PI * i as f32 / len as f32).cos());
        buf[i] *= w;
    }
}
