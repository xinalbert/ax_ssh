use super::*;
use std::sync::{Condvar, Mutex, atomic::AtomicUsize};
use tokio::sync::Notify;

#[test]
fn required_fallbacks_and_style_aliases_exist() {
    let definitions = load().unwrap();
    for style in STYLES {
        let chain = &definitions.families[&FontFamily::Name(format!("terminal-{style}").into())];
        assert_eq!(chain.len(), 3);
        assert!(
            chain
                .iter()
                .all(|name| definitions.font_data.contains_key(name))
        );
    }
}

struct GatedLoader {
    loader: Loader,
    entered: Arc<Notify>,
    gate: Arc<(Mutex<bool>, Condvar)>,
    calls: Arc<AtomicUsize>,
}

fn gated_loader() -> GatedLoader {
    let mut loader = Loader::bundled().unwrap();
    let entered = Arc::new(Notify::new());
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let (notification, release, counter) = (entered.clone(), gate.clone(), calls.clone());
    loader.before_load = Some(Arc::new(move || {
        if counter.fetch_add(1, Ordering::AcqRel) == 0 {
            notification.notify_one();
            let (lock, changed) = &*release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = changed.wait(released).unwrap();
            }
        }
    }));
    GatedLoader {
        loader,
        entered,
        gate,
        calls,
    }
}

fn release(gate: &Arc<(Mutex<bool>, Condvar)>) {
    *gate.0.lock().unwrap() = true;
    gate.1.notify_one();
}

#[tokio::test]
async fn rapid_font_changes_coalesce_and_only_latest_result_reaches_the_gui() {
    let GatedLoader {
        loader,
        entered,
        gate,
        calls,
    } = gated_loader();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = wakes.clone();
    let (owner, mut client) = FontService::spawn(
        &Handle::current(),
        loader,
        Arc::new(move || {
            counter.fetch_add(1, Ordering::AcqRel);
        }),
    );
    let ctx = egui::Context::default();
    let mut appearance = AppearanceSettings::default();
    client.update(&ctx, &appearance).unwrap();
    appearance.terminal_font_family = "Iosevka Term".into();
    client.update(&ctx, &appearance).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    appearance.terminal_font_family = "Maple Mono NF CN".into();
    client.update(&ctx, &appearance).unwrap();
    appearance.terminal_font_family = "Monaspace Neon Var".into();
    client.update(&ctx, &appearance).unwrap();
    assert!(view(&ctx).pending);
    assert_eq!(view(&ctx).terminal, BUNDLED_UI_FONT_FAMILY);
    release(&gate);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            client.results.changed().await.unwrap();
            if client.results.borrow().request.revision == client.requested.revision {
                break;
            }
        }
    })
    .await
    .unwrap();
    client.update(&ctx, &appearance).unwrap();
    assert_eq!(view(&ctx).terminal, "Monaspace Neon Var");
    assert!(!view(&ctx).pending);
    assert_eq!(calls.load(Ordering::Acquire), 2);
    assert_eq!(wakes.load(Ordering::Acquire), 1);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_and_joins_the_in_flight_blocking_job() {
    let GatedLoader {
        loader,
        entered,
        gate,
        ..
    } = gated_loader();
    let (owner, mut client) = FontService::spawn(&Handle::current(), loader, Arc::new(|| {}));
    let appearance = AppearanceSettings {
        terminal_font_family: "Iosevka Term".into(),
        ..Default::default()
    };
    client
        .update(&egui::Context::default(), &appearance)
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    let cancelled = owner.cancelled.clone();
    let shutdown = tokio::spawn(owner.shutdown());
    while !cancelled.load(Ordering::Acquire) {
        tokio::task::yield_now().await;
    }
    assert!(!shutdown.is_finished());
    release(&gate);
    tokio::time::timeout(std::time::Duration::from_secs(5), shutdown)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(client.results.borrow().request.revision, 0);
}
