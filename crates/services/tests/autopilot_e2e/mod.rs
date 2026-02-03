//! End-to-end tests for autopilot hooks.
//!
//! These tests verify the autopilot workflow:
//! 1. Completed tasks trigger merge attempts
//! 2. Successful merges queue dependent tasks
//! 3. The entire pipeline runs automatically when autopilot is enabled
//!
//! Test structure:
//! - `fixtures`: Database, config, and entity creation helpers (including `EntityGraphBuilder`)
//!   - `fixtures::git_fixtures`: Git repository fixtures for merge-related tests
//! - `mock_execution_controller`: MockExecutionController for capturing execution triggers
//! - `entity_builder_tests`: Tests for the `EntityGraphBuilder` fluent API
//! - `test_diamond_deps`: Tests for diamond dependency graph scenarios
//! - `test_merge_to_autopilot`: Tests for dependency triggering when tasks complete
//! - `test_multi_level_deps`: Tests for multi-level dependency chain propagation
//! - `test_review_to_merge`: Tests for review-to-merge flow
//! - `test_concurrent_merge`: Tests for concurrent merge queue processing
//! - `test_feedback_to_review`: Tests for the feedback-to-review flow
//! - `test_full_flow`: Comprehensive E2E test for the complete autopilot flow

pub mod fixtures;

#[cfg(test)]
#[path = "../autopilot_e2e_fixtures/mod.rs"]
pub mod mock_execution_controller;

#[cfg(test)]
mod entity_builder_tests;

#[cfg(test)]
mod test_diamond_deps;

#[cfg(test)]
mod test_merge_to_autopilot;

#[cfg(test)]
mod test_multi_level_deps;

#[cfg(test)]
mod test_review_to_merge;

#[cfg(test)]
mod test_concurrent_merge;

#[cfg(test)]
mod test_feedback_to_review;

#[cfg(test)]
mod test_full_flow;
