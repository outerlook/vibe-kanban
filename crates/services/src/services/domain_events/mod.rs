//! Domain events module for application-side effects and external workflow publication.

mod dispatcher;
mod handler;
pub mod handlers;
mod orchestration;
mod types;

pub use dispatcher::{DispatcherBuilder, DomainEventDispatcher};
pub use handler::{EventHandler, ExecutionMode, HandlerContext, HandlerError};
pub use handlers::{
    NotificationHandler, OrchestrationEventPublisherHandler, RemoteSyncHandler,
    WebSocketBroadcastHandler,
};
pub use orchestration::{
    ApprovalRequestedEventPayload, ApprovalResolvedEventPayload,
    ConversationMessageAddedEventPayload, DEFAULT_ORCHESTRATION_EVENT_SCHEMA_VERSION,
    ExecutionCompletedEventPayload, ExecutionStartedEventPayload,
    FollowUpTransitionEventPayload, MergeQueueTransitionEventPayload,
    MqttOrchestrationEventPublisher, MqttOrchestrationPublisherConfig,
    OrchestrationEmptyPayload, OrchestrationEventEnvelope, OrchestrationEventMapper,
    OrchestrationEventPayload, OrchestrationEventPublisher,
    OrchestrationEventPublisherConfig, OrchestrationEventPublisherHandle,
    OrchestrationEventType, ProjectUpdatedEventPayload,
    RecordingOrchestrationEventPublisher, TaskGroupCompletedEventPayload,
    TaskGroupTransitionEventPayload, TaskLifecycleEventPayload,
    TaskStatusChangedEventPayload, WorkspaceCreatedEventPayload,
    build_mqtt_orchestration_event_publisher, default_topic_namespace,
};
pub use types::{
    ApprovalEventKind, ApprovalResolution, ConversationMessageEventRole, DomainEvent,
    DomainEventEntityIds, EventDispatchCallback, FollowUpQueueKind, FollowUpScope,
    FollowUpTransitionState, MergeQueueTransitionState,
    TaskGroupTransitionAction, TaskLifecycleAction,
};
