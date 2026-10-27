use super::*;
use crate::controller::terminal_session::{self as session, *};
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test(start_paused = true)]
async fn deadlines_follow_latest_focus_and_saved_rates_without_idle_ticks_or_catchup() {
    let mut presentation = Presentation::default();
    let start = Instant::now();
    assert_eq!(presentation.deadline(false, None), None);
    assert_eq!(presentation.deadline(true, None), Some(start));
    presentation.published();
    assert_eq!(
        presentation.deadline(true, None),
        Some(start + Duration::from_millis(250))
    );
    tokio::time::advance(Duration::from_millis(10)).await;
    presentation.focused = true;
    let focused = start + Duration::from_secs(1) / 60;
    assert_eq!(presentation.deadline(true, None), Some(focused));
    // More incoming bytes do not move an existing output deadline.
    tokio::time::advance(Duration::from_millis(2)).await;
    assert_eq!(presentation.deadline(true, None), Some(focused));
    presentation.rates = Rates::new(20, 2);
    assert_eq!(
        presentation.deadline(true, None),
        Some(start + Duration::from_millis(50))
    );
    presentation.focused = false;
    assert_eq!(
        presentation.deadline(true, None),
        Some(start + Duration::from_millis(500))
    );
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(presentation.deadline(false, None), None);
    assert_eq!(presentation.deadline(true, None), Some(Instant::now()));
    presentation.published();
    assert_eq!(
        presentation.deadline(true, None),
        Some(Instant::now() + Duration::from_millis(500))
    );
    presentation.rates = Rates::new(0, u16::MAX);
    assert_eq!(presentation.rates, Rates::new(1, 120));
}

#[tokio::test(start_paused = true)]
async fn synchronized_output_extends_the_deadline_without_polling_an_expired_frame_timer() {
    let mut presentation = Presentation::default();
    presentation.published();
    let hold = Duration::from_secs(1);
    assert_eq!(
        presentation.deadline(true, Some(hold)),
        Some(Instant::now() + hold)
    );
    tokio::time::advance(Duration::from_millis(300)).await;
    assert_eq!(
        presentation.deadline(true, Some(Duration::from_millis(700))),
        Some(Instant::now() + Duration::from_millis(700))
    );
    assert_eq!(presentation.deadline(true, None), Some(Instant::now()));
    assert_eq!(presentation.deadline(false, Some(hold)), None);
}

