use super::{
    feed::Feed,
    listener::{Listener, Subscription},
};
use crate::raw::ClientError;
use parking_lot::Mutex;
use protocol::v1 as wire;
use std::sync::{Arc, Weak};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(crate) struct Manager {
    feeds: Mutex<Vec<Weak<Feed>>>,
}

impl Manager {
    pub(crate) fn close(&self) {
        let feeds = self
            .feeds
            .lock()
            .drain(..)
            .filter_map(|feed| feed.upgrade())
            .collect::<Vec<_>>();
        for feed in feeds {
            feed.finish(&Err(ClientError::Closed));
        }
    }

    pub(crate) async fn subscribe(
        &self,
        mut request: wire::Subscribe,
        receive_events: bool,
        rpc: irpc::Client<wire::ObserverProtocolV1>,
        closed: CancellationToken,
    ) -> Result<Subscription, ClientError> {
        if closed.is_cancelled() {
            return Err(ClientError::Closed);
        }
        request.topics.sort();
        request.topics.dedup();
        if request.topics.is_empty() {
            return Err(ClientError::Request(wire::RequestError::InvalidRequest {
                message: "empty topic selection".into(),
            }));
        }
        let subscription = Subscription {
            inner: Arc::new(Listener::new(receive_events)),
        };
        let mut selected = Vec::new();
        for topic in request.topics {
            let selection = wire::Subscribe {
                sessions: request.sessions.clone(),
                topics: vec![topic],
            };
            let mut feeds = self.feeds.lock();
            if closed.is_cancelled() {
                return Err(ClientError::Closed);
            }
            feeds.retain(|feed| feed.upgrade().is_some_and(|feed| !feed.stop.is_cancelled()));
            let existing = feeds
                .iter()
                .filter_map(Weak::upgrade)
                .find(|feed| feed.selection == selection && feed.attach(&subscription.inner));
            let (feed, start) = existing.map_or_else(
                || {
                    let feed = Arc::new(Feed::new(selection));
                    feed.attach(&subscription.inner);
                    (feed, true)
                },
                |feed| (feed, false),
            );
            if !subscription.inner.register(feed.clone()) {
                feed.detach(&subscription.inner);
                subscription.inner.check_open()?;
                return Err(ClientError::Closed);
            }
            if start {
                feeds.push(Arc::downgrade(&feed));
                n0_future::task::spawn(feed.clone().run(rpc.clone(), closed.clone()));
            }
            selected.push(feed);
        }
        for feed in selected {
            tokio::select! {
                biased;
                () = closed.cancelled() => return Err(ClientError::Closed),
                result = feed.wait_ready() => {
                    subscription.inner.check_open()?;
                    result?;
                },
            }
        }
        subscription.inner.check_open()?;
        Ok(subscription)
    }
}
