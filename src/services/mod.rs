//! Domain services. HTTP stays in `handlers`, SQL in `repositories`.
//!
//! Book search, import, and new-release refresh go through [`catalog`] (Yes24).
//! DB columns `aladin_*` are historical names for external ids
//! (`title:`, `isbn:`, `yes24:`, `manual:`).

pub mod catalog;
pub mod backup;
pub mod catalog_edit;
pub mod catalog_requests;
pub mod email;
pub mod manual;
pub mod new_releases;
pub mod quota;
pub mod rate_limit;
pub mod scheduler;
pub mod search;
pub mod search_keys;
pub mod series;
pub mod status_report;
pub mod tierlist;
pub mod title_rules;
pub mod tour;
pub mod yes24_limit;
