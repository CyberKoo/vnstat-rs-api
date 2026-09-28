use crate::task_registry::{TaskMessage, TaskRegistry};
use async_stream::stream;
use futures_util::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tracing::warn;

/// A transport-neutral live statistics message emitted by the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveStatsMessage {
    Data(String),
}

pub(crate) async fn stream_interface_live_stats(
    registry: Arc<TaskRegistry>,
    if_name: String,
    command: Result<Vec<String>, anyhow::Error>,
) -> Pin<Box<dyn Stream<Item = Result<LiveStatsMessage, String>> + Send>> {
    let cmd = match command {
        Ok(cmd) => cmd,
        Err(error) => {
            warn!("Failed to build live stream command: {}", error);
            return Box::pin(futures_util::stream::once(async move {
                Err(format!("Failed to start live stream: {}", error))
            }));
        }
    };

    Box::pin(stream! {
        let (mut receiver, _guard) = registry.subscribe(if_name, cmd).await;
        while let Some(message) = receiver.recv().await {
            match message {
                TaskMessage::Data(data) => yield Ok(LiveStatsMessage::Data(data)),
                TaskMessage::Error(error) => yield Err(error),
                TaskMessage::Eof => break,
            }
        }
    })
}
