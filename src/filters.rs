use anyhow::{Context, Result};

use crate::types::{PostFilter, PullRequest, SearchFilter};

macro_rules! simple_search_filter {
    ($vis:vis $ty:ident, $apply:expr, $matches:expr) => {
        #[derive(Debug)]
        $vis struct $ty;

        impl SearchFilter for $ty {
            fn apply(&self, terms: &mut Vec<String>) {
                ($apply)(terms)
            }

            fn matches(&self, pr: &PullRequest) -> bool {
                ($matches)(pr)
            }
        }
    };
}

macro_rules! multi_search_filter {
    ($vis:vis $ty:ident, $field:ident, $apply:expr, $matches:expr) => {
        #[derive(Debug, Clone)]
        $vis struct $ty {
            pub $field: Vec<String>,
        }
        impl SearchFilter for $ty {
            fn apply(&self, terms: &mut Vec<String>) {
                ($apply)(&self.$field, terms)
            }
            fn matches(&self, pr: &PullRequest) -> bool {
                ($matches)(&self.$field, pr)
            }
        }
    };
}

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

simple_search_filter!(
    pub NeedsApproveSearch,
    |terms: &mut Vec<String>| terms.push("-label:approved".into()),
    |pr: &PullRequest| !pr.has_label("approved")
);

simple_search_filter!(
    pub NeedsLgtmSearch,
    |terms: &mut Vec<String>| terms.push("-label:lgtm".into()),
    |pr: &PullRequest| !pr.has_label("lgtm")
);

simple_search_filter!(
    pub NeedsOkToTestSearch,
    |terms: &mut Vec<String>| terms.push("label:needs-ok-to-test".into()),
    |pr: &PullRequest| pr.has_label("needs-ok-to-test")
);

multi_search_filter!(
    pub LabelSearch,
    labels,
    |names: &[String], terms: &mut Vec<String>| {
        for lbl in names {
            if let Some(neg) = lbl.strip_prefix('-') {
                terms.push(format!("-label:{neg}"));
            } else {
                terms.push(format!("label:{lbl}"));
            }
        }
    },
    |names: &[String], pr: &PullRequest| {
        for lbl in names {
            if let Some(neg) = lbl.strip_prefix('-') {
                if pr.has_label(neg) {
                    return false;
                }
            } else if !pr.has_label(lbl) {
                return false;
            }
        }
        true
    }
);

#[derive(Debug)]
pub struct BaseBranchSearch {
    pub branch: String,
}

impl SearchFilter for BaseBranchSearch {
    fn apply(&self, terms: &mut Vec<String>) {
        terms.push(format!("base:{}", self.branch));
    }

    fn matches(&self, pr: &PullRequest) -> bool {
        pr.matches_base_branch(&self.branch)
    }
}

simple_post_filter!(pub FailingCiPost, |pr: &PullRequest| {
    pr.has_failing_ci()
});

single_post_filter!(pub AuthorPost, author, |pr: &PullRequest, name: &str| {
    pr.matches_author(name)
});

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
            author_search_format: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/123".to_string(),
            labels: labels.iter().map(|label| label.to_string()).collect(),
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: base_branch.to_string(),
            commit_count,
            checks: vec![],
            recent_comments: vec![],
        }
    }

    fn terms(filter: &dyn SearchFilter) -> Vec<String> {
        let mut terms = Vec::new();
        filter.apply(&mut terms);
        terms
    }

    #[test]
    fn needs_approve_search_term_matches_local_predicate() {
        let filter = NeedsApproveSearch;

        assert_eq!(terms(&filter), vec!["-label:approved"]);
        assert!(filter.matches(&pr(&[], "main", 1)));
        assert!(!filter.matches(&pr(&["approved"], "main", 1)));
    }

    #[test]
    fn label_search_terms_match_local_predicate() {
        let filter = LabelSearch {
            labels: vec!["bug".to_string(), "-wip".to_string()],
        };

        assert_eq!(terms(&filter), vec!["label:bug", "-label:wip"]);
        assert!(filter.matches(&pr(&["bug"], "main", 1)));
        assert!(!filter.matches(&pr(&["bug", "wip"], "main", 1)));
        assert!(!filter.matches(&pr(&["feature"], "main", 1)));
    }

    #[test]
    fn base_branch_search_term_matches_local_predicate() {
        let filter = BaseBranchSearch {
            branch: "release-1.0".to_string(),
        };

        assert_eq!(terms(&filter), vec!["base:release-1.0"]);
        assert!(filter.matches(&pr(&[], "release-1.0", 1)));
        assert!(!filter.matches(&pr(&[], "main", 1)));
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
