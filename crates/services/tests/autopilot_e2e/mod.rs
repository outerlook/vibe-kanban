//! End-to-end tests for autopilot hooks.
//!
//! These tests verify the autopilot workflow:
//! 1. Completed tasks trigger merge attempts
//! 2. Successful merges queue dependent tasks
//! 3. The entire pipeline runs automatically when autopilot is enabled
//!
//! Test structure:
//! - `fixtures`: Database, config, and entity creation helpers (including `EntityGraphBuilder`)
//! - `entity_builder_tests`: Tests for the `EntityGraphBuilder` fluent API
//! - Additional test modules will be added for specific scenarios

pub mod fixtures;

#[cfg(test)]
mod entity_builder_tests;
