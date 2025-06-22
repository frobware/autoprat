use std::{collections::HashMap, time::Duration};

use anyhow::Result;
use autoprat::{CheckName, CheckUrl, LogUrl, PullRequest};
use futures::{StreamExt, stream};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use tokio::io::AsyncBufReadExt;
use tokio_stream::wrappers::LinesStream;
use tracing::debug;

#[derive(Debug)]
struct StreamResult<T> {
    pr_number: u64,
    check_name: CheckName,
    check_url: CheckUrl,
    log_url: LogUrl,
    result: Result<T>,
}

/// Details about a failed log-fetch operation, including its context.
#[derive(Debug)]
pub struct FetchError {
    pub pr_number: u64,
    pub check_name: CheckName,
    pub check_url: CheckUrl,
    pub log_url: LogUrl,
    pub error: anyhow::Error,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PR {} check '{}' ({}): {} -> {}",
            self.pr_number, self.check_name, self.check_url, self.log_url, self.error
        )
    }
}

/// Result of fetching logs for a single pull request.
///
/// Contains the PR, any successfully fetched error logs, and any fetch failures
/// that occurred when trying to get logs for this PR's failing checks.
#[derive(Debug)]
pub struct PrResult {
    pub pr: PullRequest,
    pub logs: HashMap<CheckName, Vec<String>>,
    pub fetch_errors: Vec<FetchError>,
}

/// Fetches error logs from CI systems for pull requests with failing checks.
///
/// LogFetcher identifies PRs with failing CI checks, extracts log URLs from those checks,
/// and streams the logs to identify error lines using configurable patterns.
pub struct LogFetcher {
    client: ClientWithMiddleware,
    max_concurrent: usize,
}

impl LogFetcher {
    pub fn new(max_concurrent: usize, timeout: Duration) -> Self {
        let base_client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4) // Limit connection reuse per host.
            .pool_idle_timeout(Duration::from_secs(30))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        // Add retry middleware with exponential backoff.
        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(Duration::from_millis(100), Duration::from_secs(5))
            .build_with_max_retries(3);

        let client = ClientBuilder::new(base_client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        Self {
            client,
            max_concurrent,
        }
    }

    /// Fetch error-logs for the given PRs, returning results with errors co-located per PR.
    pub async fn fetch_logs_for_prs(&self, prs: &[PullRequest]) -> Vec<PrResult> {
        let mut pr_results: HashMap<u64, PrResult> = prs
            .iter()
            .map(|pr| {
                let result = PrResult {
                    pr: pr.clone(),
                    logs: HashMap::new(),
                    fetch_errors: Vec::new(),
                };
                (pr.number, result)
            })
            .collect();

        let urls_to_fetch = self.collect_failing_check_urls(&pr_results);

        if !urls_to_fetch.is_empty() {
            struct TaskState {
                check_name: CheckName,
                error_lines: Vec<String>,
                error_count: usize,
                line_count: usize,
                pattern_matches: HashMap<String, usize>,
            }

            impl TaskState {
                fn new(check_name: CheckName) -> Self {
                    Self {
                        check_name,
                        error_lines: Vec::new(),
                        error_count: 0,
                        line_count: 0,
                        pattern_matches: HashMap::new(),
                    }
                }
            }

            let tasks: Vec<_> = urls_to_fetch
                .into_iter()
                .map(|(pr_number, check_name, check_url, log_url)| {
                    let processor = move |line: &str, state: &mut TaskState| -> bool {
                        state.line_count += 1;

                        if line.trim().is_empty() || line.len() > 500 {
                            return state.line_count < 1000;
                        }

                        if let Some(pattern_name) = is_error_line_with_pattern(line) {
                            state.error_lines.push(line.trim().to_string());
                            state.error_count += 1;

                            *state
                                .pattern_matches
                                .entry(pattern_name.to_string())
                                .or_insert(0) += 1;

                            if state.error_count >= 20 {
                                state.error_lines.push("... (truncated)".to_string());
                                return false;
                            }
                        }

                        state.line_count < 1000
                    };

                    let constructor = {
                        let check_name = check_name.clone();
                        move || TaskState::new(check_name)
                    };
                    (
                        pr_number,
                        check_name,
                        check_url,
                        log_url,
                        processor,
                        constructor,
                    )
                })
                .collect();

            let stream_results: Vec<StreamResult<TaskState>> =
                self.fetch_urls_concurrently(tasks).await;

            for stream_result in stream_results {
                if let Some(pr_result) = pr_results.get_mut(&stream_result.pr_number) {
                    match stream_result.result {
                        Ok(state) => {
                            if !state.pattern_matches.is_empty() {
                                debug!(
                                    pr_number = stream_result.pr_number,
                                    check_name = %state.check_name,
                                    total_errors = state.error_count,
                                    total_lines = state.line_count,
                                    patterns = ?state.pattern_matches,
                                    "Error pattern match statistics"
                                );
                            }

                            if !state.error_lines.is_empty() {
                                pr_result.logs.insert(state.check_name, state.error_lines);
                            }
                        }
                        Err(e) => {
                            pr_result.fetch_errors.push(FetchError {
                                pr_number: stream_result.pr_number,
                                check_name: stream_result.check_name,
                                check_url: stream_result.check_url,
                                log_url: stream_result.log_url,
                                error: e,
                            });
                        }
                    }
                }
            }
        }

        prs.iter()
            .filter_map(|pr| pr_results.remove(&pr.number))
            .collect()
    }

