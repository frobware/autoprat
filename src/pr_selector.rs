use crate::types::{Repo, RepoUrlError, parse_forge_url};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrIdentifier {
    pub repo: Repo,
    pub number: u64,
}

impl PrIdentifier {
    pub const fn new(repo: Repo, number: u64) -> Self {
        Self { repo, number }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrSelectorError {
    InvalidIdentifier(String),
    MissingDefaultRepo(String),
    ReversedRange { input: String, start: u64, end: u64 },
    Url(RepoUrlError),
    UrlWithoutPull(String),
}

impl std::fmt::Display for PrSelectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrSelectorError::InvalidIdentifier(input) => write!(
                f,
                "Invalid PR identifier '{input}': expected a PR number (e.g. 123), an inclusive range (e.g. 123-127), or URL (e.g. https://github.com/owner/repo/pull/123)"
            ),
            PrSelectorError::MissingDefaultRepo(input) => {
                write!(f, "PR identifier '{input}' requires --repo to be specified")
            }
            PrSelectorError::ReversedRange { input, start, end } => write!(
                f,
                "Invalid PR range '{input}': start {start} is greater than end {end}"
            ),
            PrSelectorError::Url(source) => write!(f, "{source}"),
            PrSelectorError::UrlWithoutPull(input) => {
                write!(f, "URL must contain '/pull/' in the path: {input}")
            }
        }
    }
}

impl std::error::Error for PrSelectorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PrSelectorError::Url(source) => Some(source),
            PrSelectorError::InvalidIdentifier(_)
            | PrSelectorError::MissingDefaultRepo(_)
            | PrSelectorError::ReversedRange { .. }
            | PrSelectorError::UrlWithoutPull(_) => None,
        }
    }
}

pub fn parse_pr_identifiers(
    default_repo: Option<&Repo>,
    tokens: &[String],
) -> Result<Vec<PrIdentifier>, PrSelectorError> {
    let mut identifiers = Vec::new();

    for token in tokens {
        identifiers.extend(parse_pr_identifier_token(default_repo, token)?);
    }

    Ok(identifiers)
}

pub fn parse_pr_identifier_token(
    default_repo: Option<&Repo>,
    token: &str,
) -> Result<Vec<PrIdentifier>, PrSelectorError> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(Vec::new());
    }

    if token.starts_with("https://") {
        return parse_pr_url(token).map(|identifier| vec![identifier]);
    }

    let repo = default_repo
        .cloned()
        .ok_or_else(|| PrSelectorError::MissingDefaultRepo(token.to_string()))?;

    expand_pr_number_token(token).map(|numbers| {
        numbers
            .into_iter()
            .map(|number| PrIdentifier::new(repo.clone(), number))
            .collect()
    })
}

pub fn parse_pr_url(token: &str) -> Result<PrIdentifier, PrSelectorError> {
    let parsed = parse_forge_url(token).map_err(PrSelectorError::Url)?;
    let number = extract_pr_number(&parsed.path_segments)
        .ok_or_else(|| PrSelectorError::UrlWithoutPull(token.to_string()))?;

    Ok(PrIdentifier::new(parsed.repo, number))
}

fn extract_pr_number(path_segments: &[String]) -> Option<u64> {
    let pr_keywords = ["pull", "pulls", "merge_requests", "pull-requests"];

    for keyword in &pr_keywords {
        if let Some(index) = path_segments.iter().position(|segment| segment == keyword)
            && index + 1 < path_segments.len()
            && let Ok(number) = path_segments[index + 1].parse::<u64>()
        {
            return Some(number);
        }
    }

    None
}

pub fn expand_pr_number_token(token: &str) -> Result<Vec<u64>, PrSelectorError> {
    if let Ok(n) = token.parse::<u64>() {
        return Ok(vec![n]);
    }

    if let Some((start, end)) = token.split_once('-')
        && let (Ok(start), Ok(end)) = (start.parse::<u64>(), end.parse::<u64>())
    {
        if start > end {
            return Err(PrSelectorError::ReversedRange {
                input: token.to_string(),
                start,
                end,
            });
        }
        return Ok((start..=end).collect());
    }

    Err(PrSelectorError::InvalidIdentifier(token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> Repo {
        Repo::new("openshift", "bpfman-operator").unwrap()
    }

    #[test]
    fn expands_inclusive_range() {
        assert_eq!(
            expand_pr_number_token("1967-1969").unwrap(),
            vec![1967, 1968, 1969]
        );
    }

    #[test]
    fn expands_multiple_ranges_and_singletons() {
        let identifiers = parse_pr_identifiers(
            Some(&repo()),
            &["1-3".to_string(), "9".to_string(), "11-12".to_string()],
        )
        .unwrap();
        let numbers: Vec<u64> = identifiers.into_iter().map(|id| id.number).collect();
        assert_eq!(numbers, vec![1, 2, 3, 9, 11, 12]);
    }

    #[test]
    fn treats_equal_bounds_as_single_pr() {
        assert_eq!(expand_pr_number_token("42-42").unwrap(), vec![42]);
    }

    #[test]
    fn rejects_reversed_range() {
        let err = expand_pr_number_token("1969-1967").unwrap_err();
        assert_eq!(
            err,
            PrSelectorError::ReversedRange {
                input: "1969-1967".to_string(),
                start: 1969,
                end: 1967,
            }
        );
    }

    #[test]
    fn rejects_bare_dash_with_helpful_message() {
        let err = expand_pr_number_token("-").unwrap_err().to_string();
        assert!(err.contains('-'));
        assert!(err.contains("PR number") || err.contains("URL"));
        assert!(!err.contains("invalid digit"));
    }

    #[test]
    fn rejects_non_numeric_with_helpful_message() {
        let err = expand_pr_number_token("red-hat-konflux")
            .unwrap_err()
            .to_string();
        assert!(!err.contains("invalid digit"));
        assert!(err.contains("PR number") || err.contains("URL"));
    }

    #[test]
    fn parses_pr_url_to_identifier() {
        let identifier = parse_pr_url("https://github.com/owner/repo/pull/123").unwrap();
        assert_eq!(identifier.repo, Repo::new("owner", "repo").unwrap());
        assert_eq!(identifier.number, 123);
    }

    #[test]
    fn pr_url_requires_pull_number() {
        let err = parse_pr_url("https://github.com/owner/repo").unwrap_err();
        assert_eq!(
            err,
            PrSelectorError::UrlWithoutPull("https://github.com/owner/repo".to_string())
        );
    }
}
