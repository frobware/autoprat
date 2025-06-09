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

# Run tests with coverage summary per package.
.PHONY: test-coverage
test-coverage:
	@echo "Running tests with coverage..."
	@go test -cover ./...

# Run tests with detailed coverage report (file-level).
.PHONY: test-coverage-detailed
test-coverage-detailed:
	@echo "Generating detailed coverage report..."
	@go test -coverprofile=coverage.out ./...
	@go tool cover -func=coverage.out
	@rm -f coverage.out

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

# Check for available updates to direct dependencies.
.PHONY: check-updates
check-updates:
	@echo "Checking for updates to direct dependencies..."
	@go list -m -u -f '{{if and (not .Indirect) .Update}}{{.Path}} {{.Version}} -> {{.Update.Version}}{{end}}' all | grep . || echo "All direct dependencies are up to date."

# List available versions for direct dependencies.
.PHONY: list-versions
list-versions:
	@echo "Available versions for direct dependencies:"
	@go list -m -f '{{if not .Indirect}}{{.Path}}{{end}}' all | tail -n +2 | grep -v '^$$' | while read -r mod; do \
		echo ""; \
		echo "$$mod:"; \
		go list -m -versions "$$mod" | tr ' ' '\n' | tail -10 | sed 's/^/  /'; \
	done

# Update only direct dependencies (not indirect).
.PHONY: update-deps
update-deps:
	go list -m -f '{{if not .Indirect}}{{.Path}}{{end}}' all | xargs -n1 go get -u
	go mod tidy

# Update all dependencies including indirect.
.PHONY: update-all-deps
update-all-deps:
	go get -u ./...
	go mod tidy

# Run all checks (development-friendly).
.PHONY: check
check: whitespace-check fmt vet mod-tidy

# CI target: run all checks and build.
.PHONY: ci
ci: whitespace-check fmt-check vet mod-tidy-check test build

# Show version that would be built.
.PHONY: version
version:
	@go run . --version

.PHONY: help
help:
	@echo "Available targets:"
	@echo "  build           - Build the binary"
	@echo "  check           - Run all checks (development-friendly)"
	@echo "  check-updates   - Check for available updates to direct dependencies"
	@echo "  ci              - Run all CI checks, tests, and build"
	@echo "  clean           - Remove build artifacts"
	@echo "  fmt             - Format code"
	@echo "  fmt-check       - Format code and check for changes (CI)"
	@echo "  help            - Show this help"
	@echo "  install         - Install to GOPATH/bin"
	@echo "  list-versions   - List available versions for direct dependencies"
	@echo "  mod-tidy        - Run go mod tidy"
	@echo "  mod-tidy-check  - Check if go.mod/go.sum need tidying (CI)"
	@echo "  test            - Run tests"
	@echo "  test-coverage   - Run tests with coverage summary per package"
	@echo "  test-coverage-detailed - Run tests with detailed coverage report"
	@echo "  update-all-deps - Update all dependencies (direct and indirect)"
	@echo "  update-deps     - Update direct dependencies to latest versions"
	@echo "  version         - Show version that would be built"
	@echo "  vet             - Run go vet"
	@echo ""
	@echo "Default target: build"
