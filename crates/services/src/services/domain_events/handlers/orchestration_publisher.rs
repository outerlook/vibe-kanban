use async_trait::async_trait;

use crate::services::domain_events::{
    DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError,
    OrchestrationEventMapper, default_topic_namespace,
};

pub struct OrchestrationEventPublisherHandler;

impl OrchestrationEventPublisherHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EventHandler for OrchestrationEventPublisherHandler {
    fn name(&self) -> &'static str {
        "orchestration_publisher"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Spawned
    }

    fn handles(&self, _event: &DomainEvent) -> bool {
        true
    }

    async fn handle(&self, event: DomainEvent, ctx: &HandlerContext) -> Result<(), HandlerError> {
        let Some(publisher) = &ctx.orchestration_event_publisher else {
            return Ok(());
        };
        let topic_namespace = ctx
            .config
            .read()
            .await
            .orchestration_event_publisher
            .mqtt()
            .map(|mqtt| mqtt.topic_namespace.clone())
            .unwrap_or_else(default_topic_namespace);

        let mapper = OrchestrationEventMapper::new(ctx.db.pool.clone());
        let envelopes = mapper.map_event(&event).await?;

        for envelope in envelopes {
            let event_name = serde_json::to_string(&envelope.event_type())
                .expect("event type serialization cannot fail")
                .trim_matches('"')
                .to_string();
            publisher
                .publish(
                    format!("{}/{}", topic_namespace.trim_end_matches('/'), event_name),
                    envelope,
                )
                .await?;
        }

        Ok(())
    }
}
