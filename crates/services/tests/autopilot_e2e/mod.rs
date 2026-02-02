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
//! - `test_merge_to_autopilot`: Tests for dependency triggering when tasks complete

pub mod fixtures;

#[cfg(test)]
mod entity_builder_tests;

#[cfg(test)]
mod test_diamond_deps;

#[cfg(test)]
mod test_merge_to_autopilot;
