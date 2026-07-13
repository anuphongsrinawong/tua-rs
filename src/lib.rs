//! 🦀 Tua Agent — Rust-specialized AI coding agent (native Rust port).
//!
//! This library provides the core building blocks for the Tua agent:
//! profiles, configuration, tool execution, and prompt generation.
pub mod agent;
pub mod ast;
pub mod cache;
pub mod checkpoint;
pub mod completion;
pub mod config;
pub mod context;
pub mod context_guard;
pub mod dashboard;
pub mod hierarchy;
pub mod highlight;
pub mod learning;
pub mod lsp;
pub mod memory;
pub mod orchestrator;
pub mod parallel;
pub mod profiles;
pub mod prompts;
pub mod providers;
pub mod review;
pub mod sandbox;
pub mod session;
pub mod setup;
pub mod skills;
pub mod theme;
pub mod tools;
pub mod tui;
pub mod wasm;
pub mod workspace;
