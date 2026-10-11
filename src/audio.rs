//! Explicitly started microphone capture: bounded callbacks, worker resampling,
//! and 16 kHz mono signed little-endian PCM for speech tools.

use anyhow::{Context, Result, ensure};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::Resampler;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc as sync,
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};

mod transcription;
pub use transcription::transcribe_pcm;

pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}
impl AudioFormat {
    pub const SPEECH: Self = Self {
        sample_rate: SAMPLE_RATE,
        channels: 1,
    };
    pub fn validate(self) -> Result<()> {
        ensure!(
            [
                8000, 16000, 22050, 24000, 32000, 44100, 48000, 96000, 192000
            ]
            .contains(&self.sample_rate),
            "unsupported audio sample rate"
        );
        ensure!(
            [1, 2, 4, 6, 8].contains(&self.channels),
            "unsupported audio channel count"
        );
        Ok(())
    }
    pub fn packet_bytes(self) -> usize {
        self.sample_rate as usize / 50 * self.channels as usize * 2
    }
}
pub const PCM_QUEUE_PACKETS: usize = 8;
const NATIVE_FRAMES: usize = 1024;

struct NativeBlock {
    samples: [f32; NATIVE_FRAMES * 8],
    len: usize,
}
struct CaptureGuard(Arc<AtomicBool>);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// User intent must precede this call. Cancellation also stops the device when
/// the caller's future is dropped; no stream handle crosses the UI boundary.
pub async fn capture(
    mut stop: watch::Receiver<bool>,
    format: AudioFormat,
    pcm: mpsc::Sender<Vec<u8>>,
    ready: oneshot::Sender<Result<()>>,
) -> Result<()> {
    format.validate()?;
    if *stop.borrow() {
        return Ok(());
    }
    tokio::select! {
        result = request_permission() => { result?; }
        _ = wait_for_stop(&mut stop) => { return Ok(()); }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let guard = CaptureGuard(cancelled.clone());
    let task = tokio::task::spawn_blocking(move || capture_worker(cancelled, format, pcm, ready));
    tokio::pin!(task);
    let result = tokio::select! {
        result = &mut task => result.context("microphone worker failed")?,
        _ = wait_for_stop(&mut stop) => {
            drop(guard);
            task.await.context("microphone shutdown failed")?
        }
    };
    result
}

pub async fn wait_for_stop(stop: &mut watch::Receiver<bool>) {
    while !*stop.borrow_and_update() {
        if stop.changed().await.is_err() {
            break;
        }
    }
}

fn capture_worker(
    cancel: Arc<AtomicBool>,
    format: AudioFormat,
    pcm: mpsc::Sender<Vec<u8>>,
    ready: oneshot::Sender<Result<()>>,
) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    let started = (|| {
        let device = cpal::default_host()
            .default_input_device()
            .context("no microphone is available")?;
        let default = device
            .default_input_config()
            .context("cannot read microphone configuration")?;
        let config = match device.supported_input_configs() {
            Ok(mut configurations) => configurations
                .find(|config| {
                    config.channels() == format.channels
                        && config.min_sample_rate() <= format.sample_rate
                        && config.max_sample_rate() >= format.sample_rate
                })
                .map(|config| config.with_sample_rate(format.sample_rate))
                .unwrap_or(default),
            Err(error) => {
                tracing::debug!(%error, "cannot enumerate microphone formats; using default input");
                default
            }
        };
        ensure!(
            (8_000..=192_000).contains(&config.sample_rate())
                && (1..=32).contains(&config.channels()),
            "unsupported microphone configuration"
        );
        let (native_tx, native_rx) = sync::sync_channel(8);
        let failed = Arc::new(AtomicBool::new(false));
        let stream_config = config.clone().into();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => input_stream::<f32>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::I16 => input_stream::<i16>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::U16 => input_stream::<u16>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::I32 => input_stream::<i32>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::I8 => input_stream::<i8>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::I24 => input_stream::<cpal::I24>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::I64 => input_stream::<i64>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::U8 => input_stream::<u8>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::U24 => input_stream::<cpal::U24>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::U32 => input_stream::<u32>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::U64 => input_stream::<u64>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            cpal::SampleFormat::F64 => input_stream::<f64>(
                &device,
                &stream_config,
                format.channels,
                native_tx,
                failed.clone(),
            )?,
            format => anyhow::bail!("unsupported microphone sample format: {format}"),
        };
        let converter = PcmConverter::new(config.sample_rate(), format)?;
        if cancel.load(Ordering::Acquire) {
            return Ok((stream, native_rx, failed, converter));
        }
        stream
            .play()
            .context("cannot start microphone (check system microphone permission)")?;
        Ok((stream, native_rx, failed, converter))
    })();
    let (stream, native_rx, failed, mut converter) = match started {
        Ok(started) => {
            if ready.send(Ok(())).is_err() {
                return Ok(());
            }
            started
        }
        Err(error) => {
            let message = error.to_string();
            if ready.send(Err(anyhow::anyhow!(message))).is_err() {
                tracing::debug!("microphone requester cancelled before startup");
            }
            return Err(error);
        }
    };
    while !cancel.load(Ordering::Acquire) {
        ensure!(
            !failed.load(Ordering::Acquire),
            "microphone stopped: device error or audio queue overflow"
        );
        match native_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(block) => converter.push(&block.samples[..block.len], &pcm)?,
            Err(sync::RecvTimeoutError::Timeout) => {}
            Err(sync::RecvTimeoutError::Disconnected) => anyhow::bail!("microphone stream ended"),
        }
    }
    drop(stream);
    while let Ok(block) = native_rx.try_recv() {
        converter.push(&block.samples[..block.len], &pcm)?;
    }
    if !pcm.is_closed() {
        converter.finish(&pcm)?;
    }
    Ok(())
}

