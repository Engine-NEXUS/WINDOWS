//! NEXUS Reverse Lab core - STATIC analysis only.
//!
//! Hard rules (see `docs/features/91-reverse-lab.md`, enforced by tests and a grep gate):
//!   * inputs are only ever READ as bytes - nothing is executed, loaded, patched or replayed;
//!   * secrets never leave: HAR data is redacted before inference, output and any LLM prompt;
//!   * every LLM claim must cite evidence this crate produced (`evidence`, `explain`);
//!   * no decompilation, cracking, keygen, licence bypass or exploit generation.
//!
//! Everything here is a pure function over bytes/strings, so it is unit-testable on any OS.

pub mod analyze;
pub mod dotnet;
pub mod electron;
pub mod evidence;
pub mod har;
pub mod packer;
pub mod pe;
pub mod rules;
pub mod redact;
pub mod strings;
pub mod synth;
