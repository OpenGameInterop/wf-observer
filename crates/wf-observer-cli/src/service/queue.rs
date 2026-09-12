//! Bounded subscription data and independent, prioritized terminal state.

use parking_lot::Mutex;
use protocol::v1 as wire;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::Notify;

pub(super) struct Queue {
    state: Mutex<Contents>,
    pub(super) ready: Notify,
    message_bytes: u32,
}

struct Contents {
    initial: VecDeque<wire::SubscriptionItem>,
    updates: VecDeque<(wire::SubscriptionItem, usize)>,
    bytes: usize,
    terminal: Option<wire::SubscriptionEnd>,
    completed: bool,
}

impl Queue {
    pub(super) fn new(message_bytes: u32) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(Contents {
                initial: VecDeque::new(),
                updates: VecDeque::new(),
                bytes: 0,
                terminal: None,
                completed: false,
            }),
            ready: Notify::new(),
            message_bytes,
        })
    }

    pub(super) fn offer(&self, item: wire::SubscriptionItem) -> bool {
        let mut queue = self.state.lock();
        if queue.terminal.is_some() || queue.completed {
            return false;
        }
        let size = postcard::experimental::serialized_size(&Ok::<_, wire::RequestError>(&item))
            .unwrap_or(usize::MAX);
        let budget = self.message_bytes as usize;
        let exceeded = if size > budget {
            Some(wire::Resource::MessageBytes)
        } else if size > budget.saturating_sub(queue.bytes) {
            Some(wire::Resource::QueuedBytes)
        } else {
            None
        };
        if let Some(resource) = exceeded {
            queue.discard();
            queue.terminal = Some(wire::SubscriptionEnd::ResyncRequired {
                reason: wire::ResyncReason::Lagged { resource },
            });
            self.ready.notify_one();
            return false;
        }
        queue.bytes += size;
        queue.updates.push_back((item, size));
        self.ready.notify_one();
        true
    }

    pub(super) fn finish(&self, end: wire::SubscriptionEnd, discard: bool) {
        let mut queue = self.state.lock();
        if queue.terminal.is_some() || queue.completed {
            return;
        }
        if discard {
            queue.discard();
        }
        queue.terminal = Some(end);
        self.ready.notify_one();
    }

    pub(super) fn ended(&self) -> bool {
        let queue = self.state.lock();
        queue.terminal.is_some() || queue.completed
    }

    pub(super) fn initial(&self, initial: VecDeque<wire::SubscriptionItem>) {
        self.state.lock().initial = initial;
    }

    pub(super) fn pop(&self) -> (Option<wire::SubscriptionItem>, bool) {
        let mut queue = self.state.lock();
        if let Some(item) = queue.initial.pop_front() {
            return (Some(item), false);
        }
        if let Some((item, bytes)) = queue.updates.pop_front() {
            queue.bytes -= bytes;
            return (Some(item), false);
        }
        if let Some(end) = queue.terminal.take() {
            queue.completed = true;
            return (Some(wire::SubscriptionItem::Closed(end)), false);
        }
        (None, queue.completed)
    }
}

impl Contents {
    fn discard(&mut self) {
        // Preserve an unsent Begin so even an early terminal has a valid prefix.
        let begin = self
            .initial
            .pop_front()
            .filter(|item| matches!(item, wire::SubscriptionItem::Begin(_)));
        self.initial.clear();
        self.initial.extend(begin);
        self.updates.clear();
        self.bytes = 0;
    }
}
