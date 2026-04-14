use async_trait::async_trait;

use crate::services::domain_events::{
    DomainEvent, EventHandler, ExecutionMode, HandlerContext, HandlerError,
    OrchestrationEventMapper,
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
        let Some(mqtt) = ctx
            .config
            .read()
            .await
            .orchestration_event_publisher
            .mqtt()
            .cloned()
        else {
            return Ok(());
        };

        let mapper = OrchestrationEventMapper::new(ctx.db.pool.clone());
        let envelopes = mapper.map_event(&event).await?;

        for envelope in envelopes {
            publisher
                .publish(mqtt.topic_for(&envelope.event_type), envelope)
                .await?;
        }

        Ok(())
    }
}
