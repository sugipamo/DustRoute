//! Business-workflow fixtures only. Never compiled into the MCP server.
//! Explicit fixture receipts do not establish Voxrig or Minecraft conformance.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Clone, Debug)]
pub(super) struct TestTransport {
    pub(super) address: String,
    pub(super) timeout: Duration,
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
impl TestTransport {
    pub(super) async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, BotBridgeError> {
        let timeout = self.timeout;
        let operation = async {
            let mut stream = TcpStream::connect(&self.address).await?;
            let request = json!({
                "id": NEXT_ID.fetch_add(1, Ordering::Relaxed),
                "method": method,
                "params": params,
            });
            stream
                .write_all(serde_json::to_string(&request)?.as_bytes())
                .await?;
            stream.write_all(b"\n").await?;
            let mut response = String::new();
            BufReader::new(stream).read_line(&mut response).await?;
            let response: Value = serde_json::from_str(&response)?;
            if let Some(error) = response.get("error") {
                if response.get("failure_protocol").and_then(Value::as_str)
                    == Some("dustroute.bridge-failure.v1")
                    && let Some(detail) = response.get("submission_failure")
                {
                    let detail = serde_json::from_value::<crate::failure::SubmissionFailure>(
                        detail.clone(),
                    )?;
                    return Err(BotBridgeError::Submission(Box::new(detail)));
                }
                return Err(BotBridgeError::Protocol(
                    error.as_str().unwrap_or("unknown bridge error").to_owned(),
                ));
            }
            serde_json::from_value(
                response
                    .get("result")
                    .cloned()
                    .ok_or_else(|| BotBridgeError::Protocol("response has no result".to_owned()))?,
            )
            .map_err(Into::into)
        };
        tokio::time::timeout(timeout, operation)
            .await
            .map_err(|_| BotBridgeError::Timeout(timeout))?
    }
}
