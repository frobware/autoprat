# Get version from git, fallback to 'dev' if no tags exist
VERSION ?= $(shell git describe --tags --always 2>/dev/null || echo "dev")

# Build flags
LDFLAGS = -X main.version=$(VERSION)

# Default target
.PHONY: all
all: build

# Build the binary
.PHONY: build
build:
	go build -ldflags "$(LDFLAGS)" -o autoprat .

# Install to GOPATH/bin
.PHONY: install
install:
	go install -ldflags "$(LDFLAGS)" .

# Clean build artifacts
.PHONY: clean
clean:
	rm -f autoprat

# Run tests
.PHONY: test
test:
	go test -v ./...

# Run pre-commit checks
.PHONY: check
check:
	./scripts/pre-commit

# Show version that would be built
.PHONY: version
version:
	@echo $(VERSION)

# Development build (same as build, but explicit)
.PHONY: dev
dev: build

.PHONY: help
help:
	@echo "Available targets:"
	@echo "  all     - Build the binary (default)"
	@echo "  build   - Build the binary with version info"
	@echo "  install - Install to GOPATH/bin"
	@echo "  clean   - Remove build artifacts"
	@echo "  test    - Run tests"
	@echo "  check   - Run pre-commit checks"
	@echo "  version - Show version that would be built"
	@echo "  dev     - Development build (alias for build)"
	@echo "  help    - Show this help"