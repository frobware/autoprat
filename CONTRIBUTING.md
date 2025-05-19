# Contributing to autoprat

## Development Setup

### Pre-commit Hook

To ensure your changes pass the CI checks, install the pre-commit hook:

```bash
./scripts/pre-commit --install
```

This creates a symbolic link to the pre-commit hook in your .git/hooks directory. The advantage is that you'll automatically get any future improvements to the hook without having to reinstall it.

This will run checks locally before each commit:
- Code formatting (gofmt)
- Code analysis (go vet)
- Build verification
- Dependency verification (go mod tidy)

### Manual Checks

You can also run these checks manually:

```bash
# Check formatting
gofmt -l .

# Fix formatting
gofmt -w .

# Verify code
go vet ./...

# Build
go build ./cmd/autoprat

# Check dependencies
go mod tidy
```

## CI Workflows

This project uses GitHub Actions to run checks on every push:

1. **Build**: Ensures the code compiles with multiple Go versions
2. **Lint**: Checks code formatting and runs go vet
3. **Dependencies**: Verifies go.mod and go.sum are up-to-date

These checks run automatically when code is pushed to any branch.