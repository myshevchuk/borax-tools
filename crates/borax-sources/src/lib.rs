//! Adapters for online bibliographic sources (Crossref, OpenAlex, arXiv)
//! behind a common `Source` trait, with response caching, rate limiting,
//! and polite-pool identification.
//!
//! The response readers ([`crossref`], [`openalex`], [`arxiv`]) and the
//! dispatcher ([`dispatch`]) are pure: they turn a body into a
//! [`borax_core::record::Record`] and decide who to ask, without
//! performing any I/O. Tests drive them from recorded responses under
//! `tests/cassettes/`, so the whole layer is verified offline.

pub mod arxiv;
pub mod crossref;
pub mod dispatch;
pub mod openalex;
pub mod source;
