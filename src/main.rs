mod audio;
mod config;
mod dsp;
mod platform;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "wisprnito", version, about = "Real-time voice anonymizer")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start voice anonymization daemon in the background
    Start {
        /// Input device name (substring match)
        #[arg(long)]
        input: Option<String>,
        /// Output device name (substring match)
        #[arg(long)]
        output: Option<String>,
        /// Use smaller FFT for lower latency (~20ms less)
        #[arg(long)]
        low_latency: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Show current status and session parameters
    Status,
    /// List audio devices
    Devices,
    /// Show current configuration
    Config,
    /// Remove LaunchAgent and stop the daemon
    Uninstall,
    /// Run in foreground (used internally by `start`)
    #[command(hide = true)]
    Foreground {
        #[arg(long)]
        input: Option<String>,
        #[arg(long)]
        output: Option<String>,
        #[arg(long)]
        low_latency: bool,
        #[arg(long)]
        pitch: f32,
        #[arg(long)]
        formant: f32,
        #[arg(long)]
        jitter: f32,
        /// Skip DSP — copy mic directly to output for testing routing
        #[arg(long)]
        passthrough: bool,
    },
}

fn pid_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("wisprnito")
        .join("wisprnito.pid")
}

fn session_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("wisprnito")
        .join("session.json")
}

fn log_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("wisprnito")
        .join("wisprnito.log")
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { input, output, low_latency } => cmd_start(input, output, low_latency),
        Commands::Stop => cmd_stop(),
        Commands::Status => cmd_status(),
        Commands::Devices => cmd_devices(),
        Commands::Config => cmd_config(),
        Commands::Uninstall => cmd_uninstall(),
        Commands::Foreground { input, output, low_latency, pitch, formant, jitter, passthrough } => {
            cmd_foreground(input, output, low_latency, pitch, formant, jitter, passthrough)
        }
    }
}

fn cmd_start(
    input_name: Option<String>,
    output_name: Option<String>,
    low_latency: bool,
) -> anyhow::Result<()> {
    // Check not already running
    let pid_path = pid_file_path();
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                let alive = unsafe { libc::kill(pid, 0) == 0 };
                if alive {
                    println!("Wisprnito is already running (PID {}). Run `wisprnito stop` first.", pid);
                    return Ok(());
                }
            }
        }
    }

    // Load config for defaults
    let cfg = config::Config::load()?;

    // Build anonymization params (randomized unless overridden in config)
    let random = dsp::AnonymizationParams::random();
    let params = dsp::AnonymizationParams {
        pitch_semitones: cfg.pitch_semitones.unwrap_or(random.pitch_semitones),
        formant_ratio: cfg.formant_ratio.unwrap_or(random.formant_ratio),
        phase_jitter: cfg.phase_jitter.unwrap_or(random.phase_jitter),
    };

    println!("Session parameters:");
    params.display();

    // Save session params so `status` can read them
    let session = serde_json::json!({
        "pitch_semitones": params.pitch_semitones,
        "formant_ratio": params.formant_ratio,
        "phase_jitter": params.phase_jitter,
    });
    let session_path = session_file_path();
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&session_path, serde_json::to_string_pretty(&session)?)?;

    // Prepare log file
    let log_path = log_file_path();
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log_file = std::fs::File::create(&log_path)?;

    // Resolve the current binary path
    let exe = std::env::current_exe()?;

    // Build args for the foreground subcommand
    // Use --flag=value format so negative floats aren't parsed as flags by clap
    let mut args = vec![
        "foreground".to_string(),
        format!("--pitch={}", params.pitch_semitones),
        format!("--formant={}", params.formant_ratio),
        format!("--jitter={}", params.phase_jitter),
    ];
    if let Some(ref i) = input_name.or(cfg.default_input) {
        args.push(format!("--input={}", i));
    }
    if let Some(ref o) = output_name.or(cfg.default_output) {
        args.push(format!("--output={}", o));
    }
    if low_latency {
        args.push("--low-latency".to_string());
    }

    // Spawn child process — stdout/stderr go to log file, process is detached
    let child = std::process::Command::new(&exe)
        .args(&args)
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .stdin(std::process::Stdio::null())
        .spawn()?;

    // Write PID file
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&pid_path, child.id().to_string())?;

    println!("Started in background (PID {}).", child.id());
    println!("Logs: {}", log_file_path().display());
    println!("Set BlackHole 2ch as mic input in System Settings → Sound → Input");

    Ok(())
}

