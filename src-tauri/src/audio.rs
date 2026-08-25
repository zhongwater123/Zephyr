use crate::incident::model::IncidentEvent;
use crate::incident::IncidentSink;
use crate::provider::{AudioChunk, AudioStreamInfo};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{mpsc::error::TrySendError, mpsc::Sender, Notify};

const PCM_ENCODING: &str = "pcm_s16le";
const TARGET_SAMPLE_RATE: u32 = 16_000;
const TARGET_CHANNELS: u16 = 1;

#[derive(Clone)]
pub struct IncidentAudioTap {
    sink: Arc<dyn IncidentSink>,
    attempt_id: Arc<str>,
    sequence: Arc<AtomicU64>,
}

impl IncidentAudioTap {
    pub fn new(sink: Arc<dyn IncidentSink>, attempt_id: String) -> Self {
        Self {
            sink,
            attempt_id: attempt_id.into(),
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    fn emit(&self, chunk: &AudioChunk) {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let _ = self.sink.try_emit(IncidentEvent::AudioChunk {
            attempt_id: self.attempt_id.clone(),
            sequence,
            bytes: chunk.bytes.clone(),
            duration_ms: chunk.duration_ms,
            is_final: chunk.is_final,
        });
    }
}
const DEFAULT_CHUNK_MS: u16 = 200;
const PREROLL_MS: u16 = 300;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no default microphone is available")]
    NoInputDevice,
    #[error("failed to read default microphone config: {0}")]
    DefaultConfig(String),
    #[error("failed to create audio input stream: {0}")]
    BuildStream(String),
    #[error("failed to start audio input stream: {0}")]
    PlayStream(String),
    #[error("recording has not started")]
    NotRecording,
    #[error("WAV encoding failed: {0}")]
    Encode(String),
    #[error("audio queue overflowed")]
    QueueOverflow,
}

#[derive(Debug, Default)]
pub struct AudioQueueMonitor {
    packets: AtomicU64,
    high_watermark: AtomicUsize,
    overflow: AtomicBool,
    notify: Notify,
}

#[derive(Debug, Clone, Copy)]
pub struct AudioQueueSnapshot {
    pub packets: u64,
    pub high_watermark: usize,
    pub overflow: bool,
}

impl AudioQueueMonitor {
    fn record_sent(&self, sender: &Sender<AudioChunk>) {
        self.packets.fetch_add(1, Ordering::Relaxed);
        let used = sender.max_capacity().saturating_sub(sender.capacity());
        self.high_watermark.fetch_max(used, Ordering::Relaxed);
    }

    fn record_overflow(&self) {
        if !self.overflow.swap(true, Ordering::AcqRel) {
            self.notify.notify_waiters();
        }
    }

    pub async fn overflowed(&self) {
        let notified = self.notify.notified();
        if self.overflow.load(Ordering::Acquire) {
            return;
        }
        notified.await;
    }

