use super::*;

/// User-selected remote PCM format and optional local whisper.cpp engine.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct AudioSettings {
    pub sample_rate: u32,
    pub channels: u16,
    pub whisper_executable: String,
    pub whisper_model: String,
}
impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            whisper_executable: String::new(),
            whisper_model: String::new(),
        }
    }
}
impl AudioSettings {
    pub fn format(&self) -> crate::audio::AudioFormat {
        let selected = crate::audio::AudioFormat {
            sample_rate: self.sample_rate,
            channels: self.channels,
        };
        if selected.validate().is_ok() {
            selected
        } else {
            crate::audio::AudioFormat {
                sample_rate: 48_000,
                channels: 2,
            }
        }
    }
}
