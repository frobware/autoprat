# Build the binary.
.PHONY: build
build:
	gofmt -w .
	go build -o autoprat .

# Install to GOPATH/bin.
.PHONY: install
install:
	go install .

# Clean build artifacts.
.PHONY: clean
clean:
	rm -f autoprat

# Run tests.
.PHONY: test
test:
	go test -v ./...

# Format code.
.PHONY: fmt
fmt:
	gofmt -w .

# Format and check for changes (CI).
.PHONY: fmt-check
fmt-check:
	gofmt -w .
	git diff --exit-code

# Vet code.
.PHONY: vet
vet:
	go vet ./...

# Check for whitespace issues.
.PHONY: whitespace-check
whitespace-check:
	git diff --check HEAD

# Tidy go.mod and go.sum.
.PHONY: mod-tidy
mod-tidy:
	go mod tidy

# Verify go.mod and go.sum are tidy (CI).
.PHONY: mod-tidy-check
mod-tidy-check:
	go mod tidy
	git diff --exit-code go.mod go.sum

# Run all checks.
.PHONY: check
check: whitespace-check fmt-check vet mod-tidy-check

# CI target: run all checks and build.
.PHONY: ci
ci: check test build

# Show version that would be built.
.PHONY: version
version:
	@go run . --version

.PHONY: help
help:
	@echo "Available targets:"
	@echo "  build          - Build the binary (default)"
	@echo "  install        - Install to GOPATH/bin"
	@echo "  clean          - Remove build artifacts"
	@echo "  test           - Run tests"
	@echo "  fmt            - Format code"
	@echo "  fmt-check      - Format code and check for changes (CI)"
	@echo "  mod-tidy       - Run go mod tidy"
	@echo "  vet            - Run go vet"
	@echo "  mod-tidy-check - Check if go.mod/go.sum need tidying (CI)"
	@echo "  check          - Run all checks (format, vet, mod-tidy, whitespace)"
	@echo "  ci             - Run all checks, tests, and build"
	@echo "  version        - Show version that would be built"
	@echo "  help           - Show this help"