fn cmd_foreground(
    input_name: Option<String>,
    output_name: Option<String>,
    low_latency: bool,
    pitch: f32,
    formant: f32,
    jitter: f32,
    passthrough: bool,
) -> anyhow::Result<()> {
    let params = dsp::AnonymizationParams {
        pitch_semitones: pitch,
        formant_ratio: formant,
        phase_jitter: jitter,
    };

    let cfg = config::Config::load().unwrap_or_default();

    let input_device = match input_name.as_deref() {
        Some(name) => audio::devices::find_device(name, true)?,
        None => match cfg.default_input.as_deref() {
            Some(name) => audio::devices::find_device(name, true)?,
            None => audio::devices::default_input()?,
        },
    };

    let output_device = match output_name.as_deref() {
        Some(name) => audio::devices::find_device(name, false)?,
        None => match cfg.default_output.as_deref() {
            Some(name) => audio::devices::find_device(name, false)?,
            None => audio::devices::find_blackhole().map_err(|_| {
                anyhow::anyhow!(
                    "BlackHole 2ch not found. Install it with: brew install --cask blackhole-2ch"
                )
            })?,
        },
    };

    use cpal::traits::DeviceTrait;
    let fft_size = if low_latency { 512 } else { 2048 };
    let block_size = fft_size / 2;

    let input_name = input_device.name().unwrap_or_default();
    let output_name = output_device.name().unwrap_or_default();
    eprintln!("Input:  {}", input_name);
    eprintln!("Output: {}", output_name);
    eprintln!("FFT size: {}", fft_size);

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc_sigterm(r);

    let (input_prod, input_cons, output_prod, output_cons) =
        audio::stream::create_ring_buffers();

    let streams = audio::stream::AudioStreams::new(
        &input_device,
        &output_device,
        input_prod,
        output_cons,
    )?;

    let sample_rate = streams.sample_rate;
    eprintln!("Sample rate: {}Hz", sample_rate);

    let dsp_running = running.clone();
    let dsp_thread = if passthrough {
        eprintln!("Mode: PASSTHROUGH (no DSP — testing routing only)");
        std::thread::spawn(move || {
            audio::routing::run_passthrough(input_cons, output_prod, dsp_running);
        })
    } else {
        let pipeline = dsp::Pipeline::new(params, sample_rate, fft_size)?;
        std::thread::spawn(move || {
            audio::routing::run_dsp_loop(input_cons, output_prod, pipeline, block_size, dsp_running);
        })
    };

    eprintln!("Running. Send SIGTERM to stop.");

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    dsp_thread.join().ok();
    let _ = std::fs::remove_file(pid_file_path());
    let _ = std::fs::remove_file(session_file_path());

    Ok(())
}

fn cmd_stop() -> anyhow::Result<()> {
    let pid_path = pid_file_path();
    if !pid_path.exists() {
        println!("Wisprnito is not running.");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse()?;

    unsafe {
        if libc::kill(pid, libc::SIGTERM) == 0 {
            println!("Stopped wisprnito (PID {}).", pid);
        } else {
            println!("Process {} not found, cleaning up.", pid);
        }
    }

    let _ = std::fs::remove_file(&pid_path);
    let _ = std::fs::remove_file(session_file_path());
    Ok(())
}

fn cmd_status() -> anyhow::Result<()> {
    let pid_path = pid_file_path();
    if !pid_path.exists() {
        println!("Wisprnito is not running.");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse()?;

    let alive = unsafe { libc::kill(pid, 0) == 0 };

    if alive {
        println!("Wisprnito is running (PID {}).", pid);
        let session_path = session_file_path();
        if session_path.exists() {
            if let Ok(data) = std::fs::read_to_string(&session_path) {
                if let Ok(session) = serde_json::from_str::<serde_json::Value>(&data) {
                    println!("Session parameters:");
                    if let Some(p) = session["pitch_semitones"].as_f64() {
                        println!("  Pitch shift:   {:.2} semitones", p);
                    }
                    if let Some(f) = session["formant_ratio"].as_f64() {
                        println!("  Formant ratio: {:.3}", f);
                    }
                    if let Some(j) = session["phase_jitter"].as_f64() {
                        println!("  Phase jitter:  {:.3}", j);
                    }
                }
            }
        }
        println!("Logs: {}", log_file_path().display());
    } else {
        println!("Wisprnito is not running (stale PID file, cleaning up).");
        let _ = std::fs::remove_file(&pid_path);
        let _ = std::fs::remove_file(session_file_path());
        println!("Hint: check logs at {}", log_file_path().display());
    }

    Ok(())
}

fn cmd_devices() -> anyhow::Result<()> {
    let devices = audio::devices::list_devices()?;

    if devices.is_empty() {
        println!("No audio devices found.");
        return Ok(());
    }

    println!("Audio devices:");
    for d in &devices {
        let direction = match (d.is_input, d.is_output) {
            (true, true) => "I/O",
            (true, false) => "IN ",
            (false, true) => "OUT",
            _ => "???",
        };
        let blackhole_marker = if d.name.contains("BlackHole") { " (BlackHole)" } else { "" };
        let rates: Vec<String> = d.sample_rates.iter().map(|r| format!("{}Hz", r)).collect();
        println!("  [{}] {}{}  [{}]", direction, d.name, blackhole_marker, rates.join(", "));
    }

    Ok(())
}

fn cmd_config() -> anyhow::Result<()> {
    let cfg = config::Config::load()?;
    let path = config::Config::path();
    println!("Config file: {}", path.display());
    println!("{}", serde_json::to_string_pretty(&cfg)?);
    Ok(())
}

fn cmd_uninstall() -> anyhow::Result<()> {
    // Stop daemon if running
    cmd_stop().ok();
    // Unload and remove LaunchAgent plist
    let plist = {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Library/LaunchAgents/com.devpolo.wisprnito.plist")
    };
    if plist.exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", plist.to_str().unwrap()])
            .status();
        std::fs::remove_file(&plist)?;
        println!("LaunchAgent removed.");
    }
    // Remove data dir
    let _ = std::fs::remove_dir_all(pid_file_path().parent().unwrap());
    println!("Uninstalled. Remove the binary with: sudo rm /usr/local/bin/wisprnito");
    Ok(())
}

fn ctrlc_sigterm(running: Arc<AtomicBool>) {
    unsafe {
        libc::signal(libc::SIGTERM, sigterm_handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, sigterm_handler as libc::sighandler_t);
    }
    RUNNING_FLAG.store(Arc::into_raw(running) as usize, Ordering::SeqCst);
}

static RUNNING_FLAG: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

extern "C" fn sigterm_handler(_: libc::c_int) {
    let ptr = RUNNING_FLAG.load(Ordering::SeqCst);
    if ptr != 0 {
        let running = unsafe { &*(ptr as *const AtomicBool) };
        running.store(false, Ordering::SeqCst);
    }
}
