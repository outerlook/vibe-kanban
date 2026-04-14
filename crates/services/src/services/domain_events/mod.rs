//! Domain events module for the event-driven hook system.
//!
//! This module defines the core types for domain events and hook points
//! that can be used to trigger custom actions at specific points in the
//! application lifecycle.

mod dispatcher;
mod handler;
pub mod handlers;
mod hook_execution;
mod hook_points;
mod orchestration;
mod types;

pub use dispatcher::{DispatcherBuilder, DomainEventDispatcher};
pub use handler::{EventHandler, ExecutionMode, HandlerContext, HandlerError};
pub use handlers::{
    AutopilotHandler, FeedbackCollectionHandler, HookExecutionUpdaterHandler, NotificationHandler,
    OrchestrationEventPublisherHandler, RemoteSyncHandler, ReviewAttentionHandler,
    WebSocketBroadcastHandler,
};
pub use hook_execution::{HookExecution, HookExecutionStatus, HookExecutionStore};
pub use hook_points::HookPoint;
pub use orchestration::{
    DEFAULT_ORCHESTRATION_EVENT_SCHEMA_VERSION, MqttOrchestrationEventPublisher,
    MqttOrchestrationPublisherConfig, OrchestrationEventEnvelope, OrchestrationEventMapper,
    OrchestrationEventPublisher, OrchestrationEventPublisherConfig,
    OrchestrationEventPublisherHandle, OrchestrationEventType,
    RecordingOrchestrationEventPublisher, build_mqtt_orchestration_event_publisher,
    default_topic_namespace,
};
pub use types::{
    ApprovalEventKind, ApprovalResolution, ConversationMessageEventRole, DomainEvent,
    DomainEventEntityIds, EventDispatchCallback, ExecutionTrigger, ExecutionTriggerCallback,
    FollowUpQueueKind, FollowUpScope, FollowUpTransitionState, MergeQueueTransitionState,
    TaskGroupTransitionAction, TaskLifecycleAction,
};
