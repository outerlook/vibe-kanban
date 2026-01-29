//! Domain event handlers.
//!
//! This module contains handlers that react to domain events like task status
//! changes, execution completions, and workspace lifecycle events.

mod feedback_collection;

pub use feedback_collection::FeedbackCollectionHandler;
