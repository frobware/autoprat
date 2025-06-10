#!/bin/bash
# Check formatting with nightly rustfmt
exec cargo +nightly fmt --all -- --check