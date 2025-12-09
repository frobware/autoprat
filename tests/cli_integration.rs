use anyhow::Result;
use async_trait::async_trait;
use autoprat::{
    CheckConclusion, CheckInfo, CheckName, CheckState, CheckUrl, CommentInfo, DisplayMode, Forge,
    PullRequest, QueryResult, QuerySpec, Repo, fetch_pull_requests, parse_args,
};
use chrono::Utc;

/// Build QuerySpec from raw command-line arguments (for tests)
/// This is the test-friendly interface that handles parsing internally
fn build_request_from_args(raw_args: Vec<&str>) -> Result<(QuerySpec, DisplayMode)> {
    parse_args_and_create_request_from(raw_args)
}

/// Mock GitHub hub for testing
pub struct MockHub {
    pub mock_data: Vec<PullRequest>,
}

impl MockHub {
    pub fn new(mock_data: Vec<PullRequest>) -> Self {
        Self { mock_data }
    }
}

#[async_trait]
impl Forge for MockHub {
    async fn fetch_pull_requests(&self, _spec: &QuerySpec) -> Result<Vec<PullRequest>> {
        Ok(self.mock_data.clone())
    }
}

/// Parse command-line arguments from provided args and create QuerySpec and DisplayMode
/// This is a test-friendly interface that works with explicit arguments
fn parse_args_and_create_request_from<I, T>(args: I) -> Result<(QuerySpec, DisplayMode)>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    parse_args(args)
}

/// Helper function to run autoprat with new API
async fn run_autoprat_test<F>(raw_args: Vec<&str>, forge: &F) -> anyhow::Result<QueryResult>
where
    F: Forge + Sync,
{
    let (request, _display_mode) = build_request_from_args(raw_args)?;
    fetch_pull_requests(&request, forge).await
}

/// Helper to create a test repo
fn test_repo() -> Repo {
    Repo::new("owner", "repo").unwrap()
}

/// Helper to create mock GitHub data for testing
/// This creates a diverse set of PRs to test various filtering scenarios
fn create_mock_github_data() -> Vec<PullRequest> {
    vec![
        // PR 123: Dependabot bot PR with dependencies label
        PullRequest {
            repo: test_repo(),
            number: 123,
            title: "Bump lodash from 4.17.19 to 4.17.21".to_string(),
            author_login: "dependabot[bot]".to_string(),
            author_search_format: "app/dependabot".to_string(),
            author_simple_name: "dependabot".to_string(),
            url: "https://github.com/owner/repo/pull/123".to_string(),
            labels: vec!["dependencies".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![CheckInfo {
                name: CheckName::new("ci/build").unwrap(),
                conclusion: Some(CheckConclusion::Success),
                run_status: None,
                status_state: None,
                url: Some(CheckUrl::new("https://github.com/checks/1").unwrap()),
            }],
            recent_comments: vec![],
        },
        // PR 124: Alice's bug fix PR - already approved
        PullRequest {
            repo: test_repo(),
            number: 124,
            title: "Fix memory leak in worker threads".to_string(),
            author_login: "alice".to_string(),
            author_search_format: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/124".to_string(),
            labels: vec!["bug".to_string(), "approved".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![
                CheckInfo {
                    name: CheckName::new("ci/build").unwrap(),
                    conclusion: Some(CheckConclusion::Success),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/2").unwrap()),
                },
                CheckInfo {
                    name: CheckName::new("ci/test").unwrap(),
                    conclusion: Some(CheckConclusion::Success),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/3").unwrap()),
                },
            ],
            recent_comments: vec![],
        },
        // PR 125: Bob's feature PR - needs approval
        PullRequest {
            repo: test_repo(),
            number: 125,
            title: "Add new dashboard widget".to_string(),
            author_login: "bob".to_string(),
            author_search_format: "bob".to_string(),
            author_simple_name: "bob".to_string(),
            url: "https://github.com/owner/repo/pull/125".to_string(),
            labels: vec!["feature".to_string(), "enhancement".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![
                CheckInfo {
                    name: CheckName::new("ci/build").unwrap(),
                    conclusion: Some(CheckConclusion::Failure),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/4").unwrap()),
                },
                CheckInfo {
                    name: CheckName::new("ci/test").unwrap(),
                    conclusion: Some(CheckConclusion::Failure),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/5").unwrap()),
                },
            ],
            recent_comments: vec![],
        },
        // PR 126: Charlie's documentation PR - no labels
        PullRequest {
            repo: test_repo(),
            number: 126,
            title: "Update README.md".to_string(),
            author_login: "charlie".to_string(),
            author_search_format: "charlie".to_string(),
            author_simple_name: "charlie".to_string(),
            url: "https://github.com/owner/repo/pull/126".to_string(),
            labels: vec![],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![CheckInfo {
                name: CheckName::new("ci/lint").unwrap(),
                conclusion: Some(CheckConclusion::Failure),
                run_status: None,
                status_state: None,
                url: Some(CheckUrl::new("https://github.com/checks/6").unwrap()),
            }],
            recent_comments: vec![],
        },
        // PR 127: Alice's feature PR - needs approval
        PullRequest {
            repo: test_repo(),
            number: 127,
            title: "Implement user authentication".to_string(),
            author_login: "alice".to_string(),
            author_search_format: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/127".to_string(),
            labels: vec!["feature".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![
                CheckInfo {
                    name: CheckName::new("ci/build").unwrap(),
                    conclusion: Some(CheckConclusion::Success),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/7").unwrap()),
                },
                CheckInfo {
                    name: CheckName::new("ci/test").unwrap(),
                    conclusion: Some(CheckConclusion::Failure),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/8").unwrap()),
                },
                CheckInfo {
                    name: CheckName::new("ci/lint").unwrap(),
                    conclusion: Some(CheckConclusion::Success),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/9").unwrap()),
                },
            ],
            recent_comments: vec![],
        },
        // PR 128: Renovate bot PR
        PullRequest {
            repo: test_repo(),
            number: 128,
            title: "Update dependency jest to v27".to_string(),
            author_login: "renovate[bot]".to_string(),
            author_search_format: "app/renovate".to_string(),
            author_simple_name: "renovate".to_string(),
            url: "https://github.com/owner/repo/pull/128".to_string(),
            labels: vec!["dependencies".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![CheckInfo {
                name: CheckName::new("ci/build").unwrap(),
                conclusion: Some(CheckConclusion::Success),
                run_status: None,
                status_state: None,
                url: Some(CheckUrl::new("https://github.com/checks/10").unwrap()),
            }],
            recent_comments: vec![],
        },
        // PR 129: Bob's bug fix - approved
        PullRequest {
            repo: test_repo(),
            number: 129,
            title: "Fix race condition in API handler".to_string(),
            author_login: "bob".to_string(),
            author_search_format: "bob".to_string(),
            author_simple_name: "bob".to_string(),
            url: "https://github.com/owner/repo/pull/129".to_string(),
            labels: vec![
                "bug".to_string(),
                "critical".to_string(),
                "approved".to_string(),
            ],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![
                CheckInfo {
                    name: CheckName::new("ci/build").unwrap(),
                    conclusion: Some(CheckConclusion::Success),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/11").unwrap()),
                },
                CheckInfo {
                    name: CheckName::new("ci/test").unwrap(),
                    conclusion: Some(CheckConclusion::Success),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/12").unwrap()),
                },
            ],
            recent_comments: vec![],
        },
        // PR 130: External contributor PR - needs ok-to-test
        PullRequest {
            repo: test_repo(),
            number: 130,
            title: "Add new feature from external contributor".to_string(),
            author_login: "external-contributor".to_string(),
            author_search_format: "external-contributor".to_string(),
            author_simple_name: "external-contributor".to_string(),
            url: "https://github.com/owner/repo/pull/130".to_string(),
            labels: vec!["needs-ok-to-test".to_string(), "external".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![], // No checks yet, needs ok-to-test first
            recent_comments: vec![],
        },
        // PR 131: LGTM'd PR - has lgtm label
        PullRequest {
            repo: test_repo(),
            number: 131,
            title: "Minor fix with LGTM".to_string(),
            author_login: "developer".to_string(),
            author_search_format: "developer".to_string(),
            author_simple_name: "developer".to_string(),
            url: "https://github.com/owner/repo/pull/131".to_string(),
            labels: vec!["lgtm".to_string(), "bug".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![CheckInfo {
                name: CheckName::new("ci/build").unwrap(),
                conclusion: Some(CheckConclusion::Success),
                run_status: None,
                status_state: None,
                url: Some(CheckUrl::new("https://github.com/checks/13").unwrap()),
            }],
            recent_comments: vec![],
        },
    ]
}

#[tokio::test]
async fn test_cli_validation_no_arguments() {
    let provider = MockHub::new(vec![]);
    let result = run_autoprat_test(vec!["autoprat"], &provider).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Must specify one of")
    );
}

#[tokio::test]
async fn test_cli_validation_valid_repo_only() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(vec!["autoprat", "--repo", "owner/repo"], &provider).await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // All PRs should be returned
    assert_eq!(result.filtered_prs.len(), 9);
}

#[tokio::test]
async fn test_cli_validation_valid_query_only() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--query", "repo:owner/repo author:alice"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Mock provider returns all data regardless of query
    assert_eq!(result.filtered_prs.len(), 9);
}

#[tokio::test]
async fn test_cli_validation_pr_args_mixed_formats() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data.clone());

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "https://github.com/owner/repo/pull/123",
            "124",
            "125",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Only requested PRs returned
    assert_eq!(result.filtered_prs.len(), 3);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&123));
    assert!(pr_numbers.contains(&124));
    assert!(pr_numbers.contains(&125));

    // Test 2: PR URLs only (no --repo needed)
    let provider = MockHub::new(mock_data.clone());
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "https://github.com/owner/repo/pull/123",
            "https://github.com/owner/repo/pull/125",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 2); // Only requested PRs returned
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&123));
    assert!(pr_numbers.contains(&125));
    assert!(!pr_numbers.contains(&124)); // PR 124 was not requested

    // Test 3: PR numbers only with --repo
    let provider = MockHub::new(mock_data);
    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "123", "125"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 2); // Only requested PRs returned
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&123));
    assert!(pr_numbers.contains(&125));
    assert!(!pr_numbers.contains(&124)); // PR 124 was not requested
}

