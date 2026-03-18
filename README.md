# Wisprnito

Real-time voice anonymizer. Intercepts your microphone, transforms pitch, formants, and phase to defeat voice fingerprinting, then exposes the result as a virtual microphone (BlackHole 2ch on macOS).

Speech stays intelligible. Your voice becomes unrecognizable to speaker-ID models.

---

## Requirements

- macOS (primary target)
- [BlackHole 2ch](https://existential.audio/blackhole/) virtual audio driver
- Rust (for building from source)

Check if BlackHole is already installed:

```bash
ls /Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver
```

If missing, install it:

```bash
brew install --cask blackhole-2ch
```

---

## Build

```bash
cargo build --release
```

The binary ends up at `./target/release/wisprnito`.

Optionally, copy it to your PATH:

```bash
cp target/release/wisprnito /usr/local/bin/wisprnito
```

---

## Testing

### 1. Verify your devices

```bash
wisprnito devices
```

Expected output (exact names will vary):

```
Audio devices:
  [I/O] BlackHole 2ch (BlackHole)  [44100Hz, 48000Hz]
  [IN ] MacBook Pro Microphone     [44100Hz, 48000Hz]
  [OUT] MacBook Pro Speakers       [44100Hz, 48000Hz]
```

BlackHole 2ch must appear. If it doesn't, reinstall the driver and restart your machine.

---

### 2. Start the daemon

```bash
wisprnito start
```

Output:

```
Session parameters:
AnonymizationParams { pitch_semitones: -2.14, formant_ratio: 1.07, phase_jitter: 0.045 }
Forking to background...
```

Parameters are randomized each session — this is intentional. A different transform every run makes voice fingerprinting harder across sessions.

**Low-latency mode** (512-pt FFT instead of 2048-pt, ~20ms less latency):

```bash
wisprnito start --low-latency
```

**Specify devices explicitly** (useful if you have multiple mics or audio interfaces):

```bash
wisprnito start --input "Bose" --output "BlackHole"
```

Device names are substring-matched, case-insensitive.

---

### 3. Set BlackHole 2ch as your mic input

Open **System Settings → Sound → Input** and select **BlackHole 2ch**.

Any app that reads your microphone will now receive the anonymized audio.

> After stopping wisprnito, remember to switch your input back to your real mic.

---

### 4. Verify it's running

```bash
wisprnito status
```

```
Wisprnito is running (PID 12345).
Session parameters:
  Pitch shift: -2.1 semitones
  Formant ratio: 1.07
  Phase jitter: 0.045
```

---

### 5. Do a quick recording test

Open **QuickTime Player → File → New Audio Recording**.

Click the dropdown arrow next to the record button and select **BlackHole 2ch** as the source.

Record a few seconds of speech. Play it back. Your voice should be:
- Audibly shifted in pitch
- Formant structure altered (vowels sound different)
- Still fully intelligible

Compare it to a recording made directly from your real mic to hear the difference.

Alternatively, use `sox` from the command line:

```bash
brew install sox
rec -r 48000 -c 1 test.wav trim 0 5   # Records 5s from default input
```

With BlackHole 2ch selected as your system input, `rec` will capture the anonymized audio.

---

### 6. Stop the daemon

```bash
wisprnito stop
```

```
Sent stop signal to wisprnito (PID 12345).
```

Switch your system mic input back to your real microphone in System Settings.

---

## Configuration

The config file lives at `~/.config/wisprnito/config.json`.

```bash
wisprnito config
```

To pin specific transform parameters instead of randomizing each session, edit the file:

```json
{
  "default_input": null,
  "default_output": null,
  "pitch_semitones": -2.0,
  "formant_ratio": 1.08,
  "phase_jitter": 0.05
}
```

Set a field to `null` to keep it random. Set `default_input`/`default_output` to avoid passing `--input`/`--output` every time.

---

## How it works

```
Physical mic → cpal input stream → ring buffer → DSP thread → ring buffer → cpal output → BlackHole 2ch
                                                                                                  ↓
                                                                              Apps use BlackHole as mic input
```

DSP pipeline per audio block:

1. **STFT** — 2048-point FFT, 512-sample hop, Hann window
2. **Phase vocoder** — time-stretch by `alpha = 2^(semitones/12)` with phase coherence
3. **Phase jitter** — per-bin phase randomization to break vocoder fingerprinting
4. **ISTFT** — overlap-add synthesis
5. **Resample by `1/alpha`** — net effect is pitch-shifted audio at original length
6. **Formant shift** — 16-pole LPC envelope estimated and warped independently of pitch

End-to-end latency: ~60–100ms (imperceptible in conversation).

---

## Troubleshooting

**`wisprnito start` fails with "BlackHole not found"**

Run `wisprnito devices` — if BlackHole 2ch doesn't appear, reinstall the driver and log out/in.

**No audio comes through in Zoom/Discord**

- Confirm BlackHole 2ch is selected as the input in System Settings → Sound → Input, not just in the app's own settings.
- Run `wisprnito status` to confirm the daemon is alive.
- Check Activity Monitor for a `wisprnito` process.

**Binary blocked by Gatekeeper** (if installed from a release, not built from source)

```bash
xattr -d com.apple.quarantine /usr/local/bin/wisprnito
```

**Bose/Bluetooth headset as mic source**

Bluetooth SCO runs at 16kHz. The pipeline resamples to 48kHz internally before processing — no action needed. Just pass `--input "Bose"` (or whatever substring matches the device name in `wisprnito devices`).

**Stale PID file after crash**

```bash
rm ~/.local/share/wisprnito/wisprnito.pid
```

---

## CLI reference

```
wisprnito start [--input <device>] [--output <device>] [--low-latency]
wisprnito stop
wisprnito status
wisprnito devices
wisprnito config
```
