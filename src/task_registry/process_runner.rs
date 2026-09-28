use super::message::TaskMessage;
use super::state::State;
use anyhow::Result;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

pub(super) fn spawn(
    state: &Arc<Mutex<State>>,
    cmd: &[String],
    cancel_token: CancellationToken,
    generation: usize,
) -> Result<()> {
    if cmd.is_empty() {
        anyhow::bail!("spawn_process called with empty cmd");
    }

    let program = cmd[0].clone();
    let args = cmd[1..].to_vec();
    trace!("spawning process: {} {:?}", &program, &args);

    let mut child = Command::new(&program)
        .args(&args)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| {
            anyhow::anyhow!(
                "Failed to spawn child process: {} {:?}: {}",
                program,
                args,
                error
            )
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout of process: {:?}", cmd))?;

    let state = Arc::clone(state);
    let cmd_debug = cmd.to_vec();
    let mut reader = BufReader::new(stdout).lines();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    debug!("cancellation received, killing process: {:?}", cmd_debug);
                    if let Err(error) = child.kill().await {
                        warn!("failed to kill child process {:?}: {}", cmd_debug, error);
                    }
                    break;
                }
                line = reader.next_line() => match line {
                    Ok(Some(data)) => forward(&state, generation, TaskMessage::Data(data)),
                    Ok(None) => {
                        debug!("process finished (EOF): {:?}", cmd_debug);
                        finish(&state, generation, Some(TaskMessage::Eof));
                        break;
                    }
                    Err(error) => {
                        error!("read error from process {:?}: {}", cmd_debug, error);
                        finish(&state, generation, Some(TaskMessage::Error(format!(
                            "Failed to read line from process: {:?}, err: {}", cmd_debug, error
                        ))));
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

fn forward(state: &Mutex<State>, generation: usize, message: TaskMessage) {
    let mut cancel = None;
    let mut state = state.lock().expect("state mutex poisoned");
    if let State::Active {
        generation: active_generation,
        subscribers,
        cancel_token,
    } = &mut *state
        && *active_generation == generation
    {
        subscribers.retain(|(_, sender)| match sender.try_send(message.clone()) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                trace!("subscriber buffer full, dropping message");
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        });
        if subscribers.is_empty() {
            cancel = Some(cancel_token.clone());
            *state = State::Idle;
        }
    }
    drop(state);
    if let Some(token) = cancel {
        token.cancel();
    }
}

fn finish(state: &Mutex<State>, generation: usize, final_message: Option<TaskMessage>) {
    let mut state = state.lock().expect("state mutex poisoned");
    let subscribers = match &mut *state {
        State::Active {
            generation: active_generation,
            subscribers,
            ..
        } if *active_generation == generation => std::mem::take(subscribers),
        _ => return,
    };
    *state = State::Idle;
    drop(state);

    for (_, sender) in subscribers {
        if let Some(message) = &final_message {
            let _ = sender.try_send(message.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_messages_from_old_process_do_not_touch_new_generation() {
        let (sender, mut receiver) = mpsc::channel(4);
        let state = Mutex::new(State::Active {
            generation: 2,
            cancel_token: CancellationToken::new(),
            subscribers: vec![(2, sender)],
        });

        forward(&state, 1, TaskMessage::Data("stale".into()));
        finish(&state, 1, Some(TaskMessage::Eof));
        assert!(matches!(
            *state.lock().unwrap(),
            State::Active { generation: 2, .. }
        ));
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));

        forward(&state, 2, TaskMessage::Data("new".into()));
        finish(&state, 2, Some(TaskMessage::Eof));
        assert!(matches!(*state.lock().unwrap(), State::Idle));
        assert_eq!(
            receiver.try_recv().unwrap(),
            TaskMessage::Data("new".into())
        );
        assert_eq!(receiver.try_recv().unwrap(), TaskMessage::Eof);
    }
}
