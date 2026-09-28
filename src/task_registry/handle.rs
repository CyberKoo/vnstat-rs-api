use super::message::TaskMessage;
use super::process_runner;
use super::state::State;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

pub(super) struct TaskHandle {
    state: Arc<Mutex<State>>,
    next_subscriber_id: AtomicUsize,
}

impl TaskHandle {
    pub(super) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::Idle)),
            next_subscriber_id: AtomicUsize::new(0),
        }
    }

    pub(super) fn subscribe(
        &self,
        cmd: Vec<String>,
        buffer: usize,
    ) -> (mpsc::Receiver<TaskMessage>, usize) {
        let (tx, rx) = mpsc::channel(buffer);
        let subscriber_id = self.next_subscriber_id.fetch_add(1, Ordering::Relaxed);
        let mut need_spawn: Option<(Vec<String>, CancellationToken)> = None;

        {
            let mut state = self.state.lock().expect("state mutex poisoned");
            match &mut *state {
                State::Idle => {
                    let token = CancellationToken::new();
                    need_spawn = Some((cmd.clone(), token.clone()));
                    *state = State::Active {
                        generation: subscriber_id,
                        cancel_token: token,
                        subscribers: vec![(subscriber_id, tx)],
                    };
                }
                State::Active { subscribers, .. } => {
                    subscribers.push((subscriber_id, tx));
                }
            }
        }

        if let Some((cmd, token)) = need_spawn {
            if let Err(error) = process_runner::spawn(&self.state, &cmd, token, subscriber_id) {
                error!(%error, "spawn task failed");
                let mut state = self.state.lock().expect("state mutex poisoned");
                let subscribers = match &mut *state {
                    State::Active { subscribers, .. } => std::mem::take(subscribers),
                    State::Idle => unreachable!("state was set to Active before spawning"),
                };
                *state = State::Idle;
                drop(state);
                for (_, sender) in subscribers {
                    let _ = sender.try_send(TaskMessage::Error("Spawn task failed".to_string()));
                }
            } else {
                debug!("process spawned successfully");
            }
        }

        (rx, subscriber_id)
    }

    pub(super) fn is_idle(&self) -> bool {
        matches!(
            *self.state.lock().expect("state mutex poisoned"),
            State::Idle
        )
    }

    pub(super) fn unsubscribe(&self, id: usize) {
        let mut cancel = None;
        {
            let mut state = self.state.lock().expect("state mutex poisoned");
            if let State::Active {
                subscribers,
                cancel_token,
                ..
            } = &mut *state
            {
                subscribers.retain(|(subscriber_id, _)| *subscriber_id != id);
                if subscribers.is_empty() {
                    cancel = Some(cancel_token.clone());
                    *state = State::Idle;
                }
            }
        }
        if let Some(token) = cancel {
            token.cancel();
        }
    }
}