    pub fn snapshot(&self) -> AudioQueueSnapshot {
        AudioQueueSnapshot {
            packets: self.packets.load(Ordering::Relaxed),
            high_watermark: self.high_watermark.load(Ordering::Relaxed),
            overflow: self.overflow.load(Ordering::Acquire),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AudioBuffer {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration: Duration,
}

pub struct Recorder {
    stream: Option<Stream>,
    capture: Arc<Mutex<CaptureBuffer>>,
    chunk_sender: Option<Sender<AudioChunk>>,
    queue_monitor: Option<Arc<AudioQueueMonitor>>,
    incident_tap: Option<IncidentAudioTap>,
    stream_info: AudioStreamInfo,
    samples_per_chunk: usize,
    started_at: Option<Instant>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            stream: None,
            capture: Arc::new(Mutex::new(CaptureBuffer::new(TARGET_SAMPLE_RATE))),
            chunk_sender: None,
            queue_monitor: None,
            incident_tap: None,
            stream_info: AudioStreamInfo {
                sample_rate: TARGET_SAMPLE_RATE,
                channels: TARGET_CHANNELS,
                encoding: PCM_ENCODING,
                chunk_duration_ms: DEFAULT_CHUNK_MS,
            },
            samples_per_chunk: samples_per_chunk(
                TARGET_SAMPLE_RATE,
                TARGET_CHANNELS,
                DEFAULT_CHUNK_MS,
            ),
            started_at: None,
        }
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn warm_up(&mut self) -> Result<(), AudioError> {
        self.ensure_stream(DEFAULT_CHUNK_MS)
    }

    pub fn start_streaming(
        &mut self,
        chunk_duration_ms: u16,
        chunk_sender: Sender<AudioChunk>,
        queue_monitor: Arc<AudioQueueMonitor>,
        incident_tap: Option<IncidentAudioTap>,
    ) -> Result<AudioStreamInfo, AudioError> {
        self.ensure_stream(chunk_duration_ms)?;

        let stream_info = AudioStreamInfo {
            sample_rate: TARGET_SAMPLE_RATE,
            channels: TARGET_CHANNELS,
            encoding: PCM_ENCODING,
            chunk_duration_ms,
        };
        let samples_per_chunk = samples_per_chunk(
            stream_info.sample_rate,
            stream_info.channels,
            chunk_duration_ms,
        );

        if let Ok(mut capture) = self.capture.lock() {
            capture.start_recording(
                chunk_sender.clone(),
                queue_monitor.clone(),
                samples_per_chunk,
                chunk_duration_ms,
                incident_tap.clone(),
            );
        }

        self.chunk_sender = Some(chunk_sender);
        self.queue_monitor = Some(queue_monitor);
        self.stream_info = stream_info.clone();
        self.samples_per_chunk = samples_per_chunk;
        self.incident_tap = incident_tap;
        self.started_at = Some(Instant::now());
        Ok(stream_info)
    }

    fn ensure_stream(&mut self, chunk_duration_ms: u16) -> Result<(), AudioError> {
        if self.stream.is_some() {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioError::NoInputDevice)?;
        let supported_config = device
            .default_input_config()
            .map_err(|error| AudioError::DefaultConfig(error.to_string()))?;
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        let input_sample_rate = config.sample_rate;
        let input_channels = config.channels as usize;
        let capture = Arc::new(Mutex::new(CaptureBuffer::new(input_sample_rate)));

        let stream = match sample_format {
            SampleFormat::F32 => {
                let capture = Arc::clone(&capture);
                device.build_input_stream(
                    config,
                    move |data: &[f32], _| push_f32(data, input_channels, &capture),
                    log_stream_error,
                    None,
                )
            }
            SampleFormat::I16 => {
                let capture = Arc::clone(&capture);
                device.build_input_stream(
                    config,
                    move |data: &[i16], _| push_i16(data, input_channels, &capture),
                    log_stream_error,
                    None,
                )
            }
            SampleFormat::U16 => {
                let capture = Arc::clone(&capture);
                device.build_input_stream(
                    config,
                    move |data: &[u16], _| push_u16(data, input_channels, &capture),
                    log_stream_error,
                    None,
                )
            }
            other => {
                return Err(AudioError::BuildStream(format!(
                    "unsupported sample format: {other:?}"
                )))
            }
        }
        .map_err(|error| AudioError::BuildStream(error.to_string()))?;

        stream
            .play()
            .map_err(|error| AudioError::PlayStream(error.to_string()))?;

        self.stream = Some(stream);
        self.capture = capture;
        self.stream_info.chunk_duration_ms = chunk_duration_ms;
        self.samples_per_chunk =
            samples_per_chunk(TARGET_SAMPLE_RATE, TARGET_CHANNELS, chunk_duration_ms);
        Ok(())
    }

    pub fn stop_streaming(&mut self) -> Result<Duration, AudioError> {
        let started_at = self.started_at.take().ok_or(AudioError::NotRecording)?;
        let duration = started_at.elapsed();

        if let Some(sender) = self.chunk_sender.take() {
            let final_samples = self
                .capture
                .lock()
                .map(|mut capture| capture.stop_recording())
                .unwrap_or_default();

            let chunk = AudioChunk {
                bytes: samples_to_le_bytes(&final_samples).into(),
                duration_ms: estimate_duration_ms(
                    final_samples.len(),
                    self.stream_info.sample_rate,
                    self.stream_info.channels,
                ),
                is_final: true,
            };
            if let Some(tap) = &self.incident_tap {
                tap.emit(&chunk);
            }
            match sender.try_send(chunk) {
                Ok(()) => {
                    if let Some(monitor) = self.queue_monitor.take() {
                        monitor.record_sent(&sender);
                    }
                }
                Err(TrySendError::Full(_)) => {
                    if let Some(monitor) = self.queue_monitor.take() {
                        monitor.record_overflow();
                    }
                    return Err(AudioError::QueueOverflow);
                }
                Err(TrySendError::Closed(_)) => {
                    self.queue_monitor.take();
                }
            }
        }

        self.incident_tap = None;
        Ok(duration)
    }
}

#[allow(dead_code)]
pub fn encode_wav(buffer: &AudioBuffer) -> Result<Vec<u8>, AudioError> {
    let spec = hound::WavSpec {
        channels: buffer.channels,
        sample_rate: buffer.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut cursor, spec)
        .map_err(|error| AudioError::Encode(error.to_string()))?;
    for sample in &buffer.samples {
        writer
            .write_sample(*sample)
            .map_err(|error| AudioError::Encode(error.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|error| AudioError::Encode(error.to_string()))?;
    Ok(cursor.into_inner())
}

#[allow(dead_code)]
pub fn split_pcm_into_chunks(buffer: &AudioBuffer, chunk_duration_ms: u16) -> Vec<AudioChunk> {
    if buffer.samples.is_empty() || chunk_duration_ms == 0 {
        return Vec::new();
    }

    let samples_per_chunk =
        samples_per_chunk(buffer.sample_rate, buffer.channels, chunk_duration_ms);
    let total_chunks = buffer.samples.chunks(samples_per_chunk).count();

    buffer
        .samples
        .chunks(samples_per_chunk)
        .enumerate()
        .map(|(index, samples)| AudioChunk {
            bytes: samples_to_le_bytes(samples).into(),
            duration_ms: chunk_duration_ms,
            is_final: index + 1 == total_chunks,
        })
        .collect()
}

fn log_stream_error(error: cpal::Error) {
    log::error!("audio input stream error: {error}");
}

fn samples_per_chunk(sample_rate: u32, channels: u16, chunk_duration_ms: u16) -> usize {
    ((sample_rate as u64 * channels as u64 * chunk_duration_ms as u64) / 1000).max(1) as usize
}

fn preroll_samples() -> usize {
    samples_per_chunk(TARGET_SAMPLE_RATE, TARGET_CHANNELS, PREROLL_MS)
}

fn estimate_duration_ms(sample_count: usize, sample_rate: u32, channels: u16) -> u16 {
    if sample_count == 0 || sample_rate == 0 || channels == 0 {
        return 0;
    }
    ((sample_count as u64 * 1000) / (sample_rate as u64 * channels as u64)) as u16
}

fn samples_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

struct CaptureBuffer {
    pending_samples: Vec<i16>,
    preroll_samples: VecDeque<i16>,
    resampler: AveragingResampler,
    sender: Option<(
        Sender<AudioChunk>,
        Arc<AudioQueueMonitor>,
        Option<IncidentAudioTap>,
    )>,
    samples_per_chunk: usize,
    chunk_duration_ms: u16,
    max_preroll_samples: usize,
}

impl CaptureBuffer {
    fn new(input_sample_rate: u32) -> Self {
        Self {
            pending_samples: Vec::new(),
            preroll_samples: VecDeque::with_capacity(preroll_samples()),
            resampler: AveragingResampler::new(input_sample_rate, TARGET_SAMPLE_RATE),
            sender: None,
            samples_per_chunk: samples_per_chunk(
                TARGET_SAMPLE_RATE,
                TARGET_CHANNELS,
                DEFAULT_CHUNK_MS,
            ),
            chunk_duration_ms: DEFAULT_CHUNK_MS,
            max_preroll_samples: preroll_samples(),
        }
    }

    fn push_mono_sample(&mut self, sample: f32) {
        let is_recording = self.sender.is_some();
        let before_len = self.pending_samples.len();
        self.resampler.push(sample, &mut self.pending_samples);

        if !is_recording {
            let new_samples = self.pending_samples[before_len..].to_vec();
            for sample in new_samples {
                self.push_preroll_sample(sample);
            }
        }

        if let Some((sender, monitor, tap)) = &self.sender {
            emit_ready_chunks(
                &mut self.pending_samples,
                self.samples_per_chunk,
                self.chunk_duration_ms,
                sender,
                monitor,
                tap.as_ref(),
            );
        } else {
            self.pending_samples.clear();
        }
    }

    fn start_recording(
        &mut self,
        sender: Sender<AudioChunk>,
        monitor: Arc<AudioQueueMonitor>,
        samples_per_chunk: usize,
        chunk_duration_ms: u16,
        incident_tap: Option<IncidentAudioTap>,
    ) {
        let before_len = self.pending_samples.len();
        self.resampler.flush(&mut self.pending_samples);
        let flushed_samples = self.pending_samples[before_len..].to_vec();
        for sample in flushed_samples {
            self.push_preroll_sample(sample);
        }

        self.pending_samples = self.preroll_samples.iter().copied().collect();
        self.preroll_samples.clear();
        self.samples_per_chunk = samples_per_chunk;
        self.chunk_duration_ms = chunk_duration_ms;
        self.sender = Some((sender, monitor, incident_tap));
        if let Some((sender, monitor, tap)) = &self.sender {
            emit_ready_chunks(
                &mut self.pending_samples,
                self.samples_per_chunk,
                self.chunk_duration_ms,
                sender,
                monitor,
                tap.as_ref(),
            );
        }
    }

    fn stop_recording(&mut self) -> Vec<i16> {
        self.resampler.flush(&mut self.pending_samples);
        self.sender = None;
        std::mem::take(&mut self.pending_samples)
    }

    fn push_preroll_sample(&mut self, sample: i16) {
        self.preroll_samples.push_back(sample);
        while self.preroll_samples.len() > self.max_preroll_samples {
            self.preroll_samples.pop_front();
        }
    }
}

struct AveragingResampler {
    output_per_input: f64,
    phase: f64,
    sum: f32,
    count: usize,
    last_output: f32,
}

impl AveragingResampler {
    fn new(input_sample_rate: u32, output_sample_rate: u32) -> Self {
        Self {
            output_per_input: output_sample_rate as f64 / input_sample_rate.max(1) as f64,
            phase: 0.0,
            sum: 0.0,
            count: 0,
            last_output: 0.0,
        }
    }

    fn push(&mut self, sample: f32, output: &mut Vec<i16>) {
        self.sum += sample;
        self.count += 1;
        self.phase += self.output_per_input;

        while self.phase + f64::EPSILON >= 1.0 {
            let output_sample = if self.count == 0 {
                self.last_output
            } else {
                self.sum / self.count as f32
            };
            self.last_output = output_sample;
            output.push(f32_to_i16(output_sample));
            self.sum = 0.0;
            self.count = 0;
            self.phase -= 1.0;
        }
    }

    fn flush(&mut self, output: &mut Vec<i16>) {
        if self.count == 0 {
            return;
        }
        let sample = self.sum / self.count as f32;
        self.last_output = sample;
        output.push(f32_to_i16(sample));
        self.sum = 0.0;
        self.count = 0;
        self.phase = 0.0;
    }
}

fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

fn push_f32(data: &[f32], channels: usize, capture: &Arc<Mutex<CaptureBuffer>>) {
    if let Ok(mut capture) = capture.lock() {
        for sample in mono_f32_frames(data, channels) {
            capture.push_mono_sample(sample);
        }
    }
}

fn push_i16(data: &[i16], channels: usize, capture: &Arc<Mutex<CaptureBuffer>>) {
    if let Ok(mut capture) = capture.lock() {
        for sample in mono_i16_frames(data, channels) {
            capture.push_mono_sample(sample);
        }
    }
}

fn push_u16(data: &[u16], channels: usize, capture: &Arc<Mutex<CaptureBuffer>>) {
    if let Ok(mut capture) = capture.lock() {
        for sample in mono_u16_frames(data, channels) {
            capture.push_mono_sample(sample);
        }
    }
}

fn mono_f32_frames(data: &[f32], channels: usize) -> impl Iterator<Item = f32> + '_ {
    let channels = channels.max(1);
    data.chunks_exact(channels)
        .map(move |frame| frame.iter().copied().sum::<f32>() / channels as f32)
}

fn mono_i16_frames(data: &[i16], channels: usize) -> impl Iterator<Item = f32> + '_ {
    let channels = channels.max(1);
    let scale = i16::MAX as f32;
    data.chunks_exact(channels).map(move |frame| {
        frame
            .iter()
            .map(|sample| *sample as f32 / scale)
            .sum::<f32>()
            / channels as f32
    })
}

fn mono_u16_frames(data: &[u16], channels: usize) -> impl Iterator<Item = f32> + '_ {
    let channels = channels.max(1);
    data.chunks_exact(channels).map(move |frame| {
        frame
            .iter()
            .map(|sample| (*sample as f32 - 32_768.0) / 32_768.0)
            .sum::<f32>()
            / channels as f32
    })
}

fn emit_ready_chunks(
    samples: &mut Vec<i16>,
    samples_per_chunk: usize,
    chunk_duration_ms: u16,
    sender: &Sender<AudioChunk>,
    monitor: &AudioQueueMonitor,
    incident_tap: Option<&IncidentAudioTap>,
) {
    while samples.len() >= samples_per_chunk {
        let chunk_samples = samples.drain(..samples_per_chunk).collect::<Vec<_>>();
        let chunk = AudioChunk {
            bytes: bytes::Bytes::from(samples_to_le_bytes(&chunk_samples)),
            duration_ms: chunk_duration_ms,
            is_final: false,
        };
        if let Some(tap) = incident_tap {
            tap.emit(&chunk);
        }
        match sender.try_send(chunk) {
            Ok(()) => monitor.record_sent(sender),
            Err(TrySendError::Full(_)) => {
                monitor.record_overflow();
                samples.clear();
                break;
            }
            Err(TrySendError::Closed(_)) => {
                samples.clear();
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_wav_writes_riff_header() {
        let buffer = AudioBuffer {
            samples: vec![0, 120, -120],
            sample_rate: 16_000,
            channels: 1,
            duration: Duration::from_millis(500),
        };

        let wav = encode_wav(&buffer).expect("wav should encode");

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn split_pcm_into_200ms_chunks_marks_last_chunk_final() {
        let buffer = AudioBuffer {
            samples: vec![1; 16_000],
            sample_rate: 16_000,
            channels: 1,
            duration: Duration::from_secs(1),
        };

        let chunks = split_pcm_into_chunks(&buffer, 200);

        assert_eq!(chunks.len(), 5);
        assert!(chunks.iter().take(4).all(|chunk| !chunk.is_final));
        assert!(chunks[4].is_final);
        assert_eq!(chunks[0].duration_ms, 200);
    }

    #[test]
    fn emit_ready_chunks_sends_only_complete_non_final_chunks() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
        let monitor = AudioQueueMonitor::default();
        let mut samples = vec![1, 2, 3, 4, 5];

        emit_ready_chunks(&mut samples, 2, 200, &sender, &monitor, None);

        let first = receiver.try_recv().unwrap();
        let second = receiver.try_recv().unwrap();
        assert_eq!(first.bytes, vec![1, 0, 2, 0]);
        assert_eq!(second.bytes, vec![3, 0, 4, 0]);
        assert!(!first.is_final);
        assert!(!second.is_final);
        assert_eq!(samples, vec![5]);
        assert!(receiver.try_recv().is_err());
        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.packets, 2);
        assert_eq!(snapshot.high_watermark, 2);
        assert!(!snapshot.overflow);
    }

    #[test]
    fn bounded_audio_queue_marks_overflow_without_dropping_silently() {
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);
        let monitor = AudioQueueMonitor::default();
        let mut samples = vec![1, 2, 3, 4];

        emit_ready_chunks(&mut samples, 2, 200, &sender, &monitor, None);

        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.packets, 1);
        assert_eq!(snapshot.high_watermark, 1);
        assert!(snapshot.overflow);
    }

    #[test]
    fn closed_audio_consumer_is_not_reported_as_overflow() {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let monitor = AudioQueueMonitor::default();
        let mut samples = vec![1, 2];
        drop(receiver);

        emit_ready_chunks(&mut samples, 2, 200, &sender, &monitor, None);

        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.packets, 0);
        assert_eq!(snapshot.high_watermark, 0);
        assert!(!snapshot.overflow);
    }

