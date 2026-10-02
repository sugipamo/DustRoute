//! The live 1.21.11 sender closes a session on any uncertain frame boundary.
use super::*;
use tokio::io::AsyncWrite;

struct WriteAttempt<'a> {
    session: &'a Session,
    packet_id: i32,
    armed: bool,
}
impl Drop for WriteAttempt<'_> {
    fn drop(&mut self) {
        if self.armed {
            // Publish uncertainty before closure and before the writer lock is
            // released. Queued sends recheck this boundary under that same lock.
            let _ = self.session.interrupted_packet.compare_exchange(
                -1,
                self.packet_id,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            self.session.stop();
        }
    }
}

impl Session {
    pub(super) fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        // Wake both a blocked writer and the receive loop. Retain a permit for
        // teardown which has not yet started waiting.
        self.cancel.notify_waiters();
        self.cancel.notify_one();
        self.changed.notify_waiters();
    }

    pub(super) fn check_outbound(&self) -> Result<()> {
        let closed = self.stopped.load(Ordering::Acquire);
        let packet = self.interrupted_packet.load(Ordering::Acquire);
        if packet >= 0 {
            return Err(Error::new(
                ErrorKind::UncertainDispatch,
                anyhow::anyhow!(
                    "connection closed after interrupted frame for packet {packet}; server effects unknown; inspect history without retry"
                ),
            ));
        }
        if closed {
            return Err(Error::new(
                ErrorKind::Disconnected,
                anyhow::anyhow!("connection closed"),
            ));
        }
        Ok(())
    }

    pub(super) async fn send(&self, id: i32, payload: &[u8]) -> Result<()> {
        self.check_outbound()?;
        // Cancellation while waiting for this lock cannot interrupt a frame.
        let mut writer = self.writer.lock().await;
        let compression = writer.compression;
        self.write_frame(&mut writer.stream, compression, id, payload)
            .await
    }

    // Shared by the real TCP sender and bounded stream tests. The caller owns
    // the writer lock for the complete attempt, including guard destruction.
    async fn write_frame<W: AsyncWrite + Unpin>(
        &self,
        stream: &mut W,
        compression: Option<i32>,
        id: i32,
        payload: &[u8],
    ) -> Result<()> {
        let cancelled = self.cancel.notified();
        tokio::pin!(cancelled);
        cancelled.as_mut().enable();
        self.check_outbound()?;
        let mut attempt = WriteAttempt {
            session: self,
            packet_id: id,
            armed: true,
        };
        let result = tokio::select! {
            biased;
            _ = &mut cancelled => Err(anyhow::anyhow!("connection closed during frame write")),
            result = write_packet(stream, compression, id, payload) => result,
        };
        match result {
            Ok(()) => {
                attempt.armed = false;
                Ok(())
            }
            Err(error) => {
                drop(attempt);
                Err(Error::new(
                    ErrorKind::UncertainDispatch,
                    error.context(
                        "connection closed after uncertain frame write; server effects unknown",
                    ),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests;
