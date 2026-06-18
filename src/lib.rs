//! Autoprat: Automated pull request analysis and action tool.
//!
//! Provides functionality for querying GitHub pull requests, filtering them
//! based on various criteria, and performing automated actions like posting
//! comments or approvals. Supports both specific PR queries and broad
//! searches with sophisticated filtering capabilities.

pub mod cli;
pub mod decision;
pub mod filters;
pub mod github;
pub mod pr_selector;
pub mod query;
pub mod render;
pub mod search;
pub mod shell;
pub mod types;

pub use cli::parse_args;
pub use github::{GhCliRenderer, GitHub};
pub use pr_selector::{PrIdentifier, PrSelectorError};
pub use query::{fetch_pull_requests, fetch_pull_requests_at};
pub use types::{
    ActionPolicy, AppRequest, CheckConclusion, CheckInfo, CheckName, CheckNameError,
    CheckRunStatus, CheckState, CheckUrl, CommentAction, CommentInfo, DisplayMode, DisplaySettings,
    FetchCriteria, Forge, LogUrl, LogUrlError, PostFilter, PrAction, PrState, PullRequest,
    QueryResult, QuerySpec, Repo, RepoError, RepoUrlError, SearchCriterion, SelectionPolicy, Task,
};