    #[test]
    fn estimate_duration_handles_empty_final_marker() {
        assert_eq!(estimate_duration_ms(0, 16_000, 1), 0);
        assert_eq!(estimate_duration_ms(3_200, 16_000, 1), 200);
    }

    #[test]
    fn idle_warm_capture_keeps_only_bounded_preroll() {
        let mut capture = CaptureBuffer::new(TARGET_SAMPLE_RATE);

        for _ in 0..(preroll_samples() + 100) {
            capture.push_mono_sample(0.25);
        }

        assert_eq!(capture.pending_samples.len(), 0);
        assert_eq!(capture.preroll_samples.len(), preroll_samples());
    }
    #[derive(Default)]
    struct CapturingIncidentSink {
        audio_pointers: std::sync::Mutex<Vec<usize>>,
    }

    impl IncidentSink for CapturingIncidentSink {
        fn try_emit(&self, event: IncidentEvent) -> crate::incident::model::EmitOutcome {
            if let IncidentEvent::AudioChunk { bytes, .. } = event {
                self.audio_pointers
                    .lock()
                    .unwrap()
                    .push(bytes.as_ptr() as usize);
            }
            crate::incident::model::EmitOutcome::Accepted
        }

        fn health_snapshot(&self) -> crate::incident::model::IncidentHealth {
            crate::incident::model::IncidentHealth::default()
        }
    }

    #[test]
    fn incident_audio_tap_shares_pcm_storage_instead_of_copying_it() {
        let sink = Arc::new(CapturingIncidentSink::default());
        let tap = IncidentAudioTap::new(sink.clone(), "attempt-zero-copy".to_string());
        let bytes = bytes::Bytes::from_static(&[1, 2, 3, 4]);
        let original_pointer = bytes.as_ptr() as usize;
        let chunk = AudioChunk {
            bytes,
            duration_ms: 1,
            is_final: false,
        };

        tap.emit(&chunk);

        assert_eq!(
            sink.audio_pointers.lock().unwrap().as_slice(),
            &[original_pointer]
        );
    }
}
