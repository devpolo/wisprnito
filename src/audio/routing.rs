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
    let _ = thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Max);

    let mut input_buf = vec![0.0f32; block_size];
    let mut total_blocks: u64 = 0;

    while running.load(Ordering::Relaxed) {
        let available = input_consumer.occupied_len();
        if available < block_size {
            std::thread::sleep(std::time::Duration::from_micros(500));
            continue;
        }

        for i in 0..block_size {
            input_buf[i] = input_consumer.try_pop().unwrap_or(0.0);
        }

        // Periodically log that samples are flowing
        total_blocks += 1;
        if total_blocks == 1 || total_blocks % 500 == 0 {
            let peak = input_buf.iter().cloned().fold(0.0f32, f32::max);
            eprintln!("DSP block #{}: input peak = {:.4}", total_blocks, peak);
        }

        let output = pipeline.process_block(&input_buf);

        for &sample in &output {
            let _ = output_producer.try_push(sample);
        }
    }
}

pub fn run_passthrough(
    mut input_consumer: impl Consumer<Item = f32>,
    mut output_producer: impl Producer<Item = f32>,
    running: Arc<AtomicBool>,
) {
    let _ = thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Max);

    let block_size = 512;
    let mut total_blocks: u64 = 0;

    while running.load(Ordering::Relaxed) {
        let available = input_consumer.occupied_len();
        if available < block_size {
            std::thread::sleep(std::time::Duration::from_micros(500));
            continue;
        }

        total_blocks += 1;
        let mut peak = 0.0f32;
        for _ in 0..block_size {
            let s = input_consumer.try_pop().unwrap_or(0.0);
            peak = peak.max(s.abs());
            let _ = output_producer.try_push(s);
        }

        if total_blocks == 1 || total_blocks % 200 == 0 {
            eprintln!("Passthrough block #{}: peak = {:.4} {}", total_blocks, peak,
                if peak > 0.01 { "<<< VOICE DETECTED" } else { "(silence)" });
        }
    }
}
