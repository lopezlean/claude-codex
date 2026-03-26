#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${repo_root}"

command="${1:-run}"

case "${command}" in
  "test")
    cargo fmt --check
    cargo test
    ;;
  "run")
    shift || true
    cargo run -- "$@"
    ;;
  "auth-status")
    cargo run -- auth status
    ;;
  "proxy")
    shift || true
    cargo run -- proxy serve "$@"
    ;;
  *)
    cat <<'EOF'
Usage:
  ./run.sh test
  ./run.sh run [claude-codex args...]
  ./run.sh auth-status
  ./run.sh proxy
EOF
    exit 1
    ;;
esac