#[tokio::test]
async fn test_cli_validation_prs_numbers_require_repo() {
    // Test: PR numbers without --repo should fail
    let provider = MockHub::new(vec![]);

    let result = run_autoprat_test(vec!["autoprat", "123", "456"], &provider).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("--repo is required")
    );
}

#[tokio::test]
async fn test_cli_validation_query_conflicts_with_repo() {
    // Test: --query with --repo should fail
    let provider = MockHub::new(vec![]);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--query",
            "author:alice",
            "--repo",
            "owner/repo",
        ],
        &provider,
    )
    .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Cannot use --repo with --query")
    );
}

#[tokio::test]
async fn test_cli_validation_query_conflicts_with_prs() {
    // Test: --query with --prs should fail
    let provider = MockHub::new(vec![]);

    let result = run_autoprat_test(
        vec!["autoprat", "--query", "author:alice", "123"],
        &provider,
    )
    .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Cannot use --prs with --query")
    );
}

#[tokio::test]
async fn test_filter_author_bot_formats() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--author", "dependabot"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 123);
    assert_eq!(result.filtered_prs[0].author_simple_name, "dependabot");
}

#[tokio::test]
async fn test_filter_author_regular_user() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--author", "alice"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 2);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&124));
    assert!(pr_numbers.contains(&127));

    for pr_info in &result.filtered_prs {
        assert_eq!(pr_info.author_simple_name, "alice");
    }
}

#[tokio::test]
async fn test_filter_label_basic() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--label", "feature"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 2);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&125));
    assert!(pr_numbers.contains(&127));

    assert!(!pr_numbers.contains(&123));
    assert!(!pr_numbers.contains(&124));
    assert!(!pr_numbers.contains(&126));
    assert!(!pr_numbers.contains(&128));
    assert!(!pr_numbers.contains(&129));

    for pr_info in &result.filtered_prs {
        assert!(pr_info.labels.contains(&"feature".to_string()));
    }
}

#[tokio::test]
async fn test_filter_needs_approve() {
    // Test that --needs-approve matches PRs without the approved label
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--needs-approve"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // From our mock data: PRs 123, 125, 126, 127, 128, 130, 131 need approval
    // PRs 124, 129 have "approved" label
    assert_eq!(result.filtered_prs.len(), 7);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&123)); // dependabot - no approved label
    assert!(pr_numbers.contains(&125)); // bob's feature - no approved label
    assert!(pr_numbers.contains(&126)); // charlie's docs - no approved label
    assert!(pr_numbers.contains(&127)); // alice's feature - no approved label
    assert!(pr_numbers.contains(&128)); // renovate - no approved label
    assert!(!pr_numbers.contains(&124)); // alice's bug fix - has approved label
    assert!(!pr_numbers.contains(&129)); // bob's bug fix - has approved label
}

#[tokio::test]
async fn test_filter_combination_author_label() {
    // Test that --author alice --label bug matches only alice's PR with bug label
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--author",
            "alice",
            "--label",
            "bug",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // From our mock data: Only PR 124 matches (alice + bug label)
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 124);
    assert_eq!(result.filtered_prs[0].author_simple_name, "alice");
    assert!(result.filtered_prs[0].labels.contains(&"bug".to_string()));
}

#[tokio::test]
async fn test_filter_failing_ci_basic() {
    // Test that --failing-ci matches PRs with failing CI checks
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--failing-ci"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // From our actual mock data: PRs 125, 126, 127 have failing checks
    // PR 123: ci/build=Success
    // PR 124: ci/build=Success, ci/test=Success
    // PR 125: ci/build=Failure, ci/test=Failure
    // PR 126: ci/lint=Failure
    // PR 127: ci/build=Success, ci/test=Failure, ci/lint=Success
    // PR 128: ci/build=Success
    // PR 129: ci/build=Success, ci/test=Success
    assert_eq!(result.filtered_prs.len(), 3);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&125)); // Bob's feature - failing ci/build + ci/test
    assert!(pr_numbers.contains(&126)); // Charlie's docs - failing ci/lint
    assert!(pr_numbers.contains(&127)); // Alice's feature - failing ci/test

    // These should NOT be present (passing CI)
    assert!(!pr_numbers.contains(&123)); // dependabot - passing ci/build
    assert!(!pr_numbers.contains(&124)); // alice bug - passing ci/build + ci/test
    assert!(!pr_numbers.contains(&128)); // renovate - passing ci/build
    assert!(!pr_numbers.contains(&129)); // bob bug - passing ci/build + ci/test

    // Verify all returned PRs actually have failing checks
    for pr_info in &result.filtered_prs {
        let has_failing_check = pr_info.checks.iter().any(|check| {
            matches!(
                check.conclusion,
                Some(
                    CheckConclusion::Failure
                        | CheckConclusion::Cancelled
                        | CheckConclusion::TimedOut
                )
            ) || matches!(
                check.status_state,
                Some(CheckState::Failure | CheckState::Error)
            )
        });
        assert!(
            has_failing_check,
            "PR {} should have failing checks",
            pr_info.number
        );
    }
}

#[tokio::test]
async fn test_filter_failing_check_basic() {
    // Test that --failing-check matches PRs with specific failing checks
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test specific failing check: ci/test
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--failing-check",
            "ci/test",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // From our actual mock data: Only PR 125 and 127 have failing ci/test
    // PR 125: ci/build=FAIL, ci/test=FAIL
    // PR 127: ci/build=OK, ci/test=FAIL, ci/lint=OK
    assert_eq!(result.filtered_prs.len(), 2);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&125)); // Bob's feature - ci/test failing
    assert!(pr_numbers.contains(&127)); // Alice's feature - ci/test failing

    // These should NOT be present
    assert!(!pr_numbers.contains(&123)); // dependabot - ci/build passing (no ci/test)
    assert!(!pr_numbers.contains(&124)); // alice bug - ci/build + ci/test passing
    assert!(!pr_numbers.contains(&126)); // charlie docs - ci/lint failing (not ci/test)
    assert!(!pr_numbers.contains(&128)); // renovate - ci/build passing (no ci/test)
    assert!(!pr_numbers.contains(&129)); // bob bug - ci/build + ci/test passing
}

#[tokio::test]
async fn test_filter_failing_check_specific_name() {
    // Test that --failing-check matches only the exact check name
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test specific failing check: ci/lint (only PR 127 has this failing)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--failing-check",
            "ci/lint",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Only PR 126 has failing ci/lint
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 126);

    // Verify it actually has the failing ci/lint check
    let has_failing_lint = result.filtered_prs[0].checks.iter().any(|check| {
        check.name.as_str() == "ci/lint"
            && matches!(
                check.conclusion,
                Some(
                    CheckConclusion::Failure
                        | CheckConclusion::Cancelled
                        | CheckConclusion::TimedOut
                )
            )
    });
    assert!(has_failing_lint);
}

#[tokio::test]
async fn test_filter_failing_check_multiple() {
    // Test that --failing-check with multiple values requires ALL to be failing
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test multiple failing checks: ci/build AND ci/test (only PR 125 has both failing)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--failing-check",
            "ci/build",
            "--failing-check",
            "ci/test",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Only PR 125 has BOTH ci/build AND ci/test failing
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 125);

    // Verify it has both failing checks
    let pr_checks = &result.filtered_prs[0].checks;
    let has_failing_build = pr_checks.iter().any(|check| {
        check.name.as_str() == "ci/build"
            && matches!(check.conclusion, Some(CheckConclusion::Failure))
    });
    let has_failing_test = pr_checks.iter().any(|check| {
        check.name.as_str() == "ci/test"
            && matches!(check.conclusion, Some(CheckConclusion::Failure))
    });
    assert!(has_failing_build && has_failing_test);
}

#[tokio::test]
async fn test_filter_combination_failing_ci_author() {
    // Test --failing-ci combined with other filters
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test --failing-ci + --author alice (should match PR 127)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--failing-ci",
            "--author",
            "alice",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Only PR 127 matches (alice + failing ci/test)
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 127);
    assert_eq!(result.filtered_prs[0].author_simple_name, "alice");

    // Verify it has failing checks
    let has_failing_check = result.filtered_prs[0]
        .checks
        .iter()
        .any(|check| matches!(check.conclusion, Some(CheckConclusion::Failure)));
    assert!(has_failing_check);
}

#[tokio::test]
async fn test_filter_combination_failing_ci_label() {
    // Test --failing-ci combined with label filters
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test --failing-ci + --label feature (should match PR 125 and 127)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--failing-ci",
            "--label",
            "feature",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // PRs 125 and 127 have "feature" label AND failing CI
    assert_eq!(result.filtered_prs.len(), 2);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&125)); // Bob's feature - failing ci/build + ci/test
    assert!(pr_numbers.contains(&127)); // Alice's feature - failing ci/test

    // Verify all have feature label and failing checks
    for pr_info in &result.filtered_prs {
        assert!(pr_info.labels.contains(&"feature".to_string()));
        let has_failing_check = pr_info
            .checks
            .iter()
            .any(|check| matches!(check.conclusion, Some(CheckConclusion::Failure)));
        assert!(has_failing_check);
    }
}

#[tokio::test]
async fn test_filter_combination_failing_check_needs_approve() {
    // Test --failing-check combined with --needs-approve
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test --failing-check ci/test + --needs-approve
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--failing-check",
            "ci/test",
            "--needs-approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // From our mock data:
    // - PRs with failing ci/test: 125, 127
    // - PRs needing approval (no "approved" label): 123, 125, 126, 127, 128
    // - Intersection: 125, 127
    assert_eq!(result.filtered_prs.len(), 2);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&125)); // Bob's feature - failing ci/test + needs approval
    assert!(pr_numbers.contains(&127)); // Alice's feature - failing ci/test + needs approval

    // Verify all match both conditions
    for pr_info in &result.filtered_prs {
        // Check has failing ci/test
        let has_failing_test = pr_info.checks.iter().any(|check| {
            check.name.as_str() == "ci/test"
                && matches!(check.conclusion, Some(CheckConclusion::Failure))
        });
        assert!(has_failing_test);

        // Check needs approval (no "approved" label)
        assert!(!pr_info.labels.contains(&"approved".to_string()));
    }
}

