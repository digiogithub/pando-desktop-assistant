//! Service to UI event streaming with batched notifications.
//!
//! Services (tokio tasks, background threads, the voice and Pando clients) push events into an
//! `async_channel`; an [`EventPump`] waits for the receiver on GPUI's foreground executor,
//! drains every event already queued (a burst), applies them to a view in order and calls
//! `cx.notify()` once per batch, so a flood of SSE deltas costs one re-render, not one each.
//!
//! Pattern from Bitácora's `events.rs` (MIT), extended with burst draining.

use async_channel::Receiver;

use crate::ui::{Context, Task};

/// Upper bound of events applied between two notifications, so a never-ending producer cannot
/// starve rendering.
pub const MAX_BATCH: usize = 256;

/// Namespace for the pump constructor.
#[derive(Debug)]
pub struct EventPump;

impl EventPump {
    /// Spawns a foreground task that applies every event received on `rx` to the view being
    /// constructed or updated through `cx`, then calls `cx.notify()` once per drained burst.
    ///
    /// The task holds the view weakly and ends when the channel closes (all senders dropped)
    /// or when the view has been released. The view should store the returned [`Task`]:
    /// dropping it cancels the pump.
    pub fn attach<V, E>(
        cx: &mut Context<V>,
        rx: Receiver<E>,
        apply: impl Fn(&mut V, E, &mut Context<V>) + 'static,
    ) -> Task<()>
    where
        V: 'static,
        E: 'static,
    {
        cx.spawn(async move |this, cx| {
            while let Ok(first) = rx.recv().await {
                let mut batch = vec![first];
                while batch.len() < MAX_BATCH {
                    match rx.try_recv() {
                        Ok(event) => batch.push(event),
                        Err(_) => break,
                    }
                }
                let alive = this.update(cx, |view, cx| {
                    for event in batch {
                        apply(view, event, cx);
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use crate::ui::{AppContext as _, Entity, Render};

    struct Counter {
        seen: Vec<u32>,
        renders: usize,
        pump: Option<Task<()>>,
    }

    impl Counter {
        fn new(rx: Receiver<u32>, cx: &mut Context<Self>) -> Self {
            let pump = EventPump::attach(cx, rx, |this: &mut Self, event, _| {
                this.seen.push(event);
            });
            Self {
                seen: Vec::new(),
                renders: 0,
                pump: Some(pump),
            }
        }
    }

    impl Render for Counter {
        fn render(
            &mut self,
            _: &mut crate::ui::Window,
            _: &mut Context<Self>,
        ) -> impl crate::ui::IntoElement {
            self.renders += 1;
            crate::ui::div()
        }
    }

    #[gpui_test]
    fn applies_events_in_order(cx: &mut TestAppContext) {
        let (tx, rx) = async_channel::unbounded();
        let view: Entity<Counter> = cx.new(|cx| Counter::new(rx, cx));
        for n in 0..5 {
            tx.try_send(n).expect("send");
        }
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |c, _| c.seen.clone()),
            vec![0, 1, 2, 3, 4]
        );
        tx.try_send(5).expect("send");
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |c, _| c.seen.len()), 6);
    }

    #[gpui_test]
    fn a_burst_produces_one_notification(cx: &mut TestAppContext) {
        let (tx, rx) = async_channel::unbounded();
        let notified = std::rc::Rc::new(std::cell::Cell::new(0_usize));
        let view: Entity<Counter> = cx.new(|cx| Counter::new(rx, cx));
        let counter = notified.clone();
        let _sub = cx.update(|cx| cx.observe(&view, move |_, _| counter.set(counter.get() + 1)));
        for n in 0..50 {
            tx.try_send(n).expect("send");
        }
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |c, _| c.seen.len()), 50);
        assert_eq!(notified.get(), 1, "50 queued events must notify once");
    }

    #[gpui_test]
    fn pump_ends_without_panic_when_the_view_is_released(cx: &mut TestAppContext) {
        let (tx, rx) = async_channel::unbounded();
        let view: Entity<Counter> = cx.new(|cx| Counter::new(rx, cx));
        // Detach the pump so it outlives the view and must notice the release itself.
        view.update(cx, |c, _| {
            if let Some(task) = c.pump.take() {
                task.detach();
            }
        });
        drop(view);
        cx.run_until_parked();
        tx.try_send(1).expect("send");
        cx.run_until_parked();
        assert!(tx.try_send(2).is_err());
    }

    #[gpui_test]
    fn pump_stops_when_all_senders_drop(cx: &mut TestAppContext) {
        let (tx, rx) = async_channel::unbounded::<u32>();
        let view: Entity<Counter> = cx.new(|cx| Counter::new(rx, cx));
        drop(tx);
        cx.run_until_parked();
        assert!(view.read_with(cx, |c, _| c.seen.is_empty()));
    }
}
