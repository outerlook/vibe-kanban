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
//! - `test_diamond_deps`: Tests for diamond dependency graph scenarios
//! - `test_review_to_merge`: Tests for review-to-merge flow

pub mod fixtures;

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
