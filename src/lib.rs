//! 🦀 Tua Agent — Rust-specialized AI coding agent (native Rust port).
//!
//! This library provides the core building blocks for the Tua agent:
//! profiles, configuration, tool execution, and prompt generation.
pub mod agent;
pub mod checkpoint;
pub mod completion;
pub mod config;
pub mod context;
pub mod dashboard;
pub mod learning;
pub mod orchestrator;
pub mod parallel;
pub mod profiles;
pub mod prompts;
pub mod providers;
pub mod session;
pub mod skills;
pub mod tools;
pub mod tui;
pub mod wasm;
pub mod workspace;