fn input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    output_channels: u16,
    tx: sync::SyncSender<NativeBlock>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = usize::from(config.channels);
    let output_channels = usize::from(output_channels);
    let callback_failed = failed.clone();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                // No heap allocation, locks, filesystem, logs, or waits in the device callback.
                for chunk in data.chunks(NATIVE_FRAMES * channels) {
                    let mut block = NativeBlock {
                        samples: [0.0; NATIVE_FRAMES * 8],
                        len: 0,
                    };
                    for frame in chunk.chunks_exact(channels) {
                        for channel in 0..output_channels {
                            let sample = if output_channels == 1 {
                                frame
                                    .iter()
                                    .map(|s| <f32 as cpal::FromSample<T>>::from_sample_(*s))
                                    .sum::<f32>()
                                    / channels as f32
                            } else {
                                let sample =
                                    frame.get(channel).unwrap_or(&frame[channel % channels]);
                                <f32 as cpal::FromSample<T>>::from_sample_(*sample)
                            };
                            block.samples[block.len] = if sample.is_finite() {
                                sample.clamp(-1.0, 1.0)
                            } else {
                                0.0
                            };
                            block.len += 1;
                        }
                    }
                    if tx.try_send(block).is_err() {
                        callback_failed.store(true, Ordering::Release);
                        break;
                    }
                }
            },
            move |_| {
                failed.store(true, Ordering::Release);
            },
            None,
        )
        .context("cannot open microphone")
}

