/// Audio preprocessing utilities to improve transcription quality
/// Similar to what Google Translate uses for better accuracy

/// Normalize audio to optimal range for Whisper models
/// Whisper works best with audio in range [-1.0, 1.0] with good dynamic range
pub fn normalize_audio(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }

    // Find max absolute value
    let max_abs = samples
        .iter()
        .map(|&s| s.abs())
        .fold(0.0f32, |a, b| a.max(b));

    // Avoid division by zero and don't amplify silence
    if max_abs < 0.0001 {
        // Audio is too quiet, leave as is
        return;
    }

    // Normalize to 0.95 max to avoid clipping and leave headroom
    let target_max = 0.95;
    let scale = target_max / max_abs;

    // Apply normalization
    for sample in samples.iter_mut() {
        *sample *= scale;
    }
}

/// Remove DC offset (DC bias) from audio
/// DC offset can degrade transcription quality
pub fn remove_dc_offset(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }

    // Calculate DC offset (mean value)
    let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;

    // Remove DC offset
    for sample in samples.iter_mut() {
        *sample -= mean;
    }
}

/// Simple high-pass filter to remove low-frequency noise
/// Cutoff frequency: ~80Hz (removes rumble, wind noise, etc.)
/// Uses a simple first-order IIR high-pass filter
pub fn apply_high_pass_filter(samples: &mut [f32], sample_rate: usize) {
    if samples.is_empty() {
        return;
    }

    // High-pass filter cutoff: 80Hz
    // This removes low-frequency noise that doesn't help speech recognition
    const CUTOFF_FREQ: f32 = 80.0;
    
    // Calculate filter coefficient
    let rc = 1.0 / (2.0 * std::f32::consts::PI * CUTOFF_FREQ);
    let dt = 1.0 / sample_rate as f32;
    let alpha = rc / (rc + dt);

    // Apply high-pass filter (first-order IIR)
    let mut prev_input = samples[0];
    let mut prev_output = 0.0f32;

    for sample in samples.iter_mut() {
        let current_input = *sample;
        let output = alpha * (prev_output + current_input - prev_input);
        *sample = output;
        prev_input = current_input;
        prev_output = output;
    }
}

/// Apply all preprocessing steps to improve transcription quality
/// This is similar to what professional speech recognition systems do
pub fn preprocess_audio(samples: &mut [f32], sample_rate: usize) {
    if samples.is_empty() {
        return;
    }

    // Step 1: Remove DC offset first (before other processing)
    remove_dc_offset(samples);

    // Step 2: Apply high-pass filter to remove low-frequency noise
    apply_high_pass_filter(samples, sample_rate);

    // Step 3: Normalize to optimal range for Whisper
    normalize_audio(samples);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_audio() {
        let mut samples = vec![0.1, 0.2, 0.3, 0.4];
        normalize_audio(&mut samples);
        
        // Max should be around 0.95
        let max = samples.iter().map(|&s| s.abs()).fold(0.0f32, |a, b| a.max(b));
        assert!(max > 0.9 && max <= 1.0);
    }

    #[test]
    fn test_remove_dc_offset() {
        let mut samples = vec![1.1, 1.2, 1.3, 1.4];
        remove_dc_offset(&mut samples);
        
        // Mean should be close to zero
        let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 0.0001);
    }

    #[test]
    fn test_high_pass_filter() {
        let mut samples = vec![0.0; 100];
        // Add DC component
        for s in samples.iter_mut() {
            *s = 0.5;
        }
        
        apply_high_pass_filter(&mut samples, 16000);
        
        // DC component should be reduced
        let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 0.1);
    }
}

