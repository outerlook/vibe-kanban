//! Domain event handlers.
//!
//! This module contains handlers that react to domain events like task status
//! changes, execution completions, and workspace lifecycle events.

mod autopilot;
mod feedback_collection;
mod hook_execution_updater;
mod notifications;
mod remote_sync;
mod review_attention;
mod websocket_broadcast;

pub use autopilot::AutopilotHandler;
pub use feedback_collection::FeedbackCollectionHandler;
pub use hook_execution_updater::HookExecutionUpdaterHandler;
pub use notifications::NotificationHandler;
pub use remote_sync::RemoteSyncHandler;
pub use review_attention::ReviewAttentionHandler;
pub use websocket_broadcast::WebSocketBroadcastHandler;
