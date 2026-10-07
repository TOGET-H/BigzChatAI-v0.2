#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != Darwin ]]; then
  echo 'This entry point requires macOS; use the Windows scripts on Windows.' >&2
  exit 1
fi
for tool in git node corepack cargo rustc xcrun; do
  command -v "$tool" >/dev/null || { echo "Install system $tool before continuing." >&2; exit 1; }
done
xcrun --find clang >/dev/null
node -e 'const m=+process.versions.node.split(".")[0]; if(m!==22&&m<24) process.exit(1); if(m===22&&+process.versions.node.split(".")[1]<19) process.exit(1)'
rustc_version=$(rustc --version | awk '{print $2}')
node -e 'const [major,minor]=process.argv[1].split(".").map(Number); if(major<1||(major===1&&minor<95)) process.exit(1)' "$rustc_version"
git_version=$(git --version | awk '{print $3}')
node -e 'const [major,minor]=process.argv[1].split(".").map(Number); if(major<2||(major===2&&minor<51)) process.exit(1)' "$git_version"
case "${1:-}" in
  dev) corepack pnpm@8.14.0 --fail-if-no-match --filter @git-collab/desktop tauri dev ;;
  build) corepack pnpm@8.14.0 --fail-if-no-match --filter @git-collab/desktop tauri build --bundles app -- --locked ;;
  *) echo 'Usage: bash scripts/macos.sh dev|build' >&2; exit 2 ;;
esac
