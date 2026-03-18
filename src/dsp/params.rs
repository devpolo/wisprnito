use rand::Rng;

/// Parameters controlling the voice anonymization transform.
pub struct AnonymizationParams {
    /// Pitch shift in semitones. Random in [-3,-1] union [1,3].
    pub pitch_semitones: f32,
    /// Formant envelope scaling ratio. Random in [0.88,0.96] union [1.04,1.12].
    pub formant_ratio: f32,
    /// Phase jitter magnitude (radians scale factor). Uniform in [0.02, 0.08].
    pub phase_jitter: f32,
}

impl AnonymizationParams {
    /// Generate random anonymization parameters from the valid ranges.
    pub fn random() -> Self {
        let mut rng = rand::rng();

        let pitch_semitones = if rng.random_bool(0.5) {
            rng.random_range(-3.0f32..=-1.0)
        } else {
            rng.random_range(1.0f32..=3.0)
        };

        let formant_ratio = if rng.random_bool(0.5) {
            rng.random_range(0.88f32..=0.96)
        } else {
            rng.random_range(1.04f32..=1.12)
        };

        let phase_jitter = rng.random_range(0.02f32..=0.08);

        Self {
            pitch_semitones,
            formant_ratio,
            phase_jitter,
        }
    }

    /// Print the current parameters to stdout.
    pub fn display(&self) {
        println!(
            "AnonymizationParams {{ pitch_semitones: {:.2}, formant_ratio: {:.3}, phase_jitter: {:.3} }}",
            self.pitch_semitones, self.formant_ratio, self.phase_jitter
        );
    }
}
