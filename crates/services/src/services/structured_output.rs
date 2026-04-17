use db::models::conversation_message::{
    ConversationMessageMetadata, ConversationStructuredOutputMetadata,
    StructuredOutputSchemaValidationIssue, StructuredOutputValidationErrorMetadata,
    StructuredOutputValidationStatus,
};
use executors::actions::ExecutorAction;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum StructuredOutputValidationOutcome {
    NotRequested,
    Valid {
        metadata: ConversationMessageMetadata,
    },
    Invalid {
        metadata: ConversationMessageMetadata,
    },
}

impl StructuredOutputValidationOutcome {
    pub fn metadata(&self) -> Option<ConversationMessageMetadata> {
        match self {
            Self::NotRequested => None,
            Self::Valid { metadata } | Self::Invalid { metadata } => Some(metadata.clone()),
        }
    }

    pub fn should_fail_execution(&self) -> bool {
        matches!(self, Self::Invalid { .. })
    }
}

pub fn validate_execution_output(
    executor_action: &ExecutorAction,
    assistant_text: Option<&str>,
) -> StructuredOutputValidationOutcome {
    let Some(contract) = executor_action.structured_output() else {
        return StructuredOutputValidationOutcome::NotRequested;
    };

    let raw_text = assistant_text.unwrap_or_default();
    if raw_text.trim().is_empty() {
        return StructuredOutputValidationOutcome::Invalid {
            metadata: invalid_metadata(StructuredOutputValidationErrorMetadata::JsonParse {
                message:
                    "Assistant response was empty; expected JSON matching the requested schema"
                        .to_string(),
            }),
        };
    }

    let payload: Value = match serde_json::from_str(raw_text) {
        Ok(payload) => payload,
        Err(error) => {
            return StructuredOutputValidationOutcome::Invalid {
                metadata: invalid_metadata(StructuredOutputValidationErrorMetadata::JsonParse {
                    message: error.to_string(),
                }),
            };
        }
    };

    let schema = match serde_json::to_value(&contract.schema) {
        Ok(schema) => schema,
        Err(error) => {
            return StructuredOutputValidationOutcome::Invalid {
                metadata: invalid_metadata(
                    StructuredOutputValidationErrorMetadata::SchemaDefinition {
                        message: error.to_string(),
                    },
                ),
            };
        }
    };

    let validator = match jsonschema::validator_for(&schema) {
        Ok(validator) => validator,
        Err(error) => {
            return StructuredOutputValidationOutcome::Invalid {
                metadata: invalid_metadata(
                    StructuredOutputValidationErrorMetadata::SchemaDefinition {
                        message: error.to_string(),
                    },
                ),
            };
        }
    };

    let issues: Vec<StructuredOutputSchemaValidationIssue> = validator
        .iter_errors(&payload)
        .map(|error| StructuredOutputSchemaValidationIssue {
            instance_path: error.instance_path().to_string(),
            schema_path: error.schema_path().to_string(),
            message: error.to_string(),
        })
        .collect();

    if issues.is_empty() {
        return StructuredOutputValidationOutcome::Valid {
            metadata: ConversationMessageMetadata {
                structured_output: Some(ConversationStructuredOutputMetadata {
                    status: StructuredOutputValidationStatus::Valid,
                    payload: Some(payload),
                    error: None,
                }),
            },
        };
    }

    StructuredOutputValidationOutcome::Invalid {
        metadata: invalid_metadata(StructuredOutputValidationErrorMetadata::SchemaValidation {
            message: issues
                .first()
                .map(|issue| issue.message.clone())
                .unwrap_or_else(|| "JSON output failed schema validation".to_string()),
            issues,
        }),
    }
}

fn invalid_metadata(error: StructuredOutputValidationErrorMetadata) -> ConversationMessageMetadata {
    ConversationMessageMetadata {
        structured_output: Some(ConversationStructuredOutputMetadata {
            status: StructuredOutputValidationStatus::Invalid,
            payload: None,
            error: Some(error),
        }),
    }
}

#[cfg(test)]
mod tests {
    use executors::{
        actions::{
            ExecutorAction, ExecutorActionType, StructuredOutputContract,
            coding_agent_initial::CodingAgentInitialRequest,
        },
        executors::BaseCodingAgent,
        profile::ExecutorProfileId,
    };
    use schemars::json_schema;

    use super::*;

    fn structured_action(schema: schemars::Schema) -> ExecutorAction {
        ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt: "reply".to_string(),
                structured_output: Some(StructuredOutputContract { schema }),
                executor_profile_id: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
                working_dir: None,
            }),
            None,
        )
    }

    #[test]
    fn validate_execution_output_accepts_valid_json() {
        let action = structured_action(json_schema!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        }));

        let outcome = validate_execution_output(&action, Some(r#"{"answer":"ok"}"#));

        match outcome {
            StructuredOutputValidationOutcome::Valid { metadata } => {
                let structured = metadata
                    .structured_output
                    .expect("structured output metadata");
                assert_eq!(structured.status, StructuredOutputValidationStatus::Valid);
                assert_eq!(
                    structured.payload,
                    Some(serde_json::json!({ "answer": "ok" }))
                );
                assert!(structured.error.is_none());
            }
            other => panic!("expected valid outcome, got {other:?}"),
        }
    }

    #[test]
    fn validate_execution_output_rejects_invalid_json() {
        let action = structured_action(json_schema!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"],
            "additionalProperties": false
        }));

        let outcome = validate_execution_output(&action, Some("not json"));

        match outcome {
            StructuredOutputValidationOutcome::Invalid { metadata } => {
                let structured = metadata
                    .structured_output
                    .expect("structured output metadata");
                assert_eq!(structured.status, StructuredOutputValidationStatus::Invalid);
                assert!(structured.payload.is_none());
                assert!(matches!(
                    structured.error,
                    Some(StructuredOutputValidationErrorMetadata::JsonParse { .. })
                ));
            }
            other => panic!("expected invalid outcome, got {other:?}"),
        }
    }

    #[test]
    fn validate_execution_output_passthrough_without_schema() {
        let action = ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt: "reply".to_string(),
                structured_output: None,
                executor_profile_id: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
                working_dir: None,
            }),
            None,
        );

        let outcome = validate_execution_output(&action, Some("plain text is fine"));

        assert_eq!(outcome, StructuredOutputValidationOutcome::NotRequested);
    }
}
