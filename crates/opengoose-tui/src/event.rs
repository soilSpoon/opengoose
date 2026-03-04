use crossterm::event::{self, Event, KeyEvent};
use opengoose_types::AppEvent;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub enum TuiEvent {
    Key(KeyEvent),
    AppEvent(AppEvent),
    Tick,
    Resize,
    Quit,
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<TuiEvent>,
}

impl EventHandler {
    pub fn new(bus_rx: broadcast::Receiver<AppEvent>, cancel: CancellationToken) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Crossterm terminal events
        let tx_term = tx.clone();
        let cancel_term = cancel.clone();
        std::thread::spawn(move || {
            loop {
                if cancel_term.is_cancelled() {
                    break;
                }
                if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        let tui_event = match evt {
                            Event::Key(key) => TuiEvent::Key(key),
                            Event::Resize(..) => TuiEvent::Resize,
                            _ => continue,
                        };
                        if tx_term.send(tui_event).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        // Broadcast AppEvents
        let tx_bus = tx.clone();
        let cancel_bus = cancel.clone();
        tokio::spawn(async move {
            let mut rx = bus_rx;
            loop {
                tokio::select! {
                    _ = cancel_bus.cancelled() => break,
                    result = rx.recv() => {
                        match result {
                            Ok(event) => {
                                if tx_bus.send(TuiEvent::AppEvent(event)).is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        });

        // Tick timer
        let tx_tick = tx.clone();
        let cancel_tick = cancel.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = cancel_tick.cancelled() => break,
                    _ = interval.tick() => {
                        if tx_tick.send(TuiEvent::Tick).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        // Forward cancellation as a Quit event so the TUI loop exits
        let tx_quit = tx;
        tokio::spawn(async move {
            cancel.cancelled().await;
            let _ = tx_quit.send(TuiEvent::Quit);
        });

        Self { rx }
    }

    pub async fn next(&mut self) -> TuiEvent {
        self.rx.recv().await.unwrap_or(TuiEvent::Tick)
    }
}
