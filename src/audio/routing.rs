use ringbuf::traits::{Consumer, Producer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn run_dsp_loop(
    mut input_consumer: impl Consumer<Item = f32>,
    mut output_producer: impl Producer<Item = f32>,
    mut pipeline: crate::dsp::Pipeline,
    block_size: usize,
    running: Arc<AtomicBool>,
) {
    // Try to elevate thread priority for real-time audio
    let _ = thread_priority::set_current_thread_priority(
        thread_priority::ThreadPriority::Max,
    );

    let mut input_buf = vec![0.0f32; block_size];

    while running.load(Ordering::Relaxed) {
        // Try to read a full block
        let available = input_consumer.occupied_len();
        if available < block_size {
            std::thread::sleep(std::time::Duration::from_micros(500));
            continue;
        }

        // Pop block_size samples
        for i in 0..block_size {
            input_buf[i] = input_consumer.try_pop().unwrap_or(0.0);
        }

        let output = pipeline.process_block(&input_buf);

        for &sample in &output {
            let _ = output_producer.try_push(sample);
        }
    }
}
