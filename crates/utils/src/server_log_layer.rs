use std::sync::Arc;

use chrono::Utc;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

use crate::server_log_store::{ServerLogEntry, ServerLogStore};

/// A tracing layer that captures log events and pushes them to a `ServerLogStore`.
pub struct ServerLogLayer {
    store: Arc<ServerLogStore>,
}

impl ServerLogLayer {
    pub fn new(store: Arc<ServerLogStore>) -> Self {
        Self { store }
    }
}

impl<S> Layer<S> for ServerLogLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        let timestamp = Utc::now();
        let level = metadata.level().to_string();
        let target = metadata.target().to_string();

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let entry = ServerLogEntry {
            timestamp,
            level,
            target,
            message: visitor.into_message(),
        };

        self.store.push(entry);
    }
}

/// Visitor to extract the message field from a tracing event.
///
/// Prioritizes the "message" field, but falls back to collecting all fields
/// if no explicit message is present.
#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
    other_fields: String,
}

impl MessageVisitor {
    fn into_message(self) -> String {
        self.message.unwrap_or(self.other_fields)
    }

    fn append_field(&mut self, field: &Field, formatted: String) {
        if !self.other_fields.is_empty() {
            self.other_fields.push_str(", ");
        }
        self.other_fields.push_str(field.name());
        self.other_fields.push('=');
        self.other_fields.push_str(&formatted);
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        } else {
            self.append_field(field, format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.append_field(field, value.to_string());
        }
    }
}
