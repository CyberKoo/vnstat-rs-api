use crate::model::vnstat::{Interface, VnstatData};
use crate::task_handle::TaskMessage;
use crate::task_manager::TaskManager;
use crate::utils::timestamp;
use anyhow::{Context, Result};
use async_stream::stream;
use axum::response::sse::Event;
use cached::proc_macro::cached;
use futures_util::Stream;
use log::warn;
use std::ffi::OsString;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

static EXECUTABLE: OnceLock<OsString> = OnceLock::new();

pub fn set_vnstat_executable(executable: impl AsRef<std::ffi::OsStr>) -> Result<()> {
    EXECUTABLE
        .set(executable.as_ref().to_os_string())
        .map_err(|_| anyhow::anyhow!("vnStat executable already set"))
}

fn get_vnstat_executable() -> Result<&'static OsString> {
    EXECUTABLE
        .get()
        .context("vnStat executable not set (call set_executable first)")
}

#[cached(size = 100, time = 60, time_refresh = true, result = true)]
pub async fn fetch_vnstat_data() -> Result<VnstatData> {
    let output = tokio::process::Command::new(get_vnstat_executable()?)
        .arg("--json")
        .output()
        .await
        .context("failed to execute vnStat")?;

    let json_str =
        String::from_utf8(output.stdout).context("failed to parse vnStat response as UTF-8")?;

    serde_json::from_str(&json_str).context("failed to deserialize vnStat JSON response")
}

#[cached(size = 100, time = 600, time_refresh = true, result = true)]
pub async fn list_vnstat_interfaces() -> Result<Vec<String>> {
    let data = fetch_vnstat_data().await?;
    let interfaces: Vec<_> = data.interfaces.iter().map(|i| i.name.clone()).collect();

    Ok(interfaces)
}

pub async fn fetch_interface_stats(if_name: impl AsRef<str>) -> Result<Interface> {
    let output = fetch_vnstat_data().await?;

    output
        .interfaces
        .iter()
        .find(|i| i.name == if_name.as_ref())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("interface not found"))
}

pub fn build_live_stream_command(if_name: impl AsRef<str>) -> Result<Vec<String>> {
    let command = vec![
        get_vnstat_executable()?.to_string_lossy().to_string(),
        "-i".to_string(),
        if_name.as_ref().to_string(),
        "--json".to_string(),
        "-l".to_string(),
    ];

    Ok(command)
}

pub async fn stream_interface_live_stats(
    manager: Arc<TaskManager>,
    if_name: String,
) -> impl Stream<Item = Result<Event, String>> {
    let cmd = build_live_stream_command(&if_name).unwrap();
    let stream_name = if_name.clone();

    stream! {
        let (mut receiver, _guard) = manager.subscribe(if_name.clone(), cmd).await;

        loop {
            match receiver.recv().await {
                Ok(message) => match message {
                    TaskMessage::Data(data) => yield Ok(Event::default().data(data).id(timestamp::get_in_ms().to_string())),
                    TaskMessage::Comment(comment) => yield Ok(Event::default().comment(comment)),
                    TaskMessage::Error(error) => yield Err(error),
                    TaskMessage::Eof => break
                },
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(n)) => {
                    warn!("SSE [{}] message lagged: dropped {} messages", stream_name, n);
                    yield Ok(Event::default().comment("Message dropped (lag)"));
                }
            }
        }
    }
}
