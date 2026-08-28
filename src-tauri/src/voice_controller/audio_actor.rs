use super::resources::SessionCancellation;
use crate::audio::{AudioQueueMonitor, IncidentAudioTap, Recorder};
use crate::provider::{AudioChunk, AudioStreamInfo};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

const AUDIO_CONTROL_CAPACITY: usize = 4;

enum AudioCommand {
    Start {
        session_id: u64,
        chunk_duration_ms: u16,
        chunk_sender: mpsc::Sender<AudioChunk>,
        queue_monitor: Arc<AudioQueueMonitor>,
        incident_tap: Option<IncidentAudioTap>,
        cancellation: Arc<SessionCancellation>,
        response: oneshot::Sender<Result<AudioStreamInfo, String>>,
    },
    Stop {
        session_id: u64,
        response: oneshot::Sender<Result<Duration, String>>,
    },
    Cancel {
        session_id: u64,
        response: oneshot::Sender<()>,
    },
    Shutdown {
        response: oneshot::Sender<()>,
    },
}

#[derive(Clone)]
pub(super) struct AudioSessionHandle {
    tx: mpsc::Sender<AudioCommand>,
}

impl AudioSessionHandle {
    pub(super) fn spawn() -> Self {
        let (tx, rx) = mpsc::channel(AUDIO_CONTROL_CAPACITY);
        tauri::async_runtime::spawn(AudioSessionActor::new(rx).run());
        Self { tx }
    }

    pub(super) async fn start(
        &self,
        session_id: u64,
        chunk_duration_ms: u16,
        chunk_sender: mpsc::Sender<AudioChunk>,
        queue_monitor: Arc<AudioQueueMonitor>,
        incident_tap: Option<IncidentAudioTap>,
        cancellation: Arc<SessionCancellation>,
    ) -> Result<AudioStreamInfo, String> {
        let (response, result) = oneshot::channel();
        self.tx
            .send(AudioCommand::Start {
                session_id,
                chunk_duration_ms,
                chunk_sender,
                queue_monitor,
                incident_tap,
                cancellation,
                response,
            })
            .await
            .map_err(|_| "音频执行器不可用".to_string())?;
        result
            .await
            .map_err(|_| "音频执行器未返回启动结果".to_string())?
    }

    pub(super) async fn stop(&self, session_id: u64) -> Result<Duration, String> {
        let (response, result) = oneshot::channel();
        self.tx
            .send(AudioCommand::Stop {
                session_id,
                response,
            })
            .await
            .map_err(|_| "音频执行器不可用".to_string())?;
        result
            .await
            .map_err(|_| "音频执行器未返回停止结果".to_string())?
    }

    pub(super) async fn cancel(&self, session_id: u64) {
        let (response, result) = oneshot::channel();
        if self
            .tx
            .send(AudioCommand::Cancel {
                session_id,
                response,
            })
            .await
            .is_ok()
        {
            let _ = result.await;
        }
    }

    pub(super) fn request_cancel(&self, session_id: u64) -> Result<(), String> {
        let (response, _result) = oneshot::channel();
        self.tx
            .try_send(AudioCommand::Cancel {
                session_id,
                response,
            })
            .map_err(|error| format!("音频取消请求未入队: {error}"))
    }

    pub(super) async fn shutdown(&self) {
        let (response, result) = oneshot::channel();
        if self
            .tx
            .send(AudioCommand::Shutdown { response })
            .await
            .is_ok()
        {
            let _ = result.await;
        }
    }
}

struct AudioSessionActor {
    recorder: Recorder,
    ownership: AudioOwnership,
    rx: mpsc::Receiver<AudioCommand>,
}

#[derive(Default)]
struct AudioOwnership {
    current_session: Option<u64>,
}

impl AudioOwnership {
    fn is_available(&self) -> bool {
        self.current_session.is_none()
    }

    fn started(&mut self, session_id: u64) {
        self.current_session = Some(session_id);
    }

    fn take_if_current(&mut self, session_id: u64) -> bool {
        if self.current_session == Some(session_id) {
            self.current_session = None;
            true
        } else {
            false
        }
    }

    fn take(&mut self) -> bool {
        self.current_session.take().is_some()
    }
}

impl AudioSessionActor {
    fn new(rx: mpsc::Receiver<AudioCommand>) -> Self {
        Self {
            recorder: Recorder::new(),
            ownership: AudioOwnership::default(),
            rx,
        }
    }

    async fn run(mut self) {
        while let Some(command) = self.rx.recv().await {
            match command {
                AudioCommand::Start {
                    session_id,
                    chunk_duration_ms,
                    chunk_sender,
                    queue_monitor,
                    incident_tap,
                    cancellation,
                    response,
                } => {
                    let outcome = if cancellation.is_cancelled() {
                        Err("录音启动已取消".to_string())
                    } else if !self.ownership.is_available() {
                        Err("已有录音会话占用音频设备".to_string())
                    } else {
                        match self.recorder.start_streaming(
                            chunk_duration_ms,
                            chunk_sender,
                            queue_monitor,
                            incident_tap,
                        ) {
                            Ok(_info) if cancellation.is_cancelled() => {
                                let _ = self.recorder.stop_streaming();
                                Err("录音启动已取消".to_string())
                            }
                            Ok(info) => {
                                self.ownership.started(session_id);
                                Ok(info)
                            }
                            Err(error) => Err(error.to_string()),
                        }
                    };
                    let _ = response.send(outcome);
                }
                AudioCommand::Stop {
                    session_id,
                    response,
                } => {
                    let outcome = if self.ownership.take_if_current(session_id) {
                        self.recorder
                            .stop_streaming()
                            .map_err(|error| error.to_string())
                    } else {
                        Err("录音会话已过期".to_string())
                    };
                    let _ = response.send(outcome);
                }
                AudioCommand::Cancel {
                    session_id,
                    response,
                } => {
                    if self.ownership.take_if_current(session_id) {
                        let _ = self.recorder.stop_streaming();
                    }
                    let _ = response.send(());
                }
                AudioCommand::Shutdown { response } => {
                    if self.ownership.take() {
                        let _ = self.recorder.stop_streaming();
                    }
                    let _ = response.send(());
                    break;
                }
            }
        }
        if self.ownership.take() {
            let _ = self.recorder.stop_streaming();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_ownership_rejects_stale_stop_and_preserves_current_session() {
        let mut ownership = AudioOwnership::default();
        assert!(ownership.is_available());
        ownership.started(8);
        assert!(!ownership.take_if_current(7));
        assert!(!ownership.is_available());
        assert!(ownership.take_if_current(8));
        assert!(ownership.is_available());
    }

    #[test]
    fn audio_ownership_allows_only_one_current_session() {
        let mut ownership = AudioOwnership::default();
        ownership.started(3);
        assert!(!ownership.is_available());
        assert!(ownership.take());
        assert!(ownership.is_available());
    }
}
