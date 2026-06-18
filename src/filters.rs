use anyhow::{Context, Result};

use crate::types::{PostFilter, PullRequest};

macro_rules! simple_post_filter {
    ($vis:vis $ty:ident, $pred:expr) => {
        #[derive(Debug)]
        $vis struct $ty;
        impl PostFilter for $ty {
            fn matches(&self, pr: &PullRequest) -> bool {
                ($pred)(pr)
            }
        }
    };
}

macro_rules! single_post_filter {
    ($vis:vis $ty:ident, $field:ident, $pred:expr) => {
        #[derive(Debug, Clone)]
        $vis struct $ty {
            $field: Option<String>,
        }

        impl $ty {
            pub const fn new() -> Self {
                Self { $field: None }
            }
            pub fn with_value(mut self, v: impl Into<String>) -> Self {
                self.$field = Some(v.into());
                self
            }
        }

        impl Default for $ty {
            fn default() -> Self {
                Self::new()
            }
        }

        impl PostFilter for $ty {
            fn matches(&self, pr: &PullRequest) -> bool {
                match &self.$field {
                    Some(val) => ($pred)(pr, val),
                    None => true,
                }
            }
        }
    };
}

macro_rules! multi_post_filter {
    ($vis:vis $ty:ident, $field:ident, $pred:expr) => {
        #[derive(Debug, Clone)]
        $vis struct $ty {
            pub $field: Vec<String>,
        }

        impl PostFilter for $ty {
            fn matches(&self, pr: &PullRequest) -> bool {
                if self.$field.is_empty() {
                    return true;
                }
                ($pred)(&self.$field, pr)
            }
        }
    };
}

simple_post_filter!(pub FailingCiPost, |pr: &PullRequest| {
    pr.has_failing_ci()
});

#[derive(Debug, Clone, Default)]
pub struct AuthorPost {
    author: Option<String>,
}

impl AuthorPost {
    pub const fn new() -> Self {
        Self { author: None }
    }

    pub fn with_value(mut self, v: impl Into<String>) -> Self {
        self.author = Some(v.into());
        self
    }
}

impl PostFilter for AuthorPost {
    fn matches(&self, pr: &PullRequest) -> bool {
        match self.author.as_deref() {
            Some(name) => matches_author(pr, name),
            None => true,
        }
    }
}

fn matches_author(pr: &PullRequest, name: &str) -> bool {
    // TODO: Move forge-specific author identity aliases behind the forge adapter.
    if let Some(app_name) = name.strip_prefix("app/") {
        return pr.author_login == format!("{app_name}[bot]");
    }

    pr.matches_author(name)
}

multi_post_filter!(
    pub FailingCheckPost,
    check_names,
    |names: &[String], pr: &PullRequest| { names.iter().all(|n| pr.has_failing_check(n)) }
);

single_post_filter!(pub TitlePost, title, |pr: &PullRequest, pattern: &str| {
    regex::Regex::new(pattern)
        .map(|re| re.is_match(&pr.title))
        .unwrap_or(false)
});

single_post_filter!(pub BaseBranchPost, base, |pr: &PullRequest, branch: &str| {
    pr.matches_base_branch(branch)
});

#[derive(Debug, Clone, Copy)]
enum CommitOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy)]
pub struct CommitExpr {
    op: CommitOp,
    value: u64,
}

impl CommitExpr {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() {
            anyhow::bail!("Empty --commits expression");
        }

        let (op, rest) = if let Some(r) = s.strip_prefix(">=") {
            (CommitOp::Ge, r)
        } else if let Some(r) = s.strip_prefix("<=") {
            (CommitOp::Le, r)
        } else if let Some(r) = s.strip_prefix("!=") {
            (CommitOp::Ne, r)
        } else if let Some(r) = s.strip_prefix('>') {
            (CommitOp::Gt, r)
        } else if let Some(r) = s.strip_prefix('<') {
            (CommitOp::Lt, r)
        } else if let Some(r) = s.strip_prefix('=') {
            (CommitOp::Eq, r)
        } else {
            (CommitOp::Eq, s)
        };

        let value: u64 = rest
            .trim()
            .parse()
            .with_context(|| format!("Invalid --commits expression '{s}'"))?;

        Ok(Self { op, value })
    }

    fn matches(self, n: u64) -> bool {
        match self.op {
            CommitOp::Eq => n == self.value,
            CommitOp::Ne => n != self.value,
            CommitOp::Lt => n < self.value,
            CommitOp::Le => n <= self.value,
            CommitOp::Gt => n > self.value,
            CommitOp::Ge => n >= self.value,
        }
    }
}

#[derive(Debug)]
pub struct CommitsPost {
    pub expr: CommitExpr,
}

impl PostFilter for CommitsPost {
    fn matches(&self, pr: &PullRequest) -> bool {
        self.expr.matches(pr.commit_count)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::types::{PullRequest, Repo};

    fn pr(labels: &[&str], base_branch: &str, commit_count: u64) -> PullRequest {
        PullRequest {
            repo: Repo::new("owner", "repo").unwrap(),
            number: 123,
            title: "Fix memory leak".to_string(),
            author_login: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/123".to_string(),
            labels: labels.iter().map(|label| label.to_string()).collect(),
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: base_branch.to_string(),
            commit_count,
            is_draft: false,
            checks: vec![],
            recent_comments: vec![],
        }
    }

    #[test]
    fn commits_post_filter_matches_parsed_expression() {
        let filter = CommitsPost {
            expr: CommitExpr::parse(">1").unwrap(),
        };

        assert!(filter.matches(&pr(&[], "main", 2)));
        assert!(!filter.matches(&pr(&[], "main", 1)));
    }
}
