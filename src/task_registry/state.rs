use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(super) enum State {
    #[default]
    Idle,
    Active {
        generation: usize,
        cancel_token: CancellationToken,
        subscribers: Vec<(usize, mpsc::Sender<super::TaskMessage>)>,
    },
}
