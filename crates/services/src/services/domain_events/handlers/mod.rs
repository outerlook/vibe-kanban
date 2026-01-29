//! Domain event handlers.
//!
//! This module contains handlers that react to domain events like task status
//! changes, execution completions, and workspace lifecycle events.

mod feedback_collection;
mod remote_sync;

pub use feedback_collection::FeedbackCollectionHandler;
pub use remote_sync::RemoteSyncHandler;