    async fn fetch_urls_concurrently<F, T, C>(
        &self,
        tasks: Vec<(u64, CheckName, CheckUrl, LogUrl, F, C)>,
    ) -> Vec<StreamResult<T>>
    where
        F: FnMut(&str, &mut T) -> bool + Send + 'static,
        C: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        if tasks.is_empty() {
            return Vec::new();
        }

        let client = self.client.clone();

        stream::iter(tasks)
            .map(
                move |(pr_number, check_name, check_url, log_url, mut processor, constructor)| {
                    let client = client.clone();
                    let log_url_clone = log_url.clone();

                    async move {
                        let result = async {
                            let response = client
                                .get(log_url_clone.as_str())
                                .send()
                                .await
                                .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

                            if !response.status().is_success() {
                                return Err(anyhow::anyhow!(
                                    "HTTP {} from {}",
                                    response.status(),
                                    log_url_clone
                                ));
                            }

                            let bytes_stream = response.bytes_stream();
                            let reader = tokio_util::io::StreamReader::new(
                                bytes_stream.map(|result| result.map_err(std::io::Error::other)),
                            );
                            let buf_reader = tokio::io::BufReader::new(reader);
                            let lines_stream = LinesStream::new(buf_reader.lines());

                            let mut result = constructor();
                            let mut lines_stream = std::pin::pin!(lines_stream);

                            while let Some(line_result) = lines_stream.next().await {
                                let line = line_result
                                    .map_err(|e| anyhow::anyhow!("Failed to read line: {}", e))?;

                                if !processor(&line, &mut result) {
                                    break;
                                }
                            }

                            Ok(result)
                        }
                        .await;

                        StreamResult {
                            pr_number,
                            check_name,
                            check_url,
                            log_url,
                            result,
                        }
                    }
                },
            )
            .buffer_unordered(self.max_concurrent)
            .collect()
            .await
    }

    fn collect_failing_check_urls(
        &self,
        pr_results: &HashMap<u64, PrResult>,
    ) -> Vec<(u64, CheckName, CheckUrl, LogUrl)> {
        let mut urls_to_fetch = Vec::new();

        for pr_result in pr_results.values() {
            for check in &pr_result.pr.checks {
                if check.is_failed() {
                    if let Some(url) = &check.url {
                        if let Some(log_url) = self.ci_url_to_log_url(url) {
                            urls_to_fetch.push((
                                pr_result.pr.number,
                                check.name.clone(),
                                url.clone(),
                                log_url,
                            ));
                        }
                    }
                }
            }
        }

        urls_to_fetch
    }

    fn ci_url_to_log_url(&self, url: &CheckUrl) -> Option<LogUrl> {
        if url.host() == Some("prow.ci.openshift.org") && url.path().contains("/view/gs/") {
            // Prow CI: Convert view URL to raw log URL.
            let new_url = format!(
                "https://storage.googleapis.com{}/build-log.txt",
                url.path().replace("/view/gs", "")
            );
            LogUrl::new(&new_url).ok()
        } else if url.host() == Some("github.com") && url.path().contains("/actions/runs/") {
            // GitHub Actions: We can't directly fetch logs without auth.
            None
        } else if url.as_str().contains("raw") || url.host() == Some("storage.googleapis.com") {
            // Already a raw URL, try to convert directly.
            LogUrl::new(url.as_str()).ok()
        } else if url.as_str().contains("#issuecomment") {
            // Skip issue comment URLs.
            None
        } else {
            // Unknown URL format.
            None
        }
    }
}

