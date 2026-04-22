use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use db::models::{
    task::{Task, TaskStatus},
    task_group::TaskGroup,
};
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::time::sleep;
use ts_rs::TS;
use uuid::Uuid;

use super::{DomainEvent, TaskGroupTransitionAction, TaskLifecycleAction};

pub const DEFAULT_ORCHESTRATION_EVENT_SCHEMA_VERSION: &str = "vk_orchestration_v1";

pub fn default_topic_namespace() -> String {
    "vk/orchestration".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub enum OrchestrationSchemaVersion {
    #[serde(rename = "vk_orchestration_v1")]
    #[ts(rename = "vk_orchestration_v1")]
    V1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationEventType {
    TaskCreated,
    TaskUpdated,
    TaskDeleted,
    TaskStatusChanged,
    ExecutionStarted,
    ExecutionCompleted,
    WorkspaceCreated,
    WorkspaceDeleted,
    ProjectUpdated,
    ApprovalRequested,
    ApprovalResolved,
    ConversationMessageAdded,
    FollowUpTransition,
    MergeQueueTransition,
    TaskGroupTransition,
    TaskGroupCompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct OrchestrationEventMetadata {
    pub event_id: Uuid,
    pub schema_version: OrchestrationSchemaVersion,
    pub occurred_at: DateTime<Utc>,
    pub task_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub execution_process_id: Option<Uuid>,
    pub task_group_id: Option<Uuid>,
}

impl OrchestrationEventMetadata {
    fn new(event: &DomainEvent) -> Self {
        let ids = event.entity_ids();

        Self {
            event_id: Uuid::new_v4(),
            schema_version: OrchestrationSchemaVersion::V1,
            occurred_at: event.occurred_at(),
            task_id: ids.task_id,
            workspace_id: ids.workspace_id,
            session_id: ids.session_id,
            execution_process_id: ids.execution_process_id,
            task_group_id: ids.task_group_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum OrchestrationEventEnvelope {
    TaskCreated {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: TaskLifecycleEventPayload,
    },
    TaskUpdated {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: TaskLifecycleEventPayload,
    },
    TaskDeleted {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: TaskLifecycleEventPayload,
    },
    TaskStatusChanged {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: TaskStatusChangedEventPayload,
    },
    ExecutionStarted {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: ExecutionStartedEventPayload,
    },
    ExecutionCompleted {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: ExecutionCompletedEventPayload,
    },
    WorkspaceCreated {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: WorkspaceCreatedEventPayload,
    },
    WorkspaceDeleted {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: OrchestrationEmptyPayload,
    },
    ProjectUpdated {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: ProjectUpdatedEventPayload,
    },
    ApprovalRequested {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: ApprovalRequestedEventPayload,
    },
    ApprovalResolved {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: ApprovalResolvedEventPayload,
    },
    ConversationMessageAdded {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: ConversationMessageAddedEventPayload,
    },
    FollowUpTransition {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: FollowUpTransitionEventPayload,
    },
    MergeQueueTransition {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: MergeQueueTransitionEventPayload,
    },
    TaskGroupTransition {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: TaskGroupTransitionEventPayload,
    },
    TaskGroupCompleted {
        #[serde(flatten)]
        #[ts(flatten)]
        metadata: OrchestrationEventMetadata,
        payload: TaskGroupCompletedEventPayload,
    },
}

impl OrchestrationEventEnvelope {
    fn new(
        event: &DomainEvent,
        event_type: OrchestrationEventType,
        payload: OrchestrationEventPayload,
    ) -> Self {
        let metadata = OrchestrationEventMetadata::new(event);

        match (event_type, payload) {
            (
                OrchestrationEventType::TaskCreated,
                OrchestrationEventPayload::TaskLifecycle(payload),
            ) => Self::TaskCreated { metadata, payload },
            (
                OrchestrationEventType::TaskUpdated,
                OrchestrationEventPayload::TaskLifecycle(payload),
            ) => Self::TaskUpdated { metadata, payload },
            (
                OrchestrationEventType::TaskDeleted,
                OrchestrationEventPayload::TaskLifecycle(payload),
            ) => Self::TaskDeleted { metadata, payload },
            (
                OrchestrationEventType::TaskStatusChanged,
                OrchestrationEventPayload::TaskStatusChanged(payload),
            ) => Self::TaskStatusChanged { metadata, payload },
            (
                OrchestrationEventType::ExecutionStarted,
                OrchestrationEventPayload::ExecutionStarted(payload),
            ) => Self::ExecutionStarted { metadata, payload },
            (
                OrchestrationEventType::ExecutionCompleted,
                OrchestrationEventPayload::ExecutionCompleted(payload),
            ) => Self::ExecutionCompleted { metadata, payload },
            (
                OrchestrationEventType::WorkspaceCreated,
                OrchestrationEventPayload::WorkspaceCreated(payload),
            ) => Self::WorkspaceCreated { metadata, payload },
            (
                OrchestrationEventType::WorkspaceDeleted,
                OrchestrationEventPayload::WorkspaceDeleted(payload),
            ) => Self::WorkspaceDeleted { metadata, payload },
            (
                OrchestrationEventType::ProjectUpdated,
                OrchestrationEventPayload::ProjectUpdated(payload),
            ) => Self::ProjectUpdated { metadata, payload },
            (
                OrchestrationEventType::ApprovalRequested,
                OrchestrationEventPayload::ApprovalRequested(payload),
            ) => Self::ApprovalRequested { metadata, payload },
            (
                OrchestrationEventType::ApprovalResolved,
                OrchestrationEventPayload::ApprovalResolved(payload),
            ) => Self::ApprovalResolved { metadata, payload },
            (
                OrchestrationEventType::ConversationMessageAdded,
                OrchestrationEventPayload::ConversationMessageAdded(payload),
            ) => Self::ConversationMessageAdded { metadata, payload },
            (
                OrchestrationEventType::FollowUpTransition,
                OrchestrationEventPayload::FollowUpTransition(payload),
            ) => Self::FollowUpTransition { metadata, payload },
            (
                OrchestrationEventType::MergeQueueTransition,
                OrchestrationEventPayload::MergeQueueTransition(payload),
            ) => Self::MergeQueueTransition { metadata, payload },
            (
                OrchestrationEventType::TaskGroupTransition,
                OrchestrationEventPayload::TaskGroupTransition(payload),
            ) => Self::TaskGroupTransition { metadata, payload },
            (
                OrchestrationEventType::TaskGroupCompleted,
                OrchestrationEventPayload::TaskGroupCompleted(payload),
            ) => Self::TaskGroupCompleted { metadata, payload },
            (event_type, payload) => unreachable!(
                "orchestration event type {event_type:?} does not match payload {payload:?}"
            ),
        }
    }

    pub fn event_type(&self) -> OrchestrationEventType {
        match self {
            Self::TaskCreated { .. } => OrchestrationEventType::TaskCreated,
            Self::TaskUpdated { .. } => OrchestrationEventType::TaskUpdated,
            Self::TaskDeleted { .. } => OrchestrationEventType::TaskDeleted,
            Self::TaskStatusChanged { .. } => OrchestrationEventType::TaskStatusChanged,
            Self::ExecutionStarted { .. } => OrchestrationEventType::ExecutionStarted,
            Self::ExecutionCompleted { .. } => OrchestrationEventType::ExecutionCompleted,
            Self::WorkspaceCreated { .. } => OrchestrationEventType::WorkspaceCreated,
            Self::WorkspaceDeleted { .. } => OrchestrationEventType::WorkspaceDeleted,
            Self::ProjectUpdated { .. } => OrchestrationEventType::ProjectUpdated,
            Self::ApprovalRequested { .. } => OrchestrationEventType::ApprovalRequested,
            Self::ApprovalResolved { .. } => OrchestrationEventType::ApprovalResolved,
            Self::ConversationMessageAdded { .. } => {
                OrchestrationEventType::ConversationMessageAdded
            }
            Self::FollowUpTransition { .. } => OrchestrationEventType::FollowUpTransition,
            Self::MergeQueueTransition { .. } => OrchestrationEventType::MergeQueueTransition,
            Self::TaskGroupTransition { .. } => OrchestrationEventType::TaskGroupTransition,
            Self::TaskGroupCompleted { .. } => OrchestrationEventType::TaskGroupCompleted,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct OrchestrationEmptyPayload {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct TaskLifecycleEventPayload {
    pub project_id: Uuid,
    pub status: TaskStatus,
    pub previous_task_group_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct TaskStatusChangedEventPayload {
    pub project_id: Uuid,
    pub status: TaskStatus,
    pub previous_status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ExecutionStartedEventPayload {
    pub status: db::models::execution_process::ExecutionProcessStatus,
    pub run_reason: db::models::execution_process::ExecutionProcessRunReason,
    pub conversation_session_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ExecutionCompletedEventPayload {
    pub status: db::models::execution_process::ExecutionProcessStatus,
    pub run_reason: db::models::execution_process::ExecutionProcessRunReason,
    pub exit_code: Option<i64>,
    pub conversation_session_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct WorkspaceCreatedEventPayload {
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ProjectUpdatedEventPayload {
    pub project_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ApprovalRequestedEventPayload {
    pub approval_id: String,
    pub kind: super::ApprovalEventKind,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub question_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ApprovalResolvedEventPayload {
    pub approval_id: String,
    pub resolution: super::ApprovalResolution,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ConversationMessageAddedEventPayload {
    pub conversation_session_id: Uuid,
    pub message_id: Uuid,
    pub execution_process_id: Option<Uuid>,
    pub role: super::ConversationMessageEventRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct FollowUpTransitionEventPayload {
    pub state: super::FollowUpTransitionState,
    pub scope: super::FollowUpScope,
    pub queue_kind: Option<super::FollowUpQueueKind>,
    pub execution_process_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct MergeQueueTransitionEventPayload {
    pub entry_id: Uuid,
    pub project_id: Uuid,
    pub repo_id: Uuid,
    pub state: super::MergeQueueTransitionState,
    pub merge_commit: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct TaskGroupTransitionEventPayload {
    pub action: super::TaskGroupTransitionAction,
    pub project_id: Uuid,
    pub task_group_id: Option<Uuid>,
    pub previous_task_group_id: Option<Uuid>,
    pub task_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct TaskGroupCompletedEventPayload {
    pub project_id: Uuid,
    pub completed_task_ids: Vec<Uuid>,
    pub terminal_task_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(untagged)]
pub enum OrchestrationEventPayload {
    TaskLifecycle(TaskLifecycleEventPayload),
    TaskStatusChanged(TaskStatusChangedEventPayload),
    ExecutionStarted(ExecutionStartedEventPayload),
    ExecutionCompleted(ExecutionCompletedEventPayload),
    WorkspaceCreated(WorkspaceCreatedEventPayload),
    WorkspaceDeleted(OrchestrationEmptyPayload),
    ProjectUpdated(ProjectUpdatedEventPayload),
    ApprovalRequested(ApprovalRequestedEventPayload),
    ApprovalResolved(ApprovalResolvedEventPayload),
    ConversationMessageAdded(ConversationMessageAddedEventPayload),
    FollowUpTransition(FollowUpTransitionEventPayload),
    MergeQueueTransition(MergeQueueTransitionEventPayload),
    TaskGroupTransition(TaskGroupTransitionEventPayload),
    TaskGroupCompleted(TaskGroupCompletedEventPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct MqttOrchestrationPublisherConfig {
    pub broker_url: String,
    #[serde(default = "default_topic_namespace")]
    pub topic_namespace: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub qos: u8,
    #[serde(default)]
    pub retain: bool,
}

impl MqttOrchestrationPublisherConfig {
    pub fn validate(&self) -> Result<()> {
        if self.broker_url.trim().is_empty() {
            return Err(anyhow!(
                "orchestration publisher MQTT broker_url is required"
            ));
        }
        if self.topic_namespace.trim().is_empty() {
            return Err(anyhow!(
                "orchestration publisher MQTT topic_namespace is required"
            ));
        }
        if self
            .topic_namespace
            .split('/')
            .any(|segment| segment.trim().is_empty())
        {
            return Err(anyhow!(
                "orchestration publisher MQTT topic_namespace cannot contain empty segments"
            ));
        }
        if self.qos > 2 {
            return Err(anyhow!(
                "orchestration publisher MQTT qos must be 0, 1, or 2"
            ));
        }

        Ok(())
    }

    pub fn topic_for(&self, event_type: &OrchestrationEventType) -> String {
        format!(
            "{}/{}",
            self.topic_namespace.trim_end_matches('/'),
            serde_json::to_string(event_type)
                .expect("event type serialization cannot fail")
                .trim_matches('"')
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct OrchestrationEventPublisherConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mqtt: Option<MqttOrchestrationPublisherConfig>,
}

impl OrchestrationEventPublisherConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            if let Some(mqtt) = &self.mqtt {
                mqtt.validate()?;
            }
            return Ok(());
        }

        let mqtt = self
            .mqtt
            .as_ref()
            .ok_or_else(|| anyhow!("orchestration publisher requires an MQTT configuration"))?;
        mqtt.validate()
    }

    pub fn mqtt(&self) -> Option<&MqttOrchestrationPublisherConfig> {
        self.mqtt.as_ref()
    }
}

#[async_trait]
pub trait OrchestrationEventPublisher: Send + Sync {
    async fn publish(&self, topic: String, envelope: OrchestrationEventEnvelope) -> Result<()>;
}

pub type OrchestrationEventPublisherHandle = Arc<dyn OrchestrationEventPublisher>;

#[derive(Clone, Default)]
pub struct RecordingOrchestrationEventPublisher {
    published: Arc<Mutex<Vec<(String, OrchestrationEventEnvelope)>>>,
}

impl RecordingOrchestrationEventPublisher {
    pub fn published(&self) -> Vec<(String, OrchestrationEventEnvelope)> {
        self.published
            .lock()
            .expect("publisher mutex poisoned")
            .clone()
    }
}

#[async_trait]
impl OrchestrationEventPublisher for RecordingOrchestrationEventPublisher {
    async fn publish(&self, topic: String, envelope: OrchestrationEventEnvelope) -> Result<()> {
        self.published
            .lock()
            .expect("publisher mutex poisoned")
            .push((topic, envelope));
        Ok(())
    }
}

#[derive(Clone)]
pub struct MqttOrchestrationEventPublisher {
    client: AsyncClient,
    qos: QoS,
    retain: bool,
}

impl MqttOrchestrationEventPublisher {
    pub fn from_config(config: &MqttOrchestrationPublisherConfig) -> Result<Self> {
        config.validate()?;

        let broker_url = url::Url::parse(&config.broker_url).with_context(|| {
            format!(
                "invalid orchestration MQTT broker_url: {}",
                config.broker_url
            )
        })?;
        let scheme = broker_url.scheme();
        if !matches!(scheme, "mqtt" | "tcp") {
            return Err(anyhow!(
                "unsupported orchestration MQTT broker_url scheme '{scheme}', expected mqtt:// or tcp://"
            ));
        }

        let host = broker_url
            .host_str()
            .ok_or_else(|| anyhow!("orchestration MQTT broker_url must include a host"))?;
        let port = broker_url.port().unwrap_or(1883);
        let client_id = config
            .client_id
            .clone()
            .unwrap_or_else(|| format!("vibe-kanban-{}", Uuid::new_v4()));

        let mut mqtt_options = MqttOptions::new(client_id, host, port);
        mqtt_options.set_keep_alive(Duration::from_secs(30));
        if !broker_url.username().is_empty() {
            mqtt_options.set_credentials(
                broker_url.username(),
                broker_url.password().unwrap_or_default(),
            );
        }

        let (client, event_loop) = AsyncClient::new(mqtt_options, 100);
        spawn_event_loop(event_loop);

        Ok(Self {
            client,
            qos: qos_from_u8(config.qos),
            retain: config.retain,
        })
    }
}

fn qos_from_u8(qos: u8) -> QoS {
    match qos {
        0 => QoS::AtMostOnce,
        1 => QoS::AtLeastOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtMostOnce,
    }
}

fn spawn_event_loop(mut event_loop: EventLoop) {
    tokio::spawn(async move {
        loop {
            if let Err(error) = event_loop.poll().await {
                tracing::warn!(%error, "Orchestration MQTT event loop error");
                sleep(Duration::from_secs(1)).await;
            }
        }
    });
}

#[async_trait]
impl OrchestrationEventPublisher for MqttOrchestrationEventPublisher {
    async fn publish(&self, topic: String, envelope: OrchestrationEventEnvelope) -> Result<()> {
        let payload = serde_json::to_vec(&envelope)?;
        self.client
            .publish(topic, self.qos, self.retain, payload)
            .await
            .map_err(|error| anyhow!("failed to publish orchestration event: {error}"))
    }
}

pub fn build_mqtt_orchestration_event_publisher(
    config: &MqttOrchestrationPublisherConfig,
) -> Result<OrchestrationEventPublisherHandle> {
    Ok(Arc::new(MqttOrchestrationEventPublisher::from_config(
        config,
    )?))
}

#[derive(Clone)]
pub struct OrchestrationEventMapper {
    pool: SqlitePool,
}

impl OrchestrationEventMapper {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn map_event(&self, event: &DomainEvent) -> Result<Vec<OrchestrationEventEnvelope>> {
        let mut envelopes = vec![self.map_primary_event(event)];

        envelopes.extend(self.map_task_group_completed(event).await?);

        Ok(envelopes)
    }

    fn map_primary_event(&self, event: &DomainEvent) -> OrchestrationEventEnvelope {
        match event {
            DomainEvent::TaskLifecycle {
                action,
                task,
                previous_task_group_id,
                ..
            } => OrchestrationEventEnvelope::new(
                event,
                match action {
                    TaskLifecycleAction::Created => OrchestrationEventType::TaskCreated,
                    TaskLifecycleAction::Updated => OrchestrationEventType::TaskUpdated,
                    TaskLifecycleAction::Deleted => OrchestrationEventType::TaskDeleted,
                },
                OrchestrationEventPayload::TaskLifecycle(TaskLifecycleEventPayload {
                    project_id: task.project_id,
                    status: task.status.clone(),
                    previous_task_group_id: *previous_task_group_id,
                }),
            ),
            DomainEvent::TaskStatusChanged {
                task,
                previous_status,
            } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::TaskStatusChanged,
                OrchestrationEventPayload::TaskStatusChanged(TaskStatusChangedEventPayload {
                    project_id: task.project_id,
                    status: task.status.clone(),
                    previous_status: previous_status.clone(),
                }),
            ),
            DomainEvent::ExecutionStarted { process, .. } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::ExecutionStarted,
                OrchestrationEventPayload::ExecutionStarted(ExecutionStartedEventPayload {
                    status: process.status.clone(),
                    run_reason: process.run_reason.clone(),
                    conversation_session_id: process.conversation_session_id,
                }),
            ),
            DomainEvent::ExecutionCompleted { process, .. } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::ExecutionCompleted,
                OrchestrationEventPayload::ExecutionCompleted(ExecutionCompletedEventPayload {
                    status: process.status.clone(),
                    run_reason: process.run_reason.clone(),
                    exit_code: process.exit_code,
                    conversation_session_id: process.conversation_session_id,
                }),
            ),
            DomainEvent::WorkspaceCreated { workspace } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::WorkspaceCreated,
                OrchestrationEventPayload::WorkspaceCreated(WorkspaceCreatedEventPayload {
                    branch: workspace.branch.clone(),
                }),
            ),
            DomainEvent::WorkspaceDeleted { .. } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::WorkspaceDeleted,
                OrchestrationEventPayload::WorkspaceDeleted(OrchestrationEmptyPayload::default()),
            ),
            DomainEvent::ProjectUpdated { project } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::ProjectUpdated,
                OrchestrationEventPayload::ProjectUpdated(ProjectUpdatedEventPayload {
                    project_id: project.id,
                }),
            ),
            DomainEvent::ApprovalRequested {
                approval_id,
                kind,
                tool_call_id,
                tool_name,
                question_count,
                ..
            } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::ApprovalRequested,
                OrchestrationEventPayload::ApprovalRequested(ApprovalRequestedEventPayload {
                    approval_id: approval_id.clone(),
                    kind: *kind,
                    tool_call_id: Some(tool_call_id.clone()),
                    tool_name: tool_name.clone(),
                    question_count: *question_count,
                }),
            ),
            DomainEvent::ApprovalResolved {
                approval_id,
                resolution,
                tool_call_id,
                ..
            } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::ApprovalResolved,
                OrchestrationEventPayload::ApprovalResolved(ApprovalResolvedEventPayload {
                    approval_id: approval_id.clone(),
                    resolution: *resolution,
                    tool_call_id: tool_call_id.clone(),
                }),
            ),
            DomainEvent::ConversationMessageAdded {
                conversation_session_id,
                message_id,
                execution_process_id,
                role,
                ..
            } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::ConversationMessageAdded,
                OrchestrationEventPayload::ConversationMessageAdded(
                    ConversationMessageAddedEventPayload {
                        conversation_session_id: *conversation_session_id,
                        message_id: *message_id,
                        execution_process_id: *execution_process_id,
                        role: *role,
                    },
                ),
            ),
            DomainEvent::FollowUpTransition {
                state,
                scope,
                queue_kind,
                execution_process_id,
                ..
            } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::FollowUpTransition,
                OrchestrationEventPayload::FollowUpTransition(FollowUpTransitionEventPayload {
                    state: *state,
                    scope: *scope,
                    queue_kind: *queue_kind,
                    execution_process_id: *execution_process_id,
                }),
            ),
            DomainEvent::MergeQueueTransition {
                entry_id,
                project_id,
                repo_id,
                state,
                merge_commit,
                detail,
                ..
            } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::MergeQueueTransition,
                OrchestrationEventPayload::MergeQueueTransition(MergeQueueTransitionEventPayload {
                    entry_id: *entry_id,
                    project_id: *project_id,
                    repo_id: *repo_id,
                    state: *state,
                    merge_commit: merge_commit.clone(),
                    detail: detail.clone(),
                }),
            ),
            DomainEvent::TaskGroupTransition {
                action,
                project_id,
                task_group_id,
                previous_task_group_id,
                task_ids,
                ..
            } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::TaskGroupTransition,
                OrchestrationEventPayload::TaskGroupTransition(TaskGroupTransitionEventPayload {
                    action: *action,
                    project_id: *project_id,
                    task_group_id: *task_group_id,
                    previous_task_group_id: *previous_task_group_id,
                    task_ids: task_ids.clone(),
                }),
            ),
            DomainEvent::TaskGroupCompleted {
                project_id,
                completed_task_ids,
                terminal_task_count,
                ..
            } => OrchestrationEventEnvelope::new(
                event,
                OrchestrationEventType::TaskGroupCompleted,
                OrchestrationEventPayload::TaskGroupCompleted(TaskGroupCompletedEventPayload {
                    project_id: *project_id,
                    completed_task_ids: completed_task_ids.clone(),
                    terminal_task_count: *terminal_task_count,
                }),
            ),
        }
    }

    async fn map_task_group_completed(
        &self,
        event: &DomainEvent,
    ) -> Result<Vec<OrchestrationEventEnvelope>> {
        let Some((project_id, candidate_group_ids)) = self.task_group_completion_candidates(event)
        else {
            return Ok(Vec::new());
        };

        let grouped_tasks = Task::find_by_project_id_with_attempt_status(&self.pool, project_id)
            .await?
            .into_iter()
            .map(|task| task.task)
            .collect::<Vec<_>>();
        let stats = TaskGroup::get_stats_for_project(&self.pool, project_id).await?;

        let mut envelopes = Vec::new();
        for task_group_id in candidate_group_ids {
            let completed_tasks = grouped_tasks
                .iter()
                .filter(|task| task.task_group_id == Some(task_group_id))
                .cloned()
                .collect::<Vec<_>>();

            if completed_tasks.is_empty() {
                continue;
            }

            let Some(group_stats) = stats.iter().find(|group| group.group.id == task_group_id)
            else {
                continue;
            };

            if group_stats.task_counts.todo
                + group_stats.task_counts.inprogress
                + group_stats.task_counts.inreview
                > 0
            {
                continue;
            }

            let terminal_task_count = completed_tasks.len();
            let derived_event = DomainEvent::TaskGroupCompleted {
                project_id,
                task_group_id,
                completed_task_ids: completed_tasks.into_iter().map(|task| task.id).collect(),
                terminal_task_count,
                occurred_at: event.occurred_at(),
            };

            envelopes.push(self.map_primary_event(&derived_event));
        }

        Ok(envelopes)
    }

    fn task_group_completion_candidates(&self, event: &DomainEvent) -> Option<(Uuid, Vec<Uuid>)> {
        match event {
            DomainEvent::TaskStatusChanged { task, .. }
                if matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled) =>
            {
                Some((task.project_id, task.task_group_id.into_iter().collect()))
            }
            DomainEvent::TaskLifecycle {
                action,
                task,
                previous_task_group_id,
                ..
            } if matches!(
                action,
                TaskLifecycleAction::Updated | TaskLifecycleAction::Deleted
            ) =>
            {
                let mut group_ids = Vec::new();
                if let Some(group_id) = *previous_task_group_id {
                    group_ids.push(group_id);
                }
                if let Some(group_id) = task.task_group_id
                    && !group_ids.contains(&group_id)
                {
                    group_ids.push(group_id);
                }

                (!group_ids.is_empty()).then_some((task.project_id, group_ids))
            }
            DomainEvent::TaskGroupTransition {
                action,
                project_id,
                task_group_id,
                previous_task_group_id,
                ..
            } if matches!(
                action,
                TaskGroupTransitionAction::Merged | TaskGroupTransitionAction::AssignmentChanged
            ) =>
            {
                let mut group_ids = Vec::new();
                if let Some(group_id) = *previous_task_group_id {
                    group_ids.push(group_id);
                }
                if let Some(group_id) = *task_group_id
                    && !group_ids.contains(&group_id)
                {
                    group_ids.push(group_id);
                }

                (!group_ids.is_empty()).then_some((*project_id, group_ids))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use db::models::{
        execution_process::{
            ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus,
            ExecutorActionField,
        },
        project::{CreateProject, Project},
        task::{CreateTask, Task},
        task_group::TaskGroup,
    };
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::services::domain_events::{
        ApprovalEventKind, ApprovalResolution, ConversationMessageEventRole,
        MergeQueueTransitionState,
    };

    async fn create_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create sqlite memory db");

        sqlx::migrate!("../db/migrations")
            .run(&pool)
            .await
            .expect("run migrations");

        pool
    }

    async fn create_project(pool: &SqlitePool) -> Project {
        Project::create(
            pool,
            &CreateProject {
                name: "Wire Contract Project".to_string(),
                repositories: vec![],
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create project")
    }

    async fn create_grouped_task(
        pool: &SqlitePool,
        project_id: Uuid,
        task_group_id: Uuid,
        title: &str,
        status: TaskStatus,
    ) -> Task {
        Task::create(
            pool,
            &CreateTask {
                project_id,
                title: title.to_string(),
                description: None,
                status: Some(status),
                parent_workspace_id: None,
                image_ids: None,
                shared_task_id: None,
                task_group_id: Some(task_group_id),
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create task")
    }

    fn sample_execution_process(session_id: Uuid) -> ExecutionProcess {
        let now = Utc::now();
        ExecutionProcess {
            id: Uuid::new_v4(),
            session_id: Some(session_id),
            conversation_session_id: None,
            run_reason: ExecutionProcessRunReason::CodingAgent,
            executor_action: sqlx::types::Json(ExecutorActionField::Other(json!({"kind": "test"}))),
            status: ExecutionProcessStatus::Completed,
            exit_code: Some(0),
            dropped: false,
            input_tokens: Some(10),
            output_tokens: Some(20),
            started_at: now,
            completed_at: Some(now),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn test_mqtt_config_validation_rejects_invalid_values() {
        let missing_namespace = MqttOrchestrationPublisherConfig {
            broker_url: "mqtt://localhost:1883".to_string(),
            topic_namespace: "".to_string(),
            client_id: None,
            qos: 0,
            retain: false,
        };
        assert!(missing_namespace.validate().is_err());

        let bad_qos = MqttOrchestrationPublisherConfig {
            broker_url: "mqtt://localhost:1883".to_string(),
            topic_namespace: "vk/orchestration".to_string(),
            client_id: None,
            qos: 3,
            retain: false,
        };
        assert!(bad_qos.validate().is_err());
    }

    #[test]
    fn test_publisher_config_requires_mqtt_when_enabled() {
        let config = OrchestrationEventPublisherConfig {
            enabled: true,
            mqtt: None,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_mqtt_topic_uses_namespace_and_event_type() {
        let mqtt = MqttOrchestrationPublisherConfig {
            broker_url: "mqtt://localhost:1883".to_string(),
            topic_namespace: "vk/orchestration".to_string(),
            client_id: Some("vk-test".to_string()),
            qos: 1,
            retain: false,
        };

        assert_eq!(
            mqtt.topic_for(&OrchestrationEventType::TaskStatusChanged),
            "vk/orchestration/task_status_changed"
        );
    }

    #[tokio::test]
    async fn test_serializes_compact_task_status_envelope() {
        let pool = create_test_pool().await;
        let mapper = OrchestrationEventMapper::new(pool.clone());
        let project = create_project(&pool).await;
        let task_group = TaskGroup::create(&pool, project.id, "Group A".to_string(), None, None)
            .await
            .expect("create group");
        let task =
            create_grouped_task(&pool, project.id, task_group.id, "Task A", TaskStatus::Done).await;

        let envelopes = mapper
            .map_event(&DomainEvent::TaskStatusChanged {
                task: task.clone(),
                previous_status: TaskStatus::InReview,
            })
            .await
            .expect("map event");

        let primary = envelopes.first().expect("primary envelope");
        let json = serde_json::to_value(primary).expect("serialize envelope");

        assert_eq!(
            json["schema_version"],
            DEFAULT_ORCHESTRATION_EVENT_SCHEMA_VERSION
        );
        assert_eq!(json["event_type"], "task_status_changed");
        assert_eq!(json["task_id"], task.id.to_string());
        assert_eq!(json["task_group_id"], task_group.id.to_string());
        assert!(json.get("workspace_id").is_some());
        assert_eq!(json["workspace_id"], serde_json::Value::Null);
        assert!(json.get("session_id").is_some());
        assert_eq!(json["session_id"], serde_json::Value::Null);
        assert!(json.get("occurred_at").is_some());
        assert!(json.get("event_id").is_some());
        assert!(json.get("task").is_none());
        assert!(json.get("process").is_none());
        assert_eq!(json["payload"]["project_id"], project.id.to_string());
        assert_eq!(json["payload"]["status"], "done");
        assert_eq!(json["payload"]["previous_status"], "inreview");
    }

    #[tokio::test]
    async fn test_maps_execution_completed_with_compact_correlation_fields() {
        let pool = create_test_pool().await;
        let mapper = OrchestrationEventMapper::new(pool);
        let session_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let task_group_id = Uuid::new_v4();

        let process = sample_execution_process(session_id);
        let process_id = process.id;
        let envelopes = mapper
            .map_event(&DomainEvent::ExecutionCompleted {
                process,
                task_id: Some(task_id),
                workspace_id: Some(workspace_id),
                task_group_id: Some(task_group_id),
            })
            .await
            .expect("map event");

        let json = serde_json::to_value(envelopes.first().expect("primary envelope"))
            .expect("serialize envelope");

        assert_eq!(json["event_type"], "execution_completed");
        assert_eq!(json["task_id"], task_id.to_string());
        assert_eq!(json["workspace_id"], workspace_id.to_string());
        assert_eq!(json["session_id"], session_id.to_string());
        assert_eq!(json["execution_process_id"], process_id.to_string());
        assert_eq!(json["task_group_id"], task_group_id.to_string());
        assert_eq!(json["payload"]["status"], "completed");
        assert_eq!(json["payload"]["run_reason"], "coding_agent");
    }

    #[tokio::test]
    async fn test_derives_task_group_completed_when_last_task_finishes() {
        let pool = create_test_pool().await;
        let mapper = OrchestrationEventMapper::new(pool.clone());
        let project = create_project(&pool).await;
        let task_group = TaskGroup::create(&pool, project.id, "Group B".to_string(), None, None)
            .await
            .expect("create group");

        let done_task = create_grouped_task(
            &pool,
            project.id,
            task_group.id,
            "Done already",
            TaskStatus::Done,
        )
        .await;
        let finishing_task = create_grouped_task(
            &pool,
            project.id,
            task_group.id,
            "Finishing",
            TaskStatus::Done,
        )
        .await;

        let envelopes = mapper
            .map_event(&DomainEvent::TaskStatusChanged {
                task: finishing_task.clone(),
                previous_status: TaskStatus::InProgress,
            })
            .await
            .expect("map event");

        assert_eq!(envelopes.len(), 2);

        let derived = envelopes
            .iter()
            .find(|envelope| envelope.event_type() == OrchestrationEventType::TaskGroupCompleted)
            .expect("derived task_group_completed envelope");
        let json = serde_json::to_value(derived).expect("serialize derived envelope");

        assert_eq!(json["task_group_id"], task_group.id.to_string());
        assert_eq!(json["payload"]["project_id"], project.id.to_string());
        assert_eq!(json["payload"]["terminal_task_count"], 2);
        let completed_task_ids = json["payload"]["completed_task_ids"]
            .as_array()
            .expect("task ids array");
        assert!(
            completed_task_ids
                .iter()
                .any(|value| value == &json!(done_task.id))
        );
        assert!(
            completed_task_ids
                .iter()
                .any(|value| value == &json!(finishing_task.id))
        );
    }

    #[tokio::test]
    async fn test_derives_task_group_completed_when_last_task_is_cancelled() {
        let pool = create_test_pool().await;
        let mapper = OrchestrationEventMapper::new(pool.clone());
        let project = create_project(&pool).await;
        let task_group =
            TaskGroup::create(&pool, project.id, "Group Cancelled".to_string(), None, None)
                .await
                .expect("create group");

        let done_task = create_grouped_task(
            &pool,
            project.id,
            task_group.id,
            "Done already",
            TaskStatus::Done,
        )
        .await;
        let cancelled_task = create_grouped_task(
            &pool,
            project.id,
            task_group.id,
            "Cancelled last",
            TaskStatus::Cancelled,
        )
        .await;

        let envelopes = mapper
            .map_event(&DomainEvent::TaskStatusChanged {
                task: cancelled_task.clone(),
                previous_status: TaskStatus::InProgress,
            })
            .await
            .expect("map event");

        let derived = envelopes
            .iter()
            .find(|envelope| envelope.event_type() == OrchestrationEventType::TaskGroupCompleted)
            .expect("derived task_group_completed envelope");
        let json = serde_json::to_value(derived).expect("serialize derived envelope");

        assert_eq!(json["task_group_id"], task_group.id.to_string());
        assert_eq!(json["payload"]["terminal_task_count"], 2);
        let completed_task_ids = json["payload"]["completed_task_ids"]
            .as_array()
            .expect("task ids array");
        assert!(
            completed_task_ids
                .iter()
                .any(|value| value == &json!(done_task.id))
        );
        assert!(
            completed_task_ids
                .iter()
                .any(|value| value == &json!(cancelled_task.id))
        );
    }

    #[tokio::test]
    async fn test_builds_concrete_mqtt_publisher_from_config() {
        let publisher =
            build_mqtt_orchestration_event_publisher(&MqttOrchestrationPublisherConfig {
                broker_url: "mqtt://127.0.0.1:1883".to_string(),
                topic_namespace: "vk/orchestration".to_string(),
                client_id: Some("vk-contract-test".to_string()),
                qos: 1,
                retain: false,
            })
            .expect("build MQTT publisher");

        let envelope = OrchestrationEventEnvelope::TaskStatusChanged {
            metadata: OrchestrationEventMetadata {
                event_id: Uuid::new_v4(),
                schema_version: OrchestrationSchemaVersion::V1,
                occurred_at: Utc::now(),
                task_id: Some(Uuid::new_v4()),
                workspace_id: None,
                session_id: None,
                execution_process_id: None,
                task_group_id: None,
            },
            payload: TaskStatusChangedEventPayload {
                project_id: Uuid::new_v4(),
                status: TaskStatus::Done,
                previous_status: TaskStatus::InProgress,
            },
        };

        // The publish call proves the concrete MQTT transport is bound into the seam.
        // It may still fail later if no broker is reachable, but the local deployment now
        // constructs a real publisher rather than silently disabling emission.
        let publish_result = publisher
            .publish("vk/orchestration/task_status_changed".to_string(), envelope)
            .await;
        assert!(publish_result.is_ok());
    }

    #[tokio::test]
    async fn test_does_not_derive_task_group_completed_when_group_has_open_work() {
        let pool = create_test_pool().await;
        let mapper = OrchestrationEventMapper::new(pool.clone());
        let project = create_project(&pool).await;
        let task_group = TaskGroup::create(&pool, project.id, "Group C".to_string(), None, None)
            .await
            .expect("create group");

        let finishing_task = create_grouped_task(
            &pool,
            project.id,
            task_group.id,
            "Finishing",
            TaskStatus::Done,
        )
        .await;
        let _open_task = create_grouped_task(
            &pool,
            project.id,
            task_group.id,
            "Still open",
            TaskStatus::InProgress,
        )
        .await;

        let envelopes = mapper
            .map_event(&DomainEvent::TaskStatusChanged {
                task: finishing_task,
                previous_status: TaskStatus::InReview,
            })
            .await
            .expect("map event");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(
            envelopes[0].event_type(),
            OrchestrationEventType::TaskStatusChanged
        );
    }

    #[tokio::test]
    async fn test_compact_payload_for_missing_lifecycle_families() {
        let now = Utc::now();
        let entity_ids = super::super::DomainEventEntityIds {
            task_id: Some(Uuid::new_v4()),
            workspace_id: Some(Uuid::new_v4()),
            session_id: Some(Uuid::new_v4()),
            execution_process_id: Some(Uuid::new_v4()),
            task_group_id: Some(Uuid::new_v4()),
        };
        let mapper = OrchestrationEventMapper::new(create_test_pool().await);

        let approval = mapper.map_primary_event(&DomainEvent::ApprovalRequested {
            approval_id: "approval-1".to_string(),
            kind: ApprovalEventKind::UserQuestion,
            tool_call_id: "tool-call-1".to_string(),
            tool_name: None,
            question_count: Some(2),
            entity_ids,
            occurred_at: now,
        });
        assert_eq!(
            approval.event_type(),
            OrchestrationEventType::ApprovalRequested
        );
        let approval_json = serde_json::to_value(&approval).expect("serialize approval event");
        assert_eq!(approval_json["payload"]["question_count"], 2);

        let conversation = mapper.map_primary_event(&DomainEvent::ConversationMessageAdded {
            conversation_session_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            execution_process_id: entity_ids.execution_process_id,
            role: ConversationMessageEventRole::Assistant,
            entity_ids,
            occurred_at: now,
        });
        assert_eq!(
            conversation.event_type(),
            OrchestrationEventType::ConversationMessageAdded
        );
        let conversation_json =
            serde_json::to_value(&conversation).expect("serialize conversation event");
        assert_eq!(conversation_json["payload"]["role"], "assistant");

        let merge_queue = mapper.map_primary_event(&DomainEvent::MergeQueueTransition {
            entry_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            workspace_id: entity_ids.workspace_id.expect("workspace id"),
            task_id: entity_ids.task_id,
            task_group_id: entity_ids.task_group_id,
            repo_id: Uuid::new_v4(),
            state: MergeQueueTransitionState::Claimed,
            merge_commit: None,
            detail: Some("claimed by processor".to_string()),
            occurred_at: now,
        });
        assert_eq!(
            merge_queue.event_type(),
            OrchestrationEventType::MergeQueueTransition
        );
        let merge_queue_json =
            serde_json::to_value(&merge_queue).expect("serialize merge queue event");
        assert_eq!(merge_queue_json["payload"]["state"], "claimed");

        let resolved = mapper.map_primary_event(&DomainEvent::ApprovalResolved {
            approval_id: "approval-1".to_string(),
            resolution: ApprovalResolution::Answered,
            tool_call_id: Some("tool-call-1".to_string()),
            entity_ids,
            occurred_at: now,
        });
        assert_eq!(
            resolved.event_type(),
            OrchestrationEventType::ApprovalResolved
        );
        let resolved_json = serde_json::to_value(&resolved).expect("serialize resolved event");
        assert_eq!(resolved_json["payload"]["resolution"], "answered");
    }
}