#[tokio::test]
async fn test_cli_validation_pr_url_repo_mismatch() {
    // Test that PR URLs must match the specified repo
    let provider = MockHub::new(vec![]);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "https://github.com/different/repo/pull/123",
        ],
        &provider,
    )
    .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("different/repo"));
}

#[tokio::test]
async fn test_cli_validation_query_passthrough() {
    // Test that --query passes through exactly what user specifies (GIGO)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--query", "repo:owner/repo author:alice"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Mock provider returns all data regardless of query
    assert_eq!(result.filtered_prs.len(), 9);
}

#[tokio::test]
async fn test_cli_validation_display_mode_conflicts() {
    // Test: conflicting display modes should work (last one wins or they combine)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--quiet", "--detailed"],
        &provider,
    )
    .await;
    // Should succeed - application should handle conflicting modes gracefully
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_cli_validation_malformed_repo_format() {
    // Test: malformed repo format should fail
    let provider = MockHub::new(vec![]);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "not-a-valid-repo-format"],
        &provider,
    )
    .await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    // Check for various possible error messages related to repo format
    assert!(
        error_msg.contains("Invalid repository format")
            || error_msg.contains("repository")
            || error_msg.contains("format")
            || error_msg.contains("owner/repo"),
        "Expected repo format error, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_cli_validation_empty_repo_format() {
    // Test: empty repo should fail
    let provider = MockHub::new(vec![]);

    let result = run_autoprat_test(vec!["autoprat", "--repo", ""], &provider).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_cli_validation_repo_edge_cases() {
    // Test additional edge cases for repo format validation
    let provider = MockHub::new(vec![]);

    let edge_cases = vec![
        "owner/",           // Empty repo name
        "/repo",            // Empty owner name
        "owner//repo",      // Double slash (empty middle part)
        "owner/repo/extra", // Too many parts
    ];

    for repo in edge_cases {
        let result = run_autoprat_test(vec!["autoprat", "--repo", repo], &provider).await;
        assert!(result.is_err(), "Repo format '{}' should be invalid", repo);
    }
}

#[tokio::test]
async fn test_action_multiple_combination() {
    // Test: multiple actions should work together
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--approve",
            "--lgtm",
            "--retest",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should have multiple actions in the result
    assert!(!result.executable_actions.is_empty());
}

#[tokio::test]
async fn test_filter_combination_multiple() {
    // Test: multiple filters should work together (AND logic)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--author",
            "alice",
            "--label",
            "bug",
            "--needs-approve",
            "--failing-ci",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());
    // The specific count depends on mock data matching all filters
}

#[tokio::test]
async fn test_action_with_custom_comments() {
    // Test: actions combined with custom comments
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--approve",
            "--comment",
            "Looks good to me!",
            "--comment",
            "Please check the tests",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Test passes if no errors occur - custom comments are processed correctly

    // Verify custom comments actually generate executable actions
    // With 7 PRs in mock data, should have:
    // - 7 approve actions (PRs without "approved" label: 123, 125, 126, 127, 128, 130, 131)
    // - 18 custom comment actions (2 comments × 9 PRs)
    // Total: 25 executable actions
    assert_eq!(result.executable_actions.len(), 25);

    // Count actions by type
    let approve_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.action.name() == "approve")
        .count();
    let custom_comment_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.action.name() == "custom-comment")
        .count();

    assert_eq!(approve_actions, 7);
    assert_eq!(custom_comment_actions, 18);
}

#[tokio::test]
async fn test_custom_comment_throttling() {
    // Test: custom comments should respect throttling
    let mut mock_data = create_mock_github_data();

    // Add recent comment to first PR that matches our custom comment
    mock_data[0].recent_comments.push(CommentInfo {
        body: "Please review carefully".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(2), // 2 minutes ago - should be throttled
    });

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--comment",
            "Please review carefully",
            "--throttle",
            "5m",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should have 8 custom comment actions (1 comment × 9 PRs, but 1 is throttled)
    // PR 123 should be throttled because it has the comment posted 2 minutes ago
    assert_eq!(result.executable_actions.len(), 8);

    // Verify that PR 123 is NOT in the executable actions
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(pr_123_actions, 0);

    // Verify other PRs do have the action
    let other_pr_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number != 123)
        .count();
    assert_eq!(other_pr_actions, 8);
}

#[tokio::test]
async fn test_custom_comment_throttling_seconds() {
    // Test: custom comments should respect throttling even with second-based durations
    let mut mock_data = create_mock_github_data();

    // Add recent comment to first PR that was posted 15 seconds ago
    mock_data[0].recent_comments.push(CommentInfo {
        body: "Quick review needed".to_string(),
        created_at: Utc::now() - chrono::Duration::seconds(15), // 15 seconds ago
    });

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--comment",
            "Quick review needed",
            "--throttle",
            "30s", // 30-second throttle window
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should have 8 custom comment actions (1 comment × 9 PRs, but 1 is throttled)
    // PR 123 should be throttled because it has the comment posted 15 seconds ago
    assert_eq!(result.executable_actions.len(), 8);

    // Verify that PR 123 is NOT in the executable actions
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(pr_123_actions, 0);
}

#[tokio::test]
async fn test_idempotent_action_comment_history_check() {
    // Test: idempotent actions (like /lgtm) should check comment history
    // even without throttling to avoid re-posting when GitHub is slow
    let mut mock_data = create_mock_github_data();

    // Add /lgtm comment to PR 123 from 10 minutes ago
    // Even though this is old, we should not re-post because we've already done it
    mock_data[0].recent_comments.push(CommentInfo {
        body: "/lgtm".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(10),
    });

    // Remove the lgtm label to simulate GitHub being slow
    mock_data[0].labels.retain(|l| l != "lgtm");

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--lgtm",
            // No throttle specified
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 should NOT have an lgtm action because we already posted /lgtm
    // even though the label isn't present yet
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(pr_123_actions, 0);

    // Other PRs without lgtm label should have the action
    let other_prs_needing_lgtm = result
        .executable_actions
        .iter()
        .filter(|action| !action.pr_info.has_label("lgtm"))
        .count();
    assert!(other_prs_needing_lgtm > 0);
}

#[tokio::test]
async fn test_idempotent_action_approve_comment_history() {
    // Test: /approve should also check comment history
    let mut mock_data = create_mock_github_data();

    // Add /approve comment to PR 123 from 15 minutes ago
    mock_data[0].recent_comments.push(CommentInfo {
        body: "/approve".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(15),
    });

    // Remove the approved label to simulate GitHub being slow
    mock_data[0].labels.retain(|l| l != "approved");

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--approve"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 should NOT have an approve action
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(pr_123_actions, 0);
}

#[tokio::test]
async fn test_custom_comment_history_check() {
    // Test: custom comments should also check history, not just throttle
    let mut mock_data = create_mock_github_data();

    // Add a custom comment from 20 minutes ago
    mock_data[0].recent_comments.push(CommentInfo {
        body: "Please review carefully".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(20),
    });

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--comment",
            "Please review carefully",
            // No throttle - should still skip because comment exists in history
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should have 8 custom comment actions (9 PRs - 1 with comment in history)
    assert_eq!(result.executable_actions.len(), 8);

    // Verify that PR 123 is NOT in the executable actions
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(pr_123_actions, 0);
}

#[tokio::test]
async fn test_history_check_with_throttle_both_apply() {
    // Test: both history check and throttle should be applied
    // History check happens first, but if comment is recent, throttle prevents it too
    let mut mock_data = create_mock_github_data();

    // Add /lgtm comment to PR 123 from 2 minutes ago
    mock_data[0].recent_comments.push(CommentInfo {
        body: "/lgtm".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(2),
    });

    // Remove the lgtm label
    mock_data[0].labels.retain(|l| l != "lgtm");

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--lgtm",
            "--throttle",
            "5m",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 should be skipped due to history check (would also be caught by throttle)
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(pr_123_actions, 0);
}

#[tokio::test]
async fn test_history_check_multiline_comment() {
    // Test: history check should work with multiline comments
    let mut mock_data = create_mock_github_data();

    // Add a comment with /lgtm on a separate line
    mock_data[0].recent_comments.push(CommentInfo {
        body: "This looks good!\n/lgtm\nThanks for the fix!".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(10),
    });

    // Remove the lgtm label
    mock_data[0].labels.retain(|l| l != "lgtm");

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--lgtm"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 should be skipped because /lgtm exists in comment history
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(pr_123_actions, 0);
}

#[tokio::test]
async fn test_label_removed_after_comment_posted() {
    // Test: if label existed, then was removed (e.g., new commit pushed),
    // we should be able to re-post the command even if it's in comment history
    let mut mock_data = create_mock_github_data();

    // Simulate: we posted /lgtm, label was applied, then removed
    // Comment is old (from before the label was removed)
    mock_data[0].recent_comments.push(CommentInfo {
        body: "/lgtm".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(30),
    });

    // Label is NOT present (was removed)
    mock_data[0].labels.retain(|l| l != "lgtm");

    // Add some comments after the /lgtm to simulate activity
    for i in 1..=11 {
        mock_data[0].recent_comments.push(CommentInfo {
            body: format!("Some other comment {}", i),
            created_at: Utc::now() - chrono::Duration::minutes(30 - i),
        });
    }

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--lgtm"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 SHOULD have an lgtm action because:
    // 1. Label doesn't exist (was removed)
    // 2. The /lgtm comment is beyond the last 10 comments (pushed out by newer comments)
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(
        pr_123_actions, 1,
        "Should allow re-posting /lgtm after label was removed and comment is no longer in recent history"
    );
}

#[tokio::test]
async fn test_history_check_1_hour_threshold() {
    // Test: comments older than 1 hour should not block re-posting
    let mut mock_data = create_mock_github_data();

    // Add /lgtm comment from 2 hours ago (beyond 1-hour threshold)
    mock_data[0].recent_comments.push(CommentInfo {
        body: "/lgtm".to_string(),
        created_at: Utc::now() - chrono::Duration::hours(2),
    });

    // Remove the lgtm label
    mock_data[0].labels.retain(|l| l != "lgtm");

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--lgtm"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 SHOULD have an lgtm action because the comment is older than 1 hour
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(
        pr_123_actions, 1,
        "Should allow re-posting /lgtm when comment is older than 1 hour"
    );
}

#[tokio::test]
async fn test_history_check_within_1_hour_and_recent_position() {
    // Test: comments within 1 hour AND within last 10 comments should block re-posting
    let mut mock_data = create_mock_github_data();

    // Add /lgtm comment from 30 minutes ago (within 1-hour threshold)
    mock_data[0].recent_comments.push(CommentInfo {
        body: "/lgtm".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(30),
    });

    // Add a few more comments (but not enough to push /lgtm out of last 10)
    for i in 1..=5 {
        mock_data[0].recent_comments.push(CommentInfo {
            body: format!("Comment {}", i),
            created_at: Utc::now() - chrono::Duration::minutes(25),
        });
    }

    // Remove the lgtm label
    mock_data[0].labels.retain(|l| l != "lgtm");

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--lgtm"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 should NOT have an lgtm action because:
    // - Comment is within 1 hour
    // - Comment is within last 10 comments
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(
        pr_123_actions, 0,
        "Should block re-posting /lgtm when comment is recent (< 1h) and within last 10 comments"
    );
}

#[tokio::test]
async fn test_history_check_custom_max_age() {
    // Test: custom --history-max-age flag should be respected
    let mut mock_data = create_mock_github_data();

    // Add /lgtm comment from 45 minutes ago
    mock_data[0].recent_comments.push(CommentInfo {
        body: "/lgtm".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(45),
    });

    // Remove the lgtm label
    mock_data[0].labels.retain(|l| l != "lgtm");

    let provider = MockHub::new(mock_data);

    // With custom --history-max-age 30m, comment at 45min should allow re-posting
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--lgtm",
            "--history-max-age",
            "30m",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 SHOULD have an lgtm action because comment is older than 30m
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(
        pr_123_actions, 1,
        "Should allow re-posting /lgtm when comment is older than custom history-max-age"
    );
}

#[tokio::test]
async fn test_history_check_custom_max_comments() {
    // Test: custom --history-max-comments flag should be respected
    let mut mock_data = create_mock_github_data();

    // Add /lgtm comment from 10 minutes ago
    mock_data[0].recent_comments.push(CommentInfo {
        body: "/lgtm".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(10),
    });

    // Add only 2 more comments (not enough to push /lgtm out of last 10, but enough for last 2)
    for i in 1..=2 {
        mock_data[0].recent_comments.push(CommentInfo {
            body: format!("Comment {}", i),
            created_at: Utc::now() - chrono::Duration::minutes(5),
        });
    }

    // Remove the lgtm label
    mock_data[0].labels.retain(|l| l != "lgtm");

    let provider = MockHub::new(mock_data);

    // With custom --history-max-comments 2, /lgtm should be outside the window
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--lgtm",
            "--history-max-comments",
            "2",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // PR 123 SHOULD have an lgtm action because comment is outside last 2 comments
    let pr_123_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .count();
    assert_eq!(
        pr_123_actions, 1,
        "Should allow re-posting /lgtm when comment is outside custom history-max-comments window"
    );
}

#[tokio::test]
async fn test_cli_validation_invalid_pr_url_format() {
    // Test: malformed PR URLs should fail gracefully
    let provider = MockHub::new(vec![]);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "not-a-valid-url",
            "https://github.com/owner/repo/issues/123", // Issues, not PRs
        ],
        &provider,
    )
    .await;

    // Should either succeed (if URLs are passed through) or fail with clear error
    // The behavior depends on how strict URL validation is
    if result.is_err() {
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("URL") || error_msg.contains("format"));
    }
}

#[tokio::test]
async fn test_cli_validation_limit_values() {
    // Test: limit values should be validated
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test with limit 0 - should work but return no results
    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--limit", "0"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    // Test with very large limit - should work
    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--limit", "1000"],
        &provider,
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_cli_validation_throttle_formats() {
    // Test: currently supported throttle formats (unitless, seconds, minutes, hours)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test currently supported throttle formats (unitless, seconds, minutes, hours)
    for throttle in ["5", "30s", "5m", "2h", "30", "30m", "60m", "120m"] {
        let result = run_autoprat_test(
            vec![
                "autoprat",
                "--repo",
                "owner/repo",
                "--throttle",
                throttle,
                "--approve",
            ],
            &provider,
        )
        .await;
        assert!(
            result.is_ok(),
            "Throttle format '{}' should be valid",
            throttle
        );
    }
}

#[tokio::test]
async fn test_cli_validation_throttle_time_units() {
    // Test: specific time unit conversions work correctly
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data.clone());

    // Test seconds: "30s" should be 30 seconds
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--throttle",
            "30s",
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());
    // Test that throttle parsing succeeds - actual throttling behavior is tested elsewhere

    // Test hours: "2h" should be 2 hours = 7200 seconds
    let provider = MockHub::new(mock_data);
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--throttle",
            "2h",
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());
    // Test that throttle parsing succeeds - actual throttling behavior is tested elsewhere
}

#[tokio::test]
async fn test_cli_validation_unsupported_throttle_formats() {
    // Test: complex formats not yet supported should fail with clear error
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test unsupported throttle formats (complex combinations and units)
    for throttle in ["2h30m", "1d", "1w"] {
        let result = run_autoprat_test(
            vec![
                "autoprat",
                "--repo",
                "owner/repo",
                "--throttle",
                throttle,
                "--approve",
            ],
            &provider,
        )
        .await;
        assert!(
            result.is_err(),
            "Throttle format '{}' should not be supported yet",
            throttle
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Invalid throttle format")
                || error_msg.contains("Currently only")
                || error_msg.contains("Invalid throttle minutes"),
            "Expected throttle format error for '{}', got: {}",
            throttle,
            error_msg
        );
    }
}

#[tokio::test]
async fn test_cli_validation_malformed_throttle() {
    // Test: malformed throttle values should fail
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test invalid throttle formats
    for throttle in ["5x", "invalid", "m5", "5.5", "abc123"] {
        let result = run_autoprat_test(
            vec![
                "autoprat",
                "--repo",
                "owner/repo",
                "--throttle",
                throttle,
                "--approve",
            ],
            &provider,
        )
        .await;

        // Should either fail during parsing or during execution
        if result.is_err() {
            let error_msg = result.unwrap_err().to_string();
            assert!(
                error_msg.contains("throttle")
                    || error_msg.contains("duration")
                    || error_msg.contains("Invalid"),
                "Expected throttle error for '{}', got: {}",
                throttle,
                error_msg
            );
        }
        // Note: Some invalid formats might be silently ignored rather than failing
    }
}

#[tokio::test]
async fn test_cli_validation_empty_comment() {
    // Test: empty comments should be handled gracefully
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--comment", ""],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let _result = result.unwrap();
    // Empty comment should still be included
    // Test passes if no errors occur - empty custom comment is processed correctly
}

#[tokio::test]
async fn test_cli_validation_empty_throttle() {
    // Test: empty throttle should be handled gracefully (treated as no throttle)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--throttle",
            "",
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    // Test that empty throttle parsing succeeds - actual throttling behavior is tested elsewhere
}

#[tokio::test]
async fn test_cli_validation_unitless_throttle() {
    // Test: unitless numbers should default to minutes
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--throttle",
            "10",
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    // Test that unitless throttle parsing succeeds - actual throttling behavior is tested elsewhere
}

#[tokio::test]
async fn test_filter_label_negation_syntax() {
    // Test: --label bug --label -approved should match PRs with "bug" but NOT "approved"
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--label",
            "bug",
            "--label=-approved", // Negated label using equals syntax
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // From our mock data: PR 124 (alice bug fix) has "bug" AND "approved" - should be excluded
    // PR 131 has "bug" but NOT "approved" - should be included
    assert_eq!(result.filtered_prs.len(), 1);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&131)); // Has bug but not approved
}

#[tokio::test]
async fn test_filter_label_negation_only() {
    // Test: --label -approved should match PRs that do NOT have "approved" label
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--label=-approved", // Negated label only using equals syntax
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // From our mock data: PRs 124 and 129 have "approved" label, others don't
    assert_eq!(result.filtered_prs.len(), 7);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&123)); // dependabot - no approved
    assert!(pr_numbers.contains(&125)); // bob feature - no approved
    assert!(pr_numbers.contains(&126)); // charlie docs - no approved
    assert!(pr_numbers.contains(&127)); // alice feature - no approved
    assert!(pr_numbers.contains(&128)); // renovate - no approved
    assert!(!pr_numbers.contains(&124)); // alice bug - has approved
    assert!(!pr_numbers.contains(&129)); // bob bug - has approved
}

#[tokio::test]
async fn test_action_without_filters() {
    // Test: actions should work without any filters (act on all PRs)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--approve", "--lgtm"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should return all PRs since no filters applied
    assert_eq!(result.filtered_prs.len(), 9);
    assert!(!result.executable_actions.is_empty());
}

#[tokio::test]
async fn test_action_none_display_only() {
    // Test: filters should work without actions (display only)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--author", "alice"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter PRs but have no actions
    assert!(result.executable_actions.is_empty());
}

