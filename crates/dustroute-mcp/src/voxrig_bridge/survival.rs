//! Exclusive loan of the source connection, including its checked replacements.
//! An abandoned/uncertain executor never returns a possibly active source.
use super::*;

pub(crate) struct SurvivalLease {
    bridge: Arc<VoxrigBridge>,
    source: Client,
    restore_on_drop: bool,
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

impl VoxrigBridge {
    pub(super) fn client(&self) -> Result<Client, BotBridgeError> {
        self.client.lock().unwrap_or_else(|e| e.into_inner()).clone()
            .ok_or_else(|| BotBridgeError::Protocol(
                "native source reserved by survival construction; inspect its job before other operations".into()))
    }

    pub(crate) fn lease_survival(self: &Arc<Self>) -> Result<SurvivalLease, BotBridgeError> {
        let guard = self
            .mutations
            .clone()
            .try_lock_owned()
            .map_err(|_| BotBridgeError::Protocol("native source is busy".into()))?;
        let source = self
            .client
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or_else(|| {
                BotBridgeError::Protocol("native source requires survival job inspection".into())
            })?;
        Ok(SurvivalLease {
            bridge: self.clone(),
            source,
            restore_on_drop: true,
            _guard: guard,
        })
    }
}

impl SurvivalLease {
    pub(crate) fn source(&self) -> Client {
        self.source.clone()
    }
    pub(crate) fn reconnect(&self) -> ConnectionConfig {
        self.bridge.reconnect.clone()
    }

    /// Only after read-only planning (no executor dispatched), or a fully
    /// completed executor or a sealed, independently checked idle checkpoint.
    /// Stopped/uncertain jobs retain their lease instead.
    pub(crate) fn begin_execution(&mut self) {
        self.restore_on_drop = false;
    }

    pub(crate) fn release(mut self, current: Client) {
        self.restore_on_drop = false;
        *self.bridge.client.lock().unwrap_or_else(|e| e.into_inner()) = Some(current);
    }
}

impl Drop for SurvivalLease {
    fn drop(&mut self) {
        // Read-only callers may be cancelled while detached planning continues.
        // Once execution begins, cancellation instead quarantines the source.
        if self.restore_on_drop {
            *self.bridge.client.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(self.source.clone());
        }
    }
}
