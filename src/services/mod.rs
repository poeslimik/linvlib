//! Domain services.
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`aladin`] | Aladin HTTP client, search, import |
//! | [`new_releases`] | ItemList-based refresh + suggestions |
//! | [`search_keys`] | Search aliases / bundles |
//! | [`catalog_requests`] | User catalog requests |
//! | [`scheduler`] | Daily 23:30 refresh trigger |
//! | [`backup`] | Midnight DB snapshots |
//! | [`status_report`] | Discord daily status |

pub mod aladin;
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
