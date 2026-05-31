//! Neutral action rendering seam.
//!
//! The core decides *what* to do through [`PrAction`] and [`Task`]. A forge
//! adapter owns how that intent becomes an operator-facing command line.

use crate::types::{PrAction, PullRequest};

pub trait ActionRenderer {
    fn render(&self, pr: &PullRequest, action: &PrAction) -> String;
}
