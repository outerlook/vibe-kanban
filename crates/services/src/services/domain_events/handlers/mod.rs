//! Domain event handlers.
//!
//! This module contains handlers that react to domain events like task status
//! changes, execution completions, and workspace lifecycle events.

mod autopilot;
mod feedback_collection;
mod notifications;
mod remote_sync;
mod review_attention;
mod websocket_broadcast;

pub use autopilot::AutopilotHandler;
pub use feedback_collection::FeedbackCollectionHandler;
pub use notifications::NotificationHandler;
pub use remote_sync::RemoteSyncHandler;
pub use review_attention::ReviewAttentionHandler;
pub use websocket_broadcast::WebSocketBroadcastHandler;
