//! Preview playback via rodio (same backend as the egui shell).

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

pub struct PreviewPlayer {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    sink: Option<Sink>,
    current_path: Option<PathBuf>,
}

impl PreviewPlayer {
    pub fn new() -> Result<Self, String> {
        let (stream, handle) =
            OutputStream::try_default().map_err(|e| format!("audio output unavailable: {e}"))?;
        Ok(Self {
            _stream: stream,
            handle,
            sink: None,
            current_path: None,
        })
    }

    pub fn play_file(&mut self, path: &Path) -> Result<(), String> {
        self.stop();
        let file = File::open(path).map_err(|e| format!("open preview: {e}"))?;
        let reader = BufReader::new(file);
        let source = Decoder::new(reader).map_err(|e| format!("decode preview: {e}"))?;
        let sink = Sink::try_new(&self.handle).map_err(|e| format!("audio sink: {e}"))?;
        sink.append(source);
        self.sink = Some(sink);
        self.current_path = Some(path.to_path_buf());
        Ok(())
    }

    pub fn pause(&self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn resume(&self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.current_path = None;
    }

    pub fn is_playing(&self) -> bool {
        self.sink
            .as_ref()
            .map(|s| !s.empty() && !s.is_paused())
            .unwrap_or(false)
    }
}