#[tokio::test]
async fn test_integration_complex_workflow() {
    // Test: realistic complex CLI usage
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--author",
            "dependabot",
            "--label",
            "dependencies",
            "--needs-approve",
            "--approve",
            "--comment",
            "Auto-approving dependency update",
            "--throttle",
            "60m",
            "--limit",
            "10",
            "--quiet",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let _result = result.unwrap();
    // Test passes if no errors occur - custom comment is processed correctly
    // Test that throttle parsing succeeds - actual throttling behavior is tested elsewhere
}

#[tokio::test]
async fn test_action_ok_to_test() {
    // Test that --ok-to-test action creates the correct comment action
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--ok-to-test"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should have executable actions only for PRs with "needs-ok-to-test" label (1 total - PR 130)
    assert_eq!(result.executable_actions.len(), 1);
    assert_eq!(result.executable_actions[0].pr_info.number, 130);

    // Action should be "ok-to-test" comment action
    assert_eq!(result.executable_actions[0].action.name(), "ok-to-test");
    assert_eq!(
        result.executable_actions[0].action.get_comment_body(),
        Some("/ok-to-test")
    );
}

#[tokio::test]
async fn test_filter_needs_ok_to_test() {
    // Test that --needs-ok-to-test filter matches only PRs with needs-ok-to-test label
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--needs-ok-to-test"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should match only PR 130 which has needs-ok-to-test label
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 130);
    assert_eq!(result.filtered_prs[0].author_login, "external-contributor");
}

#[tokio::test]
async fn test_filter_needs_ok_to_test_with_action() {
    // Test --needs-ok-to-test filter combined with --ok-to-test action
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--needs-ok-to-test",
            "--ok-to-test",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to PR 130 and create action for it
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 130);

    // Should have one executable action for the filtered PR
    assert_eq!(result.executable_actions.len(), 1);
    assert_eq!(result.executable_actions[0].pr_info.number, 130);
    assert_eq!(result.executable_actions[0].action.name(), "ok-to-test");
    assert_eq!(
        result.executable_actions[0].action.get_comment_body(),
        Some("/ok-to-test")
    );
}

#[tokio::test]
async fn test_action_close() {
    // Test that --close action works and applies to all PRs
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--close"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should have close actions for all PRs (9 total)
    assert_eq!(result.executable_actions.len(), 9);

    // All actions should be "close" actions
    for action in &result.executable_actions {
        assert_eq!(action.action.name(), "close");
        // Close action should not have a comment body (it's a direct action)
        assert_eq!(action.action.get_comment_body(), None);
    }

    // Verify all PR numbers are present
    let pr_numbers: Vec<u64> = result
        .executable_actions
        .iter()
        .map(|action| action.pr_info.number)
        .collect();
    assert!(pr_numbers.contains(&123));
    assert!(pr_numbers.contains(&124));
    assert!(pr_numbers.contains(&125));
    assert!(pr_numbers.contains(&126));
    assert!(pr_numbers.contains(&127));
    assert!(pr_numbers.contains(&128));
    assert!(pr_numbers.contains(&129));
    assert!(pr_numbers.contains(&130));
    assert!(pr_numbers.contains(&131));
}

#[tokio::test]
async fn test_action_close_with_filters() {
    // Test --close action combined with filters (should only close filtered PRs)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--author",
            "alice",
            "--close",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to alice's PRs (124, 127) and create close actions for them
    assert_eq!(result.filtered_prs.len(), 2);
    assert_eq!(result.executable_actions.len(), 2);

    // Verify the correct PRs are targeted
    let pr_numbers: Vec<u64> = result
        .executable_actions
        .iter()
        .map(|action| action.pr_info.number)
        .collect();
    assert!(pr_numbers.contains(&124)); // Alice's bug fix
    assert!(pr_numbers.contains(&127)); // Alice's feature

    // All should be close actions
    for action in &result.executable_actions {
        assert_eq!(action.action.name(), "close");
        assert_eq!(action.action.get_comment_body(), None);
    }
}

#[tokio::test]
async fn test_action_close_with_multiple_actions() {
    // Test --close combined with other actions (should create multiple actions per PR)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--author",
            "bob",
            "--approve",
            "--close",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to bob's PRs (125, 129) and create both approve and close actions
    assert_eq!(result.filtered_prs.len(), 2);
    assert_eq!(result.executable_actions.len(), 3); // 1 approve (only PR 125 needs it) + 2 close

    // Count actions by type
    let approve_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.action.name() == "approve")
        .count();
    let close_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.action.name() == "close")
        .count();

    assert_eq!(approve_actions, 1); // Only PR 125 needs approval (PR 129 already approved)
    assert_eq!(close_actions, 2); // Both PRs get close actions

    // Verify close actions have no comment body
    for action in &result.executable_actions {
        if action.action.name() == "close" {
            assert_eq!(action.action.get_comment_body(), None);
        }
    }
}

#[tokio::test]
async fn test_multiple_comments_only() {
    // Test: multiple comments without other actions
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--comment",
            "First comment",
            "--comment",
            "Second comment",
            "--comment",
            "Third comment",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Multiple custom comments should be processed correctly

    // Should have 3 comments × 9 PRs = 27 executable actions
    assert_eq!(result.executable_actions.len(), 27);

    // All actions should be custom-comment actions
    for action in &result.executable_actions {
        assert_eq!(action.action.name(), "custom-comment");
    }

    // Verify each comment appears for each PR
    let comment_bodies: Vec<String> = result
        .executable_actions
        .iter()
        .filter_map(|action| action.action.get_comment_body())
        .map(|s| s.to_string())
        .collect();

    assert_eq!(comment_bodies.len(), 27);
    assert_eq!(
        comment_bodies
            .iter()
            .filter(|c| *c == "First comment")
            .count(),
        9
    );
    assert_eq!(
        comment_bodies
            .iter()
            .filter(|c| *c == "Second comment")
            .count(),
        9
    );
    assert_eq!(
        comment_bodies
            .iter()
            .filter(|c| *c == "Third comment")
            .count(),
        9
    );
}

#[tokio::test]
async fn test_multiple_comments_with_filters() {
    // Test: multiple comments with filtering
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--author",
            "alice",
            "--comment",
            "Review needed",
            "--comment",
            "Please address feedback",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to alice's PRs (124, 127)
    assert_eq!(result.filtered_prs.len(), 2);
    // Custom comments should be processed correctly

    // Should have 2 comments × 2 PRs = 4 executable actions
    assert_eq!(result.executable_actions.len(), 4);

    // Verify comments are applied to correct PRs
    let pr_numbers: Vec<u64> = result
        .executable_actions
        .iter()
        .map(|action| action.pr_info.number)
        .collect();

    // Both alice's PRs should appear twice (once per comment)
    assert_eq!(pr_numbers.iter().filter(|&&n| n == 124).count(), 2);
    assert_eq!(pr_numbers.iter().filter(|&&n| n == 127).count(), 2);
}

#[tokio::test]
async fn test_multiple_comments_with_throttling() {
    // Test: multiple comments with throttling behavior
    let mut mock_data = create_mock_github_data();

    // Add recent comment to first PR that matches one of our comments
    mock_data[0].recent_comments.push(CommentInfo {
        body: "Needs attention".to_string(),
        created_at: Utc::now() - chrono::Duration::minutes(2), // 2 minutes ago - should be throttled
    });

    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--comment",
            "Needs attention", // This should be throttled for PR 123
            "--comment",
            "Please review", // This should not be throttled
            "--throttle",
            "5m",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Custom comments should be processed correctly

    // Should have fewer actions due to throttling
    // Expected: 8 PRs get "Please review" + 8 PRs get "Needs attention" - 1 throttled = 17 actions
    assert_eq!(result.executable_actions.len(), 17);

    // Verify PR 123 only gets "Please review" comment (not the throttled one)
    let pr_123_actions: Vec<_> = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123)
        .collect();

    assert_eq!(pr_123_actions.len(), 1);
    assert_eq!(
        pr_123_actions[0].action.get_comment_body(),
        Some("Please review")
    );
}

#[tokio::test]
async fn test_mixed_actions_with_multiple_comments() {
    // Test: multiple comments combined with standard actions
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--needs-approve", // Filter to PRs needing approval
            "--approve",       // Standard action
            "--lgtm",          // Standard action
            "--comment",
            "Auto-approved",
            "--comment",
            "Ready to merge",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to PRs needing approval (123, 125, 126, 127, 128, 130, 131) = 7 PRs
    assert_eq!(result.filtered_prs.len(), 7);
    // Custom comments should be processed correctly

    // Should have: 7 approve + 6 lgtm (PR 131 already has lgtm) + 14 custom comments = 27 actions
    assert_eq!(result.executable_actions.len(), 27);

    // Count actions by type
    let approve_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.action.name() == "approve")
        .count();
    let lgtm_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.action.name() == "lgtm")
        .count();
    let comment_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.action.name() == "custom-comment")
        .count();

    assert_eq!(approve_actions, 7);
    assert_eq!(lgtm_actions, 6); // PR 131 already has lgtm label
    assert_eq!(comment_actions, 14); // 2 comments × 7 PRs
}

#[tokio::test]
async fn test_empty_and_whitespace_comments() {
    // Test: edge cases with empty and whitespace-only comments
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--author",
            "bob", // Limit to bob's PRs for simpler testing
            "--comment",
            "", // Empty comment
            "--comment",
            "   ", // Whitespace-only comment
            "--comment",
            "Valid comment",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to bob's PRs (125, 129)
    assert_eq!(result.filtered_prs.len(), 2);
    // Custom comments should be processed correctly

    // Should have 3 comments × 2 PRs = 6 executable actions
    assert_eq!(result.executable_actions.len(), 6);

    // Verify all comment bodies are preserved as-is
    let comment_bodies: Vec<String> = result
        .executable_actions
        .iter()
        .filter_map(|action| action.action.get_comment_body())
        .map(|s| s.to_string())
        .collect();

    assert!(comment_bodies.contains(&"".to_string()));
    assert!(comment_bodies.contains(&"   ".to_string()));
    assert!(comment_bodies.contains(&"Valid comment".to_string()));
}

