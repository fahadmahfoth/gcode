//! Models: the registry, the downloader, and the inference boundary.
//!
//! Split by phase rather than by convenience. The registry is pure data and
//! arrives first; the downloader needs the network behind ADR 0001's `download`
//! feature; inference needs the FFI behind `inference`. Each is behind a switch
//! so a build without it never contains the code it guards.

pub mod download;
pub mod registry;
