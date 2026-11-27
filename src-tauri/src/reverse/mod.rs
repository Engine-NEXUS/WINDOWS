//! Reverse Lab - STATIC reverse-engineering triage for the user's own programs and captured HAR traffic.
//!
//! Hard rules (enforced by tests / grep gate, see `docs/features/91-reverse-lab.md`):
//!   * the target is only ever READ as bytes - never executed, loaded, patched or replayed;
//!   * secrets never leave the device: HAR data is redacted before inference/output/LLM;
//!   * every LLM statement must cite evidence the analysis actually produced (`evidence`, `explain`);
//!   * no decompilation, cracking, keygen, licence bypass or exploit generation.
//!
//! Everything except `target::foreground_exe_path` is a pure function over bytes, so it is fully
//! unit-testable on any OS.

pub mod target;
