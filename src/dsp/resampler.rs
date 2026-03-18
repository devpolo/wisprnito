use anyhow::Result;
use rubato::{
    Resampler as RubatoResampler, SincFixedIn, SincInterpolationParameters,
    SincInterpolationType, WindowFunction,
};

/// Thin wrapper around rubato for high-quality sample rate conversion.
pub struct Resampler {
    inner: SincFixedIn<f32>,
    channels: usize,
}

impl Resampler {
    /// Create a new resampler converting from `from_rate` to `to_rate`.
    pub fn new(from_rate: u32, to_rate: u32, channels: usize) -> Result<Self> {
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            oversampling_factor: 128,
            interpolation: SincInterpolationType::Cubic,
            window: WindowFunction::BlackmanHarris2,
        };

        let ratio = to_rate as f64 / from_rate as f64;
        // chunk_size: number of input frames per processing call
        let chunk_size = 1024;

        let inner = SincFixedIn::new(ratio, 2.0, params, chunk_size, channels)?;

        Ok(Self { inner, channels })
    }

    /// Resample a mono (or interleaved) f32 buffer.
    ///
    /// For mono signals, just pass the flat sample buffer.
    /// Returns the resampled output samples.
    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }

        let frames_per_chunk = self.inner.input_frames_max();
        let samples_per_chunk = frames_per_chunk * self.channels;

        let mut output = Vec::new();

        // Process in chunks
        let mut offset = 0;
        while offset < input.len() {
            let end = (offset + samples_per_chunk).min(input.len());
            let chunk = &input[offset..end];

            // De-interleave into per-channel vectors
            let frames_in_chunk = chunk.len() / self.channels;
            let mut channel_bufs: Vec<Vec<f32>> = (0..self.channels)
                .map(|ch| {
                    (0..frames_in_chunk)
                        .map(|f| {
                            let idx = f * self.channels + ch;
                            if idx < chunk.len() {
                                chunk[idx]
                            } else {
                                0.0
                            }
                        })
                        .collect()
                })
                .collect();

            // Pad to required chunk size if this is the last (partial) chunk
            let needed = self.inner.input_frames_next();
            for ch_buf in &mut channel_bufs {
                if ch_buf.len() < needed {
                    ch_buf.resize(needed, 0.0);
                }
            }

            let refs: Vec<&[f32]> = channel_bufs.iter().map(|v| v.as_slice()).collect();
            let resampled = self.inner.process(&refs, None)?;

            // Re-interleave
            if !resampled.is_empty() {
                let out_frames = resampled[0].len();
                for f in 0..out_frames {
                    for ch in 0..self.channels {
                        if f < resampled[ch].len() {
                            output.push(resampled[ch][f]);
                        }
                    }
                }
            }

            offset += samples_per_chunk;
        }

        Ok(output)
    }
}
