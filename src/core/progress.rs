use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressUpdate {
    Info(String),
    Started {
        title: String,
        total_bytes: u64,
    },
    Progress {
        current_bytes: u64,
        total_bytes: u64,
    },
    Finished {
        message: String,
    },
    Error(String),
}

pub trait ProgressReporter: Send + Sync {
    fn on_info(&self, message: &str);
    fn on_start(&self, title: &str, total_bytes: u64);
    fn on_progress(&self, current_bytes: u64, total_bytes: u64);
    fn on_finish(&self, message: &str);
    fn on_error(&self, error: &str);
}

/// Throttler para evitar saturar el canal de eventos en descargas rápidas.
/// Emite actualización como máximo cada `min_interval` (ej. 50ms) o cuando el avance es >= 1%.
#[allow(dead_code)]
pub struct ProgressThrottler {
    last_emit: Mutex<Instant>,
    last_bytes: AtomicU64,
    min_interval: Duration,
}

#[allow(dead_code)]
impl ProgressThrottler {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            last_emit: Mutex::new(Instant::now()),
            last_bytes: AtomicU64::new(0),
            min_interval,
        }
    }

    pub fn should_emit(&self, current_bytes: u64, total_bytes: u64) -> bool {
        let mut last_emit = self.last_emit.lock().unwrap();
        let last_bytes = self.last_bytes.load(Ordering::Relaxed);
        let elapsed = last_emit.elapsed();

        let one_percent = if total_bytes > 0 {
            total_bytes / 100
        } else {
            0
        };
        let bytes_advanced = current_bytes.saturating_sub(last_bytes);

        if elapsed >= self.min_interval || (one_percent > 0 && bytes_advanced >= one_percent) {
            *last_emit = Instant::now();
            self.last_bytes.store(current_bytes, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

/// Reporter para la interfaz TUI. Envía actualizaciones a través de un canal mpsc acotado.
/// Si el canal está temporalmente lleno para eventos de progreso intermedios, descarta la muestra
/// intermedia para no bloquear el hilo de I/O (backpressure seguro con lossy sampling de progreso).
#[allow(dead_code)]
pub struct ChannelProgressReporter {
    sender: mpsc::Sender<ProgressUpdate>,
    throttler: ProgressThrottler,
}

#[allow(dead_code)]
impl ChannelProgressReporter {
    pub fn new(sender: mpsc::Sender<ProgressUpdate>) -> Self {
        Self {
            sender,
            throttler: ProgressThrottler::new(Duration::from_millis(50)),
        }
    }
}

impl ProgressReporter for ChannelProgressReporter {
    fn on_info(&self, message: &str) {
        let _ = self
            .sender
            .try_send(ProgressUpdate::Info(message.to_string()));
    }

    fn on_start(&self, title: &str, total_bytes: u64) {
        let _ = self.sender.try_send(ProgressUpdate::Started {
            title: title.to_string(),
            total_bytes,
        });
    }

    fn on_progress(&self, current_bytes: u64, total_bytes: u64) {
        if self.throttler.should_emit(current_bytes, total_bytes) {
            let _ = self.sender.try_send(ProgressUpdate::Progress {
                current_bytes,
                total_bytes,
            });
        }
    }

    fn on_finish(&self, message: &str) {
        let _ = self.sender.try_send(ProgressUpdate::Finished {
            message: message.to_string(),
        });
    }

    fn on_error(&self, error: &str) {
        let _ = self
            .sender
            .try_send(ProgressUpdate::Error(error.to_string()));
    }
}

/// Reporter silencioso para tests o procesos demonio.
pub struct SilentProgressReporter;

impl ProgressReporter for SilentProgressReporter {
    fn on_info(&self, _message: &str) {}
    fn on_start(&self, _title: &str, _total_bytes: u64) {}
    fn on_progress(&self, _current_bytes: u64, _total_bytes: u64) {}
    fn on_finish(&self, _message: &str) {}
    fn on_error(&self, _error: &str) {}
}