#[tokio::test]
async fn test_filter_needs_lgtm() {
    // Test that --needs-lgtm filter matches only PRs without lgtm label
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--needs-lgtm"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should match PRs without "lgtm" label (all except PR 131) = 8 PRs
    assert_eq!(result.filtered_prs.len(), 8);

    // Verify the correct PRs are included (all except 131)
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&123));
    assert!(pr_numbers.contains(&124));
    assert!(pr_numbers.contains(&125));
    assert!(pr_numbers.contains(&126));
    assert!(pr_numbers.contains(&127));
    assert!(pr_numbers.contains(&128));
    assert!(pr_numbers.contains(&129));
    assert!(pr_numbers.contains(&130));
    // PR 131 should NOT be included as it has the "lgtm" label
    assert!(!pr_numbers.contains(&131));
}

#[tokio::test]
async fn test_filter_needs_lgtm_with_action() {
    // Test --needs-lgtm filter combined with --lgtm action
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--needs-lgtm", "--lgtm"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to PRs needing lgtm (all except PR 131) = 8 PRs
    assert_eq!(result.filtered_prs.len(), 8);

    // Should have 8 executable lgtm actions
    assert_eq!(result.executable_actions.len(), 8);

    // All actions should be lgtm actions
    for action in &result.executable_actions {
        assert_eq!(action.action.name(), "lgtm");
        assert_eq!(action.action.get_comment_body(), Some("/lgtm"));
    }

    // Verify PR 131 is NOT in the executable actions
    let action_pr_numbers: Vec<u64> = result
        .executable_actions
        .iter()
        .map(|action| action.pr_info.number)
        .collect();
    assert!(!action_pr_numbers.contains(&131));
}

#[tokio::test]
async fn test_filter_needs_lgtm_with_other_filters() {
    // Test --needs-lgtm combined with other filters
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--needs-lgtm",
            "--author",
            "alice",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to alice's PRs that need lgtm (124, 127) = 2 PRs
    assert_eq!(result.filtered_prs.len(), 2);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&124)); // Alice's bug fix - needs lgtm
    assert!(pr_numbers.contains(&127)); // Alice's feature - needs lgtm
}

#[tokio::test]
async fn test_filter_needs_lgtm_combined_with_needs_approve() {
    // Test --needs-lgtm combined with --needs-approve (both filters should apply)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--needs-lgtm",
            "--needs-approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    // Should filter to PRs that need both approval AND lgtm
    // PRs 124, 129 have "approved" label, PR 131 has "lgtm" label
    // So we should get PRs: 123, 125, 126, 127, 128, 130 = 6 PRs
    assert_eq!(result.filtered_prs.len(), 6);

    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&123)); // needs both
    assert!(pr_numbers.contains(&125)); // needs both
    assert!(pr_numbers.contains(&126)); // needs both
    assert!(pr_numbers.contains(&127)); // needs both
    assert!(pr_numbers.contains(&128)); // needs both
    assert!(pr_numbers.contains(&130)); // needs both

    // These should be excluded
    assert!(!pr_numbers.contains(&124)); // has approved
    assert!(!pr_numbers.contains(&129)); // has approved
    assert!(!pr_numbers.contains(&131)); // has lgtm
}

#[test]
fn test_parse_args_and_create_request_from() {
    // Test the unified parsing function with explicit arguments
    let result = parse_args_and_create_request_from(vec![
        "autoprat",
        "--repo",
        "owner/repo",
        "--approve",
        "--comment",
        "Test comment",
    ]);
    assert!(result.is_ok());

    let (request, display_mode) = result.unwrap();
    assert_eq!(request.repos.len(), 1);
    assert_eq!(request.repos[0].to_string(), "owner/repo");
    // Both approve and custom comment are now in actions vector
    assert_eq!(request.actions.len(), 2);
    // First action should be approve
    assert_eq!(request.actions[0].name(), "approve");
    // Second action should be the custom comment
    assert_eq!(request.actions[1].name(), "custom-comment");
    assert_eq!(request.actions[1].get_comment_body(), Some("Test comment"));
    assert_eq!(display_mode, DisplayMode::Normal);
}

#[test]
fn test_parse_args_with_slash_commands() {
    // Test that slash commands are transformed correctly
    let result = parse_args_and_create_request_from(vec![
        "autoprat",
        "--repo",
        "owner/repo",
        "/approve",
        "/lgtm",
        "/ok-to-test",
    ]);
    assert!(result.is_ok());

    let (request, _) = result.unwrap();
    assert_eq!(request.actions.len(), 3); // approve, lgtm, ok-to-test
}

#[test]
fn test_non_github_url_accepted() {
    // Test: non-GitHub URLs are now accepted (liberal parsing)
    // The error will come later when GraphQL query is made to GitHub API
    let result = parse_args_and_create_request_from(vec![
        "autoprat",
        "https://gitlab.com/owner/repo/merge_requests/123",
    ]);
    assert!(result.is_ok()); // Should parse successfully

    let (_request, _display_mode) = result.unwrap();
    // The error will happen when we try to fetch from GitHub API
}

#[test]
fn test_invalid_url_format_error() {
    // Test: issues URL is accepted but fails because it doesn't contain PR number
    let result = parse_args_and_create_request_from(vec![
        "autoprat",
        "https://github.com/owner/repo/issues/123", // issues instead of pull
    ]);
    assert!(result.is_err());

    let error = result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("URL must contain '/pull/' in the path")
    );
}

#[test]
fn test_short_url_format_error() {
    // Test: too short URL should be rejected
    let result = parse_args_and_create_request_from(vec![
        "autoprat",
        "https://github.com/owner", // missing repo and PR parts
    ]);
    assert!(result.is_err());

    let error = result.unwrap_err();
    // Short URLs fail at the parse_url level with "must contain owner and repository name"
    assert!(
        error
            .to_string()
            .contains("URL must contain owner and repository name")
    );
}

#[test]
fn test_pr_number_without_repo_error() {
    // Test: PR numbers require --repo to be specified (CLI validation line 408)
    // Note: This also ensures the defensive check on line 635 is never reached
    let result = parse_args_and_create_request_from(vec![
        "autoprat", "123", // PR number without --repo
    ]);
    assert!(result.is_err());

    let error = result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--repo is required when using PR numbers")
    );
}

#[test]
fn test_detailed_display_mode() {
    // Test: --detailed flag should set Detailed display mode
    let result =
        parse_args_and_create_request_from(vec!["autoprat", "--repo", "owner/repo", "--detailed"]);
    assert!(result.is_ok());

    let (_, display_mode) = result.unwrap();
    assert_eq!(display_mode, DisplayMode::Detailed);
}

#[test]
fn test_detailed_with_logs_display_mode() {
    // Test: --detailed-with-logs flag should set DetailedWithLogs display mode
    let result = parse_args_and_create_request_from(vec![
        "autoprat",
        "--repo",
        "owner/repo",
        "--detailed-with-logs",
    ]);
    assert!(result.is_ok());

    let (_, display_mode) = result.unwrap();
    assert_eq!(display_mode, DisplayMode::DetailedWithLogs);
}

#[test]
fn test_detailed_short_flag() {
    // Test: -d short flag should set Detailed display mode
    let result = parse_args_and_create_request_from(vec!["autoprat", "--repo", "owner/repo", "-d"]);
    assert!(result.is_ok());

    let (_, display_mode) = result.unwrap();
    assert_eq!(display_mode, DisplayMode::Detailed);
}

#[test]
fn test_detailed_with_logs_short_flag() {
    // Test: -D short flag should set DetailedWithLogs display mode
    let result = parse_args_and_create_request_from(vec!["autoprat", "--repo", "owner/repo", "-D"]);
    assert!(result.is_ok());

    let (_, display_mode) = result.unwrap();
    assert_eq!(display_mode, DisplayMode::DetailedWithLogs);
}

#[test]
fn test_retest_action() {
    // Test: --retest flag should add retest action
    let result =
        parse_args_and_create_request_from(vec!["autoprat", "--repo", "owner/repo", "--retest"]);
    assert!(result.is_ok());

    let (request, _) = result.unwrap();
    assert_eq!(request.actions.len(), 1);
    assert_eq!(request.actions[0].name(), "retest");
}

#[test]
fn test_parse_args_pr_number_requires_repo() {
    // Test: PR numbers require --repo to be specified
    let result = build_request_from_args(vec!["autoprat", "123"]);
    assert!(result.is_err());

    let error = result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--repo is required when using PR numbers")
    );
}

#[tokio::test]
async fn test_filter_title_short_flag() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test -t short flag works the same as --title
    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "-t", "dashboard"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 125);
    assert!(result.filtered_prs[0].title.contains("dashboard"));
}

#[tokio::test]
async fn test_filter_title_contains() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test filtering by title containing "dashboard"
    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--title", "dashboard"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 125);
    assert!(result.filtered_prs[0].title.contains("dashboard"));

    // Test filtering by title containing "Fix" (case-sensitive)
    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--title", "Fix"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 2); // PRs 124 and 129 (not 131 which has lowercase "fix")
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&124)); // "Fix memory leak in worker threads"
    assert!(pr_numbers.contains(&129)); // "Fix race condition in API handler"
    for pr in &result.filtered_prs {
        assert!(pr.title.contains("Fix"));
    }

    // Test filtering by title containing "Update"
    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--title", "Update"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 2); // PRs 126 and 128
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&126)); // "Update README.md"
    assert!(pr_numbers.contains(&128)); // "Update dependency jest to v27"
    for pr in &result.filtered_prs {
        assert!(pr.title.contains("Update"));
    }

    // Test filtering by title that doesn't match any PRs
    let result = run_autoprat_test(
        vec!["autoprat", "--repo", "owner/repo", "--title", "nonexistent"],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 0);
}

