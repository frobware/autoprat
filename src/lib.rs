//! Autoprat: Automated pull request analysis and action tool.
//!
//! Provides functionality for querying GitHub pull requests, filtering them
//! based on various criteria, and performing automated actions like posting
//! comments or approvals. Supports both specific PR queries and broad
//! searches with sophisticated filtering capabilities.

pub mod cli;
pub mod github;
pub mod query;
pub mod types;

pub use cli::parse_args;
pub use github::GitHub;
pub use query::fetch_pull_requests;
pub use types::{
    Action, CheckConclusion, CheckInfo, CheckName, CheckNameError, CheckState, CheckUrl,
    CommentInfo, DisplayMode, Forge, LogUrl, LogUrlError, PostFilter, PullRequest, QueryResult,
    QuerySpec, Repo, RepoError, SearchFilter, Task,
};
