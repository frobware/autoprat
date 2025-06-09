package main

// Export internal functions for testing

var BuildQuery = buildQuery

// ExportedParseAndValidateArgs wraps parseAndValidateArgs for testing.
// We can't test it directly due to pflag global state, but we can test buildQuery
// which is the main logic.
func ExportedBuildQuery(template QueryTemplate, value string, values []string) (string, error) {
	return buildQuery(template, value, values)
}