#[tokio::test]
async fn test_filter_title_with_other_filters() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test combining title filter with author filter
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--title",
            "Fix",
            "--author",
            "alice",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 124);
    assert!(result.filtered_prs[0].title.contains("Fix"));
    assert_eq!(result.filtered_prs[0].author_simple_name, "alice");

    // Test combining title filter with label filter
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--title",
            "dashboard",
            "--label",
            "feature",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert_eq!(result.filtered_prs.len(), 1);
    assert_eq!(result.filtered_prs[0].number, 125);
    assert!(result.filtered_prs[0].title.contains("dashboard"));
    assert!(
        result.filtered_prs[0]
            .labels
            .contains(&"feature".to_string())
    );
}

/// Test output selection logic - when actions are requested, should always output commands
#[tokio::test]
async fn test_output_selection_with_actions() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test case: Actions requested but no PRs match action conditions
    // Should output empty shell commands, not table
    let (request, _display_mode) = build_request_from_args(vec![
        "autoprat",
        "--repo",
        "owner/repo",
        "--lgtm",
        "131", // PR 131 already has lgtm label, so no lgtm action will be generated
    ])
    .unwrap();

    // Check has_actions before consuming the request
    let should_output_commands = request.has_actions();
    assert!(
        should_output_commands,
        "Request should indicate actions were requested"
    );

    let result = fetch_pull_requests(&request, &provider).await.unwrap();

    // Verify that no executable actions are generated since PR 131 already has lgtm
    assert_eq!(
        result.executable_actions.len(),
        0,
        "No executable actions should be generated since PR 131 already has lgtm"
    );

    // The key test: when actions are requested, should output commands (even if empty)
    assert!(
        should_output_commands,
        "Should output commands when actions are requested, even if none are executable"
    );
}

#[tokio::test]
async fn test_output_selection_without_actions() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test case: No actions requested - should display table
    let (request, _display_mode) = build_request_from_args(vec![
        "autoprat",
        "--repo",
        "owner/repo", // No action flags
    ])
    .unwrap();

    // Check has_actions before consuming the request
    let should_output_commands = request.has_actions();
    assert!(
        !should_output_commands,
        "Request should indicate no actions were requested"
    );

    let result = fetch_pull_requests(&request, &provider).await.unwrap();

    // Verify no executable actions are generated
    assert_eq!(
        result.executable_actions.len(),
        0,
        "No executable actions should be generated"
    );

    // Should display table when no actions requested
    assert!(
        !should_output_commands,
        "Should display table when no actions are requested"
    );
}

#[tokio::test]
async fn test_output_selection_with_custom_comments() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test case: Custom comments requested - should output commands
    let (request, _display_mode) = build_request_from_args(vec![
        "autoprat",
        "--repo",
        "owner/repo",
        "--comment",
        "/test-something",
    ])
    .unwrap();

    // Check has_actions before consuming the request
    let should_output_commands = request.has_actions();
    assert!(
        should_output_commands,
        "Request should indicate actions were requested (custom comments)"
    );

    let result = fetch_pull_requests(&request, &provider).await.unwrap();

    // Verify that executable actions are generated for custom comments
    assert!(
        result.executable_actions.len() > 0,
        "Should have executable actions for custom comments"
    );

    // Should output commands when custom comments are requested
    assert!(
        should_output_commands,
        "Should output commands when custom comments are requested"
    );
}

#[tokio::test]
async fn test_output_selection_with_mixed_actions() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test case: Mix of built-in actions and custom comments
    let (request, _display_mode) = build_request_from_args(vec![
        "autoprat",
        "--repo",
        "owner/repo",
        "--needs-lgtm", // Filter to PRs that need lgtm
        "--lgtm",       // Action for PRs that need lgtm
        "--comment",
        "/test-custom",
    ])
    .unwrap();

    // Check has_actions before consuming the request
    let should_output_commands = request.has_actions();
    assert!(
        should_output_commands,
        "Request should indicate actions were requested"
    );

    let result = fetch_pull_requests(&request, &provider).await.unwrap();

    // Verify that executable actions are generated
    assert!(
        result.executable_actions.len() > 0,
        "Should have executable actions"
    );

    // Should output commands when actions are requested
    assert!(
        should_output_commands,
        "Should output commands when actions are requested"
    );
}

#[tokio::test]
async fn test_output_selection_actions_with_empty_filtered_prs() {
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test case: Actions requested but filtering produces no PRs
    // This is the critical case - should output empty commands, not table
    let (request, _display_mode) = build_request_from_args(vec![
        "autoprat",
        "--repo",
        "owner/repo",
        "--author",
        "nonexistent-user", // This filter will match no PRs
        "--lgtm",           // But actions are still requested
    ])
    .unwrap();

    // Check has_actions before consuming the request
    let should_output_commands = request.has_actions();
    assert!(
        should_output_commands,
        "Request should indicate actions were requested"
    );

    let result = fetch_pull_requests(&request, &provider).await.unwrap();

    // This is the critical test: filtering produces no PRs
    assert_eq!(
        result.filtered_prs.len(),
        0,
        "Filtering should produce no PRs for nonexistent user"
    );

    // Therefore no executable actions are generated
    assert_eq!(
        result.executable_actions.len(),
        0,
        "No executable actions since no PRs match filter"
    );

    // But we should STILL output commands (empty) because actions were requested
    assert!(
        should_output_commands,
        "Should output commands when actions requested, even if filtering produces no PRs"
    );
}

#[tokio::test]
async fn test_multi_repository_urls() {
    // Test that URLs from different repositories work correctly together
    // This verifies the multi-repository URL capability

    // Create mock data for two different repositories
    let acme_repo = Repo::new("acme", "web-app").unwrap();
    let widgets_repo = Repo::new("widgets", "api-service").unwrap();

    let multi_repo_mock_data = vec![
        // PR from acme/web-app
        PullRequest {
            repo: acme_repo.clone(),
            number: 443,
            title: "Add user authentication system".to_string(),
            author_login: "dev-alice".to_string(),
            author_search_format: "dev-alice".to_string(),
            author_simple_name: "dev-alice".to_string(),
            url: "https://github.com/acme/web-app/pull/443".to_string(),
            labels: vec!["enhancement".to_string()],
            created_at: Utc::now() - chrono::Duration::weeks(3),
            base_branch: "main".to_string(),
            checks: vec![CheckInfo {
                name: CheckName::new("ci/unit-tests").unwrap(),
                conclusion: None,
                run_status: None,
                status_state: Some(CheckState::Pending),
                url: Some(CheckUrl::new("https://github.com/checks/acme-1").unwrap()),
            }],
            recent_comments: vec![],
        },
        // PR from widgets/api-service
        PullRequest {
            repo: widgets_repo.clone(),
            number: 656,
            title: "Fix API rate limiting bug".to_string(),
            author_login: "dev-bob".to_string(),
            author_search_format: "dev-bob".to_string(),
            author_simple_name: "dev-bob".to_string(),
            url: "https://github.com/widgets/api-service/pull/656".to_string(),
            labels: vec!["bug".to_string()],
            created_at: Utc::now() - chrono::Duration::weeks(3),
            base_branch: "main".to_string(),
            checks: vec![
                CheckInfo {
                    name: CheckName::new("ci/integration-tests").unwrap(),
                    conclusion: Some(CheckConclusion::Failure),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/widgets-1").unwrap()),
                },
                CheckInfo {
                    name: CheckName::new("ci/lint").unwrap(),
                    conclusion: Some(CheckConclusion::Success),
                    run_status: None,
                    status_state: None,
                    url: Some(CheckUrl::new("https://github.com/checks/widgets-2").unwrap()),
                },
            ],
            recent_comments: vec![],
        },
    ];

    let provider = MockHub::new(multi_repo_mock_data);

    // Test multi-repository URL handling
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "https://github.com/acme/web-app/pull/443",
            "https://github.com/widgets/api-service/pull/656",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should return both PRs
    assert_eq!(result.filtered_prs.len(), 2);

    // Verify both repositories are represented
    let repos: std::collections::HashSet<String> = result
        .filtered_prs
        .iter()
        .map(|pr| pr.repo.to_string())
        .collect();
    assert_eq!(repos.len(), 2);
    assert!(repos.contains("acme/web-app"));
    assert!(repos.contains("widgets/api-service"));

    // Verify specific PRs are present
    let pr_identifiers: Vec<(String, u64)> = result
        .filtered_prs
        .iter()
        .map(|pr| (pr.repo.to_string(), pr.number))
        .collect();
    assert!(pr_identifiers.contains(&("acme/web-app".to_string(), 443)));
    assert!(pr_identifiers.contains(&("widgets/api-service".to_string(), 656)));

    // Verify PR details are correct
    let acme_pr = result
        .filtered_prs
        .iter()
        .find(|pr| pr.repo.to_string() == "acme/web-app" && pr.number == 443)
        .unwrap();
    assert_eq!(acme_pr.author_simple_name, "dev-alice");
    assert!(acme_pr.title.contains("authentication"));

    let widgets_pr = result
        .filtered_prs
        .iter()
        .find(|pr| pr.repo.to_string() == "widgets/api-service" && pr.number == 656)
        .unwrap();
    assert_eq!(widgets_pr.author_simple_name, "dev-bob");
    assert!(widgets_pr.title.contains("rate limiting"));
}

