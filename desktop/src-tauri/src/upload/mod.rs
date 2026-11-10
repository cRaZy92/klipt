//! Talking to the owner's Klipt server: login, resumable tus upload, and finalizing an upload
//! into a shareable clip.

pub mod api;
pub mod auth;
pub mod tus;

#[cfg(test)]
mod tests;

pub use api::{ApiClient, ApiError, FinalizeOptions, FinalizeRequest, Me, ServerClip, Session, Visibility};
pub use tus::{UploadError, UploadOutcome};