async fn settle() {
    // The fixture has no I/O or blocking work. Keep the paused clock fixed
    // while the worker drains the bounded events/commands and parks again.
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn worker_coalesces_output_preserves_damage_and_applies_focus_and_settings_before_deadline() {
    let (mut worker, mut client) = session::clipboard::tests::fixture(false);
    let wakes = Arc::new(AtomicUsize::new(0));
    let observed = wakes.clone();
    worker.wake = Arc::new(move || {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    // Establish an already displayed blank frame and a bounded injected stream.
    worker.publish().unwrap();
    let before = client.view().unwrap();
    let (output, events) = mpsc::channel(8);
    worker.events = Events::Local(events);
    let (commands, queued) = mpsc::channel(8);
    client.commands = commands;
    let (stop, cancelled) = watch::channel(false);
    let (updates, settings) = watch::channel(SessionSettings {
        terminal: worker.settings.clone(),
        x11: worker.x11_settings.clone(),
        audio_format: worker.audio_format,
        refresh: Rates::default(),
    });
    let task = tokio::spawn(async move {
        let result = worker.run(queued, cancelled, settings).await;
        (worker, result)
    });
    for bytes in [b"first\r\n".as_slice(), b"second"] {
        output
            .send(LocalShellEvent::Output(
                crate::terminal::TerminalOutputChunk::new(bytes.to_vec()),
            ))
            .await
            .unwrap();
    }
    settle().await;
    assert_eq!(output.capacity(), 8);
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert_eq!(
        session::tests::terminal_text(&client.view().unwrap()).trim(),
        ""
    );
    client
        .copy(
            crate::terminal::TerminalSelectionRange {
                start_row: 0,
                start_column: 0,
                end_row: 0,
                end_column: 5,
            },
            before.selection_revision,
        )
        .unwrap();
    settle().await;
    assert_eq!(client.take_clipboard().unwrap().as_deref(), Some("first"));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(20)).await;
    settle().await;
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    client.focus(true).unwrap();
    settle().await;
    let focused = client.view().unwrap();
    assert!(session::tests::terminal_text(&focused).contains("first"));
    assert!(session::tests::terminal_text(&focused).contains("second"));
    assert_eq!(focused.selection_revision, before.selection_revision);
    assert_eq!(focused.terminal.dirty_rows, [0, 1]);
    assert_eq!(wakes.load(Ordering::SeqCst), 2);

    client.focus(false).unwrap();
    output
        .send(LocalShellEvent::Output(
            crate::terminal::TerminalOutputChunk::new(b"\r\nthird".to_vec()),
        ))
        .await
        .unwrap();
    settle().await;
    tokio::time::advance(Duration::from_millis(30)).await;
    settle().await;
    assert_eq!(wakes.load(Ordering::SeqCst), 2);
    updates.send_modify(|settings| settings.refresh = Rates::new(60, 100));
    settle().await;
    assert!(session::tests::terminal_text(&client.view().unwrap()).contains("third"));
    // A committed settings update also wakes the clipboard permission UI.
    assert_eq!(wakes.load(Ordering::SeqCst), 4);
    output
        .send(LocalShellEvent::Output(
            crate::terminal::TerminalOutputChunk::new(b"\x1b[?2026h\r\nheld".to_vec()),
        ))
        .await
        .unwrap();
    settle().await;
    tokio::time::advance(Duration::from_millis(20)).await;
    settle().await;
    assert_eq!(wakes.load(Ordering::SeqCst), 4);
    assert!(!session::tests::terminal_text(&client.view().unwrap()).contains("held"));
    output
        .send(LocalShellEvent::Output(
            crate::terminal::TerminalOutputChunk::new(b"\x1b[?2026l".to_vec()),
        ))
        .await
        .unwrap();
    settle().await;
    assert!(session::tests::terminal_text(&client.view().unwrap()).contains("held"));
    assert_eq!(wakes.load(Ordering::SeqCst), 5);
    tokio::time::advance(Duration::from_secs(30)).await;
    settle().await;
    assert_eq!(wakes.load(Ordering::SeqCst), 5);
    stop.send_replace(true);
    let (_, result) = task.await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn focus_hint_survives_overload_and_stale_connection_reports_remain_rejected() {
    let (mut worker, mut client) = session::clipboard::tests::fixture(false);
    let (commands, mut queued) = mpsc::channel(1);
    client.commands = commands;
    worker.model.process(b"\x1b[?1004h");
    let stale = client.for_view(&client.view().unwrap());
    worker.generation += 1;
    stale.focus(true).unwrap();
    assert!(stale.focus(false).is_err());
    assert!(!*worker.focus.borrow_and_update());
    assert!(stale.focus(true).is_err());
    assert!(*worker.focus.borrow_and_update());
    // A stale DEC1004 command must not reach the missing transport, even though
    // the presentation hint independently follows the latest UI focus.
    worker.dispatch(queued.recv().await.unwrap()).await.unwrap();
    assert!(worker.status.is_empty());
    assert!(worker.transport.is_none());
}

#[test]
fn repeated_presentation_hints_do_not_queue_protocol_reports_or_repeat_notifications() {
    let (mut worker, mut client) = session::clipboard::tests::fixture(false);
    let (commands, mut queued) = mpsc::channel(1);
    client.commands = commands;
    let stale = client.for_view(&client.view().unwrap());
    worker.generation += 1;
    stale.presentation_focus(true);
    assert!(*worker.focus.borrow_and_update());
    for _ in 0..100 {
        stale.presentation_focus(true);
    }
    assert!(!worker.focus.has_changed().unwrap());
    // Also clear independently of the cached protocol focus/connection state.
    stale.presentation_focus(false);
    assert!(!*worker.focus.borrow_and_update());
    assert!(matches!(
        queued.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}