fn is_error_line_with_pattern(line: &str) -> Option<&'static str> {
    use std::sync::LazyLock;

    use regex::RegexSet;

    static ERROR_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
        RegexSet::new([
            // Standard error keywords.
            r"(?i)error:",
            r"(?i)failed:",
            r"(?i)failure:",
            r"(?i)fatal:",
            r"(?i)panic:",
            r"^E ",
            r"^FAIL ",
            r"(?i)exit code.*[1-9]",
            // Common logging libraries.
            r"level=error",       // Logrus.
            r#""level":"error""#, // Zap JSON.
            r"ERROR \[",          // Java/Spring.
            r"(?i)error \|",      // Some structured loggers.
            // Kubernetes-specific patterns.
            r"Warning \w+",          // Pod events (Warning FailedMount, etc.).
            r"(?i)crashloopbackoff", // Pod crash states.
            r"(?i)imagepullbackoff", // Image pull failures.
            r"(?i)evicted",          // Pod evictions.
            // CI-specific patterns.
            r"::error::",                  // GitHub Actions.
            r"make: \*\*\*.*Error \d+",    // Make build errors.
            r"Error response from daemon", // Docker errors.
            r"(?i)build failed",           // Generic build failures.
            r"(?i)test failed",            // Test failures.
            // GitHub Actions Runner patterns.
            r"##\[error\]", // GitHub Actions error annotations.
            r"Process completed with exit code [1-9]", // Runner process failures.
            r"(?i)runner.*error", // Runner-specific errors.
            r"(?i)workflow.*failed", // Workflow failures.
            r"(?i)action.*failed", // Action failures.
            // Prow/Tide patterns.
            r"level=error.*prow",      // Prow component errors.
            r"level=error.*tide",      // Tide component errors.
            r"(?i)prow.*error",        // General Prow errors.
            r"(?i)tide.*error",        // General Tide errors.
            r"(?i)presubmit.*failed",  // Presubmit job failures.
            r"(?i)postsubmit.*failed", // Postsubmit job failures.
            r"(?i)periodic.*failed",   // Periodic job failures.
            r"(?i)prowjob.*failed",    // ProwJob failures.
            r"(?i)hook.*error",        // Prow hook errors.
            r"(?i)deck.*error",        // Prow deck errors.
            r"(?i)spyglass.*error",    // Prow spyglass errors.
            r"(?i)crier.*error",       // Prow crier errors.
            r"(?i)sinker.*error",      // Prow sinker errors.
            // Other CI systems.
            r"(?i)jenkins.*error",   // Jenkins errors.
            r"(?i)tekton.*error",    // Tekton pipeline errors.
            r"(?i)gitlab.*error",    // GitLab CI errors.
            r"(?i)circleci.*error",  // CircleCI errors.
            r"(?i)travis.*error",    // Travis CI errors.
            r"(?i)buildkite.*error", // Buildkite errors.
            r"(?i)concourse.*error", // Concourse CI errors.
            // Go error patterns.
            r#"err="[^"]*""#, // Go structured error fields.
            r"(?i)cannot ",   // Go "cannot do X" errors.
            // Additional common patterns.
            r"(?i)exception:",  // Exception logs.
            r"(?i)traceback",   // Python tracebacks.
            r"(?i)stack trace", // Stack traces.
        ])
        .expect("Failed to compile error patterns")
    });

    // Pattern names corresponding to the regex patterns above.
    static PATTERN_NAMES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
        vec![
            "error-keyword",
            "failed-keyword",
            "failure-keyword",
            "fatal-keyword",
            "panic-keyword",
            "error-prefix",
            "fail-prefix",
            "exit-code",
            "logrus-error",
            "zap-json-error",
            "java-spring-error",
            "structured-logger-error",
            "k8s-warning-events",
            "k8s-crashloop",
            "k8s-imagepull",
            "k8s-evicted",
            "github-actions-error",
            "make-error",
            "docker-daemon-error",
            "build-failed",
            "test-failed",
            "github-actions-annotation",
            "process-exit-code",
            "runner-error",
            "workflow-failed",
            "action-failed",
            "prow-component-error",
            "tide-component-error",
            "prow-general-error",
            "tide-general-error",
            "presubmit-failed",
            "postsubmit-failed",
            "periodic-failed",
            "prowjob-failed",
            "prow-hook-error",
            "prow-deck-error",
            "prow-spyglass-error",
            "prow-crier-error",
            "prow-sinker-error",
            "jenkins-error",
            "tekton-error",
            "gitlab-error",
            "circleci-error",
            "travis-error",
            "buildkite-error",
            "concourse-error",
            "go-error-field",
            "go-cannot-error",
            "exception-logs",
            "python-traceback",
            "stack-trace",
        ]
    });

    let matches = ERROR_PATTERNS.matches(line);
    if let Some(index) = matches.iter().next() {
        PATTERN_NAMES.get(index).copied()
    } else {
        None
    }
}