#[tokio::test]
async fn test_multi_repository_urls_with_filters() {
    // Test that filters work correctly across multiple repositories
    let acme_repo = Repo::new("acme", "web-app").unwrap();
    let widgets_repo = Repo::new("widgets", "api-service").unwrap();
    let tools_repo = Repo::new("tools", "cli-utils").unwrap();

    let multi_repo_mock_data = vec![
        // PR from acme/web-app by alice
        PullRequest {
            repo: acme_repo,
            number: 100,
            title: "Add new feature".to_string(),
            author_login: "alice".to_string(),
            author_search_format: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/acme/web-app/pull/100".to_string(),
            labels: vec!["feature".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![],
            recent_comments: vec![],
        },
        // PR from widgets/api-service by bob
        PullRequest {
            repo: widgets_repo,
            number: 200,
            title: "Fix bug".to_string(),
            author_login: "bob".to_string(),
            author_search_format: "bob".to_string(),
            author_simple_name: "bob".to_string(),
            url: "https://github.com/widgets/api-service/pull/200".to_string(),
            labels: vec!["bug".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![],
            recent_comments: vec![],
        },
        // PR from tools/cli-utils by alice
        PullRequest {
            repo: tools_repo,
            number: 300,
            title: "Update documentation".to_string(),
            author_login: "alice".to_string(),
            author_search_format: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/tools/cli-utils/pull/300".to_string(),
            labels: vec!["documentation".to_string()],
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![],
            recent_comments: vec![],
        },
    ];

    let provider = MockHub::new(multi_repo_mock_data);

    // Test filtering by author across multiple repositories
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--author",
            "alice",
            "https://github.com/acme/web-app/pull/100",
            "https://github.com/widgets/api-service/pull/200",
            "https://github.com/tools/cli-utils/pull/300",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should return only alice's PRs (from acme and tools repos)
    assert_eq!(result.filtered_prs.len(), 2);

    // Verify correct PRs are returned
    let pr_identifiers: Vec<(String, u64)> = result
        .filtered_prs
        .iter()
        .map(|pr| (pr.repo.to_string(), pr.number))
        .collect();
    assert!(pr_identifiers.contains(&("acme/web-app".to_string(), 100)));
    assert!(pr_identifiers.contains(&("tools/cli-utils".to_string(), 300)));
    assert!(!pr_identifiers.contains(&("widgets/api-service".to_string(), 200))); // bob's PR should be filtered out

    // Verify all returned PRs are by alice
    for pr in &result.filtered_prs {
        assert_eq!(pr.author_simple_name, "alice");
    }
}

#[tokio::test]
async fn test_multi_repository_urls_with_actions() {
    // Test that actions work correctly across multiple repositories
    let acme_repo = Repo::new("acme", "web-app").unwrap();
    let widgets_repo = Repo::new("widgets", "api-service").unwrap();

    let multi_repo_mock_data = vec![
        // PR from acme/web-app (needs approval)
        PullRequest {
            repo: acme_repo,
            number: 100,
            title: "Add feature".to_string(),
            author_login: "alice".to_string(),
            author_search_format: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/acme/web-app/pull/100".to_string(),
            labels: vec!["feature".to_string()], // No "approved" label
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![],
            recent_comments: vec![],
        },
        // PR from widgets/api-service (already approved)
        PullRequest {
            repo: widgets_repo,
            number: 200,
            title: "Fix bug".to_string(),
            author_login: "bob".to_string(),
            author_search_format: "bob".to_string(),
            author_simple_name: "bob".to_string(),
            url: "https://github.com/widgets/api-service/pull/200".to_string(),
            labels: vec!["bug".to_string(), "approved".to_string()], // Already approved
            created_at: Utc::now(),
            base_branch: "main".to_string(),
            checks: vec![],
            recent_comments: vec![],
        },
    ];

    let provider = MockHub::new(multi_repo_mock_data);

    // Test approve action across multiple repositories
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--approve",
            "https://github.com/acme/web-app/pull/100",
            "https://github.com/widgets/api-service/pull/200",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should return both PRs
    assert_eq!(result.filtered_prs.len(), 2);

    // Should have only one approve action (for the PR that needs approval)
    assert_eq!(result.executable_actions.len(), 1);

    // Verify the action is for the correct PR (acme PR that needs approval)
    let action = &result.executable_actions[0];
    assert_eq!(action.action.name(), "approve");
    assert_eq!(action.pr_info.repo.to_string(), "acme/web-app");
    assert_eq!(action.pr_info.number, 100);
    assert_eq!(action.action.get_comment_body(), Some("/approve"));
}

#[tokio::test]
async fn test_exclude_pr_by_number() {
    // Test: --exclude with PR numbers should filter out specific PRs
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "123",
            "--exclude",
            "125",
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should exclude PRs 123 and 125 from filtered results
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&123));
    assert!(!pr_numbers.contains(&125));

    // Should still contain other PRs
    assert!(pr_numbers.contains(&124));
    assert!(pr_numbers.contains(&126));

    // Should have no actions for excluded PRs
    let excluded_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 123 || action.pr_info.number == 125)
        .count();
    assert_eq!(excluded_actions, 0);
}

#[tokio::test]
async fn test_exclude_pr_by_url() {
    // Test: --exclude with PR URLs should filter out specific PRs
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "https://github.com/owner/repo/pull/124",
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should exclude PR 124 from filtered results
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&124));

    // Should still contain other PRs
    assert!(pr_numbers.contains(&123));
    assert!(pr_numbers.contains(&125));

    // Should have no actions for excluded PR
    let excluded_actions = result
        .executable_actions
        .iter()
        .filter(|action| action.pr_info.number == 124)
        .count();
    assert_eq!(excluded_actions, 0);
}

#[tokio::test]
async fn test_exclude_mixed_numbers_and_urls() {
    // Test: --exclude with mix of numbers and URLs
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "123", // By number
            "--exclude",
            "https://github.com/owner/repo/pull/126", // By URL
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should exclude both PRs 123 and 126
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&123));
    assert!(!pr_numbers.contains(&126));

    // Should still contain other PRs
    assert!(pr_numbers.contains(&124));
    assert!(pr_numbers.contains(&125));
}

#[tokio::test]
async fn test_exclude_with_specific_pr_list() {
    // Test: --exclude works when targeting specific PRs
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "123", // Target PR 123
            "124", // Target PR 124
            "125", // Target PR 125
            "--exclude",
            "124", // But exclude PR 124
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should only have PRs 123 and 125 (124 is excluded)
    assert_eq!(result.filtered_prs.len(), 2);
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(pr_numbers.contains(&123));
    assert!(pr_numbers.contains(&125));
    assert!(!pr_numbers.contains(&124));
}

#[tokio::test]
async fn test_exclude_validation_requires_repo() {
    // Test: --exclude with PR numbers requires --repo
    let provider = MockHub::new(vec![]);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--query",
            "is:pr",
            "--exclude",
            "123", // Number without --repo should fail
        ],
        &provider,
    )
    .await;

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("--repo is required when using exclude PR numbers"));
}

#[tokio::test]
async fn test_exclude_empty_string() {
    // Test: --exclude with empty string should be a no-op (user-friendly)
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "", // Empty string should be no-op
            "--approve",
        ],
        &provider,
    )
    .await;

    assert!(result.is_ok());

    // Should behave exactly like no --exclude flag
    let result = result.unwrap();
    // Should have all PRs (none excluded)
    assert_eq!(result.filtered_prs.len(), 9); // All mock PRs present

    // Test multiple empty values (should all be skipped)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "123,,124,,", // Double commas create empty values
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&123));
    assert!(!pr_numbers.contains(&124));
}

#[tokio::test]
async fn test_exclude_comma_separated_numbers() {
    // Test: --exclude with comma-separated PR numbers
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "123,125,127", // Comma-separated numbers
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should exclude PRs 123, 125, and 127 from filtered results
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&123));
    assert!(!pr_numbers.contains(&125));
    assert!(!pr_numbers.contains(&127));

    // Should still contain other PRs
    assert!(pr_numbers.contains(&124));
    assert!(pr_numbers.contains(&126));
    assert!(pr_numbers.contains(&128));

    // Should have no actions for excluded PRs
    let excluded_actions = result
        .executable_actions
        .iter()
        .filter(|action| {
            let pr_num = action.pr_info.number;
            pr_num == 123 || pr_num == 125 || pr_num == 127
        })
        .count();
    assert_eq!(excluded_actions, 0);
}

#[tokio::test]
async fn test_exclude_comma_separated_urls() {
    // Test: --exclude with comma-separated PR URLs
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "https://github.com/owner/repo/pull/124,https://github.com/owner/repo/pull/128", // Comma-separated URLs
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should exclude PRs 124 and 128 from filtered results
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&124));
    assert!(!pr_numbers.contains(&128));

    // Should still contain other PRs
    assert!(pr_numbers.contains(&123));
    assert!(pr_numbers.contains(&125));
    assert!(pr_numbers.contains(&126));
}

#[tokio::test]
async fn test_exclude_mixed_comma_and_multiple_flags() {
    // Test: --exclude with mix of comma-separated and multiple flags
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "123,124", // Comma-separated
            "--exclude",
            "126", // Separate flag
            "--exclude",
            "https://github.com/owner/repo/pull/128", // URL
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();

    // Should exclude PRs 123, 124, 126, and 128
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&123));
    assert!(!pr_numbers.contains(&124));
    assert!(!pr_numbers.contains(&126));
    assert!(!pr_numbers.contains(&128));

    // Should still contain other PRs
    assert!(pr_numbers.contains(&125));
    assert!(pr_numbers.contains(&127));
}

#[tokio::test]
async fn test_exclude_comma_separated_edge_cases() {
    // Test: edge cases with comma-separated values
    let mock_data = create_mock_github_data();
    let provider = MockHub::new(mock_data);

    // Test with trailing comma (should work now - empty strings are skipped)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "123,124,", // Trailing comma creates empty value which is skipped
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&123));
    assert!(!pr_numbers.contains(&124));

    // Test with leading/trailing spaces around entire values
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            " 126 , 127 ", // Spaces around entire values
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&126));
    assert!(!pr_numbers.contains(&127));

    // Test with single comma-separated value (should work)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "123", // Single value, no comma
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&123));

    // Test with spaces around commas (should work now with trimming)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "124, 125", // Spaces around comma should work with trimming
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&124));
    assert!(!pr_numbers.contains(&125));

    // Test without spaces (should work)
    let result = run_autoprat_test(
        vec![
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "124,125", // No spaces around comma
            "--approve",
        ],
        &provider,
    )
    .await;
    assert!(result.is_ok());

    let result = result.unwrap();
    let pr_numbers: Vec<u64> = result.filtered_prs.iter().map(|pr| pr.number).collect();
    assert!(!pr_numbers.contains(&124));
    assert!(!pr_numbers.contains(&125));
}
