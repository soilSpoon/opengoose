use crossterm::event::{self, Event, KeyEvent};
use opengoose_types::{AppEvent, AppEventKind};
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

        // Crossterm terminal events — requires a real terminal; excluded from coverage.
        let tx_term = tx.clone();
        let cancel_term = cancel.clone();
        fn crossterm_poll_loop(tx: mpsc::UnboundedSender<TuiEvent>, cancel: CancellationToken) {
            loop {
                if cancel.is_cancelled() {
                    break;
                }
                if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false)
                    && let Ok(evt) = event::read()
                {
                    let tui_event = match evt {
                        Event::Key(key) => TuiEvent::Key(key),
                        Event::Resize(..) => TuiEvent::Resize,
                        _ => continue,
                    };
                    if tx.send(tui_event).is_err() {
                        break;
                    }
                }
            }
        }
        std::thread::spawn(move || crossterm_poll_loop(tx_term, cancel_term));

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
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                let _ = tx_bus.send(TuiEvent::AppEvent(AppEvent {
                                    kind: AppEventKind::Error {
                                        context: "event_bus".into(),
                                        message: format!("{n} events dropped due to lag"),
                                    },
                                    timestamp: std::time::Instant::now(),
                                }));
                                continue;
                            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_types::EventBus;

    #[tokio::test]
    async fn test_event_handler_receives_app_event() {
        let bus = EventBus::new(16);
        let cancel = CancellationToken::new();
        let mut handler = EventHandler::new(bus.subscribe(), cancel.clone());

        bus.emit(AppEventKind::ChannelReady {
            platform: opengoose_types::Platform::Discord,
        });

        // Should receive the AppEvent
        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), handler.next())
            .await
            .unwrap();

        match evt {
            TuiEvent::AppEvent(e) => {
                assert!(matches!(
                    e.kind,
                    AppEventKind::ChannelReady {
                        platform: opengoose_types::Platform::Discord
                    }
                ));
            }
            TuiEvent::Tick => {
                // Tick may arrive first due to timing; try again
                let evt2 = tokio::time::timeout(std::time::Duration::from_secs(2), handler.next())
                    .await
                    .unwrap();
                assert!(matches!(evt2, TuiEvent::AppEvent(_)));
            }
            _ => {}
        }

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_event_handler_quit_on_cancel() {
        let bus = EventBus::new(16);
        let cancel = CancellationToken::new();
        let mut handler = EventHandler::new(bus.subscribe(), cancel.clone());

        cancel.cancel();

        // Should eventually receive Quit
        let mut got_quit = false;
        for _ in 0..10 {
            let evt = tokio::time::timeout(std::time::Duration::from_secs(1), handler.next())
                .await
                .unwrap();
            if matches!(evt, TuiEvent::Quit) {
                got_quit = true;
                break;
            }
        }
        assert!(got_quit);
    }

    #[tokio::test]
    async fn test_event_handler_receives_tick() {
        let bus = EventBus::new(16);
        let cancel = CancellationToken::new();
        let mut handler = EventHandler::new(bus.subscribe(), cancel.clone());

        // The tick timer fires every 1s, first tick is immediate
        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), handler.next())
            .await
            .unwrap();

        // Should be a Tick (first event from the interval)
        assert!(matches!(evt, TuiEvent::Tick));

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_event_handler_broadcast_closed() {
        let cancel = CancellationToken::new();
        let bus = EventBus::new(16);
        let rx = bus.subscribe();
        let mut handler = EventHandler::new(rx, cancel.clone());

        // Drop the bus to close the broadcast channel
        drop(bus);

        // Handler should still work via tick/quit
        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), handler.next())
            .await
            .unwrap();
        // Should get Tick since broadcast is closed
        assert!(matches!(evt, TuiEvent::Tick));

        cancel.cancel();
    }
}