struct PcmConverter {
    resampler: rubato::FftFixedInOut<f32>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    filled: usize,
    packet: Vec<u8>,
    format: AudioFormat,
    input_rate: u32,
    input_frames: usize,
    output_frames: usize,
    skip: usize,
}
impl PcmConverter {
    fn new(rate: u32, format: AudioFormat) -> Result<Self> {
        let resampler = rubato::FftFixedInOut::new(
            rate as usize,
            format.sample_rate as usize,
            960,
            format.channels as usize,
        )?;
        let input = resampler.input_buffer_allocate(true);
        let output = resampler.output_buffer_allocate(true);
        let skip = resampler.output_delay();
        Ok(Self {
            resampler,
            input,
            output,
            filled: 0,
            packet: Vec::with_capacity(format.packet_bytes()),
            format,
            input_rate: rate,
            input_frames: 0,
            output_frames: 0,
            skip,
        })
    }
    fn push(&mut self, samples: &[f32], pcm: &mpsc::Sender<Vec<u8>>) -> Result<()> {
        for frame in samples.chunks_exact(self.format.channels as usize) {
            for (channel, sample) in frame.iter().enumerate() {
                self.input[channel][self.filled] = *sample;
            }
            self.filled += 1;
            self.input_frames += 1;
            if self.filled == self.resampler.input_frames_next() {
                self.process(pcm, None)?;
            }
        }
        Ok(())
    }
    fn process(&mut self, pcm: &mpsc::Sender<Vec<u8>>, limit: Option<usize>) -> Result<()> {
        let (_, produced) =
            self.resampler
                .process_into_buffer(&self.input, &mut self.output, None)?;
        self.filled = 0;
        for frame in 0..produced {
            if self.skip > 0 {
                self.skip -= 1;
                continue;
            }
            if limit.is_some_and(|limit| self.output_frames >= limit) {
                break;
            }
            for channel in 0..self.format.channels as usize {
                let sample = self.output[channel][frame];
                let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
                self.packet.extend_from_slice(&value.to_le_bytes());
            }
            self.output_frames += 1;
            if self.packet.len() == self.format.packet_bytes() {
                self.send_packet(pcm)?;
            }
        }
        Ok(())
    }
    fn send_packet(&mut self, pcm: &mpsc::Sender<Vec<u8>>) -> Result<()> {
        let packet = std::mem::replace(
            &mut self.packet,
            Vec::with_capacity(self.format.packet_bytes()),
        );
        pcm.try_send(packet)
            .context("audio receiver is too slow or stopped")?;
        Ok(())
    }
    fn finish(&mut self, pcm: &mpsc::Sender<Vec<u8>>) -> Result<()> {
        let target =
            self.input_frames * self.format.sample_rate as usize / self.input_rate as usize;
        for channel in &mut self.input {
            channel[self.filled..].fill(0.0);
        }
        while self.output_frames < target {
            self.process(pcm, Some(target))?;
            for channel in &mut self.input {
                channel.fill(0.0);
            }
        }
        if !self.packet.is_empty() {
            self.packet.resize(self.format.packet_bytes(), 0);
            self.send_packet(pcm)?;
        }
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
async fn request_permission() -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
async fn request_permission() -> Result<()> {
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    let (tx, rx) = oneshot::channel();
    {
        let sender = std::sync::Mutex::new(Some(tx));
        let completion = block2::RcBlock::new(move |granted: Bool| {
            if let Ok(mut sender) = sender.lock()
                && let Some(sender) = sender.take()
                && sender.send(granted.as_bool()).is_err()
            {
                tracing::debug!("microphone permission requester cancelled");
            }
        });
        // SAFETY: AVMediaTypeAudio is Apple's process-lifetime audio constant;
        // AVFoundation copies the owned block until its single completion call.
        unsafe {
            let media_type = AVMediaTypeAudio.context("macOS audio media type unavailable")?;
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &completion);
        }
    }
    ensure!(
        rx.await.context("microphone permission request failed")?,
        "microphone access denied; enable AxSSH in system Privacy & Security → Microphone"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resampling_is_bounded_and_pcm_is_little_endian() {
        for rate in [16_000, 44_100, 48_000] {
            let (tx, mut rx) = mpsc::channel(128);
            let mut converter = PcmConverter::new(rate, AudioFormat::SPEECH).unwrap();
            converter.push(&vec![0.25; rate as usize], &tx).unwrap();
            converter.finish(&tx).unwrap();
            let mut bytes = Vec::new();
            while let Ok(packet) = rx.try_recv() {
                assert_eq!(packet.len(), AudioFormat::SPEECH.packet_bytes());
                bytes.extend(packet);
            }
            assert_eq!(bytes.len(), 32_000, "input rate: {rate}");
            let sample = i16::from_le_bytes([bytes[bytes.len() / 2], bytes[bytes.len() / 2 + 1]]);
            assert!((sample - 8192).abs() < 10);
        }
    }
    #[test]
    fn slow_consumers_stop_instead_of_accumulating_audio() {
        let (tx, _rx) = mpsc::channel(1);
        assert!(
            PcmConverter::new(48_000, AudioFormat::SPEECH)
                .unwrap()
                .push(&vec![0.0; 48_000], &tx)
                .is_err()
        );
    }
    #[tokio::test]
    async fn cancellation_before_start_does_not_open_a_device() {
        let (_stop, receiver) = watch::channel(true);
        let (tx, _rx) = mpsc::channel(8);
        let (ready, _) = oneshot::channel();
        // Check the preflight stop before permission/device access.
        capture(receiver, AudioFormat::SPEECH, tx, ready)
            .await
            .unwrap();
    }
    #[test]
    fn every_selected_rate_and_channel_count_preserves_channel_order() {
        for rate in [
            8000, 16000, 22050, 24000, 32000, 44100, 48000, 96000, 192000,
        ] {
            for channels in [1, 2, 4, 6, 8] {
                let format = AudioFormat {
                    sample_rate: rate,
                    channels,
                };
                format.validate().unwrap();
                let (tx, mut rx) = mpsc::channel(128);
                let samples: Vec<f32> = (0..48_000)
                    .flat_map(|_| (0..channels).map(|channel| 0.1 + f32::from(channel) * 0.05))
                    .collect();
                PcmConverter::new(48_000, format)
                    .unwrap()
                    .push(&samples, &tx)
                    .unwrap();
                let mut last = None;
                while let Ok(packet) = rx.try_recv() {
                    assert_eq!(packet.len(), format.packet_bytes());
                    last = Some(packet);
                }
                let last = last.expect("resampler should produce PCM");
                let tail = &last[last.len() - channels as usize * 2..];
                for (channel, bytes) in tail.chunks_exact(2).enumerate() {
                    let sample = i16::from_le_bytes([bytes[0], bytes[1]]);
                    let expected = ((0.1 + channel as f32 * 0.05) * 32767.0).round() as i16;
                    assert!(
                        (sample - expected).abs() < 10,
                        "{rate} Hz/{channels} channels"
                    );
                }
            }
        }
        assert!(
            AudioFormat {
                sample_rate: 0,
                channels: 2
            }
            .validate()
            .is_err()
        );
        assert!(
            AudioFormat {
                sample_rate: 48000,
                channels: 0
            }
            .validate()
            .is_err()
        );
    }
}
