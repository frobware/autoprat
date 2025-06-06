# Default target.
.PHONY: all
all: build

# Build the binary.
.PHONY: build
build: fmt
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

# Format check.
.PHONY: fmt-check
fmt-check:
	gofmt -w .
	git diff --exit-code

# Format code.
.PHONY: fmt
fmt:
	gofmt -w .

# Vet code.
.PHONY: vet
vet:
	go vet ./...

# Check for whitespace issues.
.PHONY: whitespace-check
whitespace-check:
	@if git diff --check --cached | grep .; then \
		exit 1; \
	fi
	@if git rev-parse --verify HEAD >/dev/null 2>&1; then \
		if git diff --check HEAD | grep .; then \
			exit 1; \
		fi; \
	fi

# Verify go.mod and go.sum are tidy.
.PHONY: mod-tidy-check
mod-tidy-check:
	@cp go.mod go.mod.backup
	@cp go.sum go.sum.backup
	@go mod tidy
	@if ! diff go.mod go.mod.backup || ! diff go.sum go.sum.backup; then \
		echo "Error: go.mod or go.sum needs updating. Run 'go mod tidy'"; \
		mv go.mod.backup go.mod; \
		mv go.sum.backup go.sum; \
		exit 1; \
	fi
	@rm go.mod.backup go.sum.backup

# Run all checks.
.PHONY: check
check: whitespace-check fmt-check vet mod-tidy-check

# CI target: run all checks and build.
.PHONY: ci
ci: check test build

# Show version that would be built.
.PHONY: version
version:
	@go build -o autoprat-temp . && ./autoprat-temp --version | head -1 | awk '{print $$3}' && rm autoprat-temp

# Development build (same as build, but explicit).
.PHONY: dev
dev: build

.PHONY: help
help:
	@echo "Available targets:"
	@echo "  all            - Build the binary (default)"
	@echo "  build          - Build the binary with version info"
	@echo "  install        - Install to GOPATH/bin"
	@echo "  clean          - Remove build artifacts"
	@echo "  test           - Run tests"
	@echo "  fmt            - Format code"
	@echo "  fmt-check      - Check code formatting"
	@echo "  vet            - Run go vet"
	@echo "  mod-tidy-check - Check if go.mod/go.sum need tidying"
	@echo "  check          - Run all checks (format, vet, mod-tidy, whitespace)"
	@echo "  ci             - Run all checks, tests, and build"
	@echo "  version        - Show version that would be built"
	@echo "  dev            - Development build (alias for build)"
	@echo "  help           - Show this help"
