//! Domain events module for the event-driven hook system.
//!
//! This module defines the core types for domain events and hook points
//! that can be used to trigger custom actions at specific points in the
//! application lifecycle.

mod dispatcher;
mod handler;
pub mod handlers;
mod hook_points;
mod types;

pub use dispatcher::{DispatcherBuilder, DomainEventDispatcher};
pub use handler::{EventHandler, ExecutionMode, HandlerContext, HandlerError};
pub use handlers::{
    AutopilotHandler, FeedbackCollectionHandler, NotificationHandler, RemoteSyncHandler,
    WebSocketBroadcastHandler,
};
pub use hook_points::HookPoint;
pub use types::DomainEvent;
