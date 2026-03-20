use anyhow::Result;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::StreamConfig;
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;

pub struct AudioStreams {
    _input_stream: cpal::Stream,
    _output_stream: cpal::Stream,
    pub sample_rate: u32,
}

impl AudioStreams {
    pub fn new(
        input_device: &cpal::Device,
        output_device: &cpal::Device,
        mut input_producer: impl Producer<Item = f32> + Send + 'static,
        mut output_consumer: impl Consumer<Item = f32> + Send + 'static,
    ) -> Result<Self> {
        // Warn if the input device is BlackHole (self-loop, no physical mic)
        if let Ok(name) = input_device.name() {
            if name.contains("BlackHole") {
                eprintln!(
                    "WARNING: Input device is '{}'. This will cause a self-loop with no physical mic.\n\
                     Set your real mic as the system input (System Settings → Sound → Input),\n\
                     or pass --input <device> explicitly.",
                    name
                );
            }
        }

        // Input: always open as mono at the device's native sample rate
        let input_config = input_device.default_input_config()?;
        let sample_rate = input_config.sample_rate().0;

        let input_stream_config = StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let input_stream = input_device.build_input_stream(
            &input_stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                for &sample in data {
                    let _ = input_producer.try_push(sample);
                }
            },
            |err| eprintln!("Input stream error: {}", err),
            None,
        )?;

        // Output: use the output device's native channel count.
        // BlackHole 2ch is stereo — we duplicate mono to both L and R.
        let output_config = output_device.default_output_config()?;
        let out_channels = output_config.channels() as usize;

        let output_stream_config = StreamConfig {
            channels: out_channels as u16,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        eprintln!(
            "Output channels: {} (mono DSP signal duplicated to all channels)",
            out_channels
        );

        let output_stream = output_device.build_output_stream(
            &output_stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // data is interleaved: [L, R, L, R, ...] for stereo
                let mut i = 0;
                while i + out_channels <= data.len() {
                    let sample = output_consumer.try_pop().unwrap_or(0.0);
                    for ch in 0..out_channels {
                        data[i + ch] = sample;
                    }
                    i += out_channels;
                }
            },
            |err| eprintln!("Output stream error: {}", err),
            None,
        )?;

        input_stream.play()?;
        output_stream.play()?;

        Ok(AudioStreams {
            _input_stream: input_stream,
            _output_stream: output_stream,
            sample_rate,
        })
    }
}

/// Create the two ring buffer pairs used for audio I/O.
/// Returns (input_producer, input_consumer, output_producer, output_consumer).
pub fn create_ring_buffers() -> (
    impl Producer<Item = f32>,
    impl Consumer<Item = f32>,
    impl Producer<Item = f32>,
    impl Consumer<Item = f32>,
) {
    let capacity = 4096 * 4;
    let input_rb = HeapRb::<f32>::new(capacity);
    let (input_prod, input_cons) = input_rb.split();
    let output_rb = HeapRb::<f32>::new(capacity);
    let (output_prod, output_cons) = output_rb.split();
    (input_prod, input_cons, output_prod, output_cons)
}
