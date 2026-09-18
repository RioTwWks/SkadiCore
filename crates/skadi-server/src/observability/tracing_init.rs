//! Инициализация tracing с маскированием Bearer-токенов в выводе.

use anyhow::Result;
use skadi_core::redact_bearer_tokens;
use std::io::{self, Write};
use tracing_subscriber::EnvFilter;

pub enum LogFormat {
    Json,
    Pretty,
}

pub fn init_tracing(log_level: &str, format: LogFormat) -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| log_level.into());

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(|| RedactingWriterGuard {
            inner: io::stdout(),
            buffer: Vec::new(),
        });

    match format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Pretty => builder.init(),
    }
    Ok(())
}

struct RedactingWriterGuard {
    inner: io::Stdout,
    buffer: Vec<u8>,
}

impl Write for RedactingWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line = self.buffer.drain(..=pos).collect::<Vec<_>>();
            let text = String::from_utf8_lossy(&line);
            let redacted = redact_bearer_tokens(&text);
            self.inner.write_all(redacted.as_bytes())?;
        }
        self.inner.flush()
    }
}
