//! Standard Rust streams over the same watches exported to other languages.
use super::{InventoryState, InventoryWatch, ObserverError};

macro_rules! impl_stream {
    ($watch:ty, $item:ty) => {
        impl $watch {
            /// Owns this watch as a standard stream. Dropping the stream releases
            /// its demand, including while the next item is pending.
            #[must_use]
            pub fn into_stream(self) -> n0_future::boxed::BoxStream<Result<$item, ObserverError>> {
                Box::pin(n0_future::stream::unfold(Some(self), |watch| async move {
                    let watch = watch?;
                    match watch.next().await {
                        Ok(Some(item)) => Some((Ok(item), Some(watch))),
                        Ok(None) => None,
                        Err(error) => Some((Err(error), None)),
                    }
                }))
            }
        }
    };
}
impl_stream!(InventoryWatch, InventoryState);
