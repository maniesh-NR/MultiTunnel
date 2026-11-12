#!/usr/bin/env bash
# MultiTunnel backup workflow — MUST run at the end of EVERY roadmap stage.
# Always creates a local commit + tag (survives sandbox recycling); pushes to
# GitHub when credentials are available.
#
# Credentials: GH_TOKEN env var; if unset, the script reads the token from
# ../upload/Multitunnel github access token.txt (outside the repo, never
# printed, never committed). Token is used one-shot per push and scrubbed
# from any captured log output.
#
# Usage: harness/backup.sh <stage-label>    e.g. harness/backup.sh s1-foundation
set -euo pipefail
cd "$(dirname "$0")/.."

LABEL="${1:-checkpoint}"
TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
LOG=/tmp/mt-push.log

if [ -z "${GH_TOKEN:-}" ] && [ -f ../upload/Multitunnel\ github\ access\ token.txt ]; then
  GH_TOKEN="$(tr -d ' \t\r\n' < "../upload/Multitunnel github access token.txt")"
  export GH_TOKEN
fi

git add -A
if git diff --cached --quiet; then
  git commit --allow-empty -m "backup(${LABEL}): ${TS} [no changes]" >/dev/null
else
  git commit -m "backup(${LABEL}): ${TS}" >/dev/null
fi
git tag -f "backup/${LABEL}" >/dev/null

PUSH_MSG="kept local (no GH_TOKEN available)"
if [ -n "${GH_TOKEN:-}" ]; then
  if git push "https://x-access-token:${GH_TOKEN}@github.com/maniesh-NR/MultiTunnel.git" main --tags >"$LOG" 2>&1; then
    PUSH_MSG="pushed to github.com/maniesh-NR/MultiTunnel (main + tags)"
  else
    PUSH_MSG="PUSH FAILED: $(sed -E 's#x-access-token:[^@]*@#x-access-token:REDACTED@#g' "$LOG" | tr '\n' ' ' | cut -c1-200)"
  fi
fi
rm -f "$LOG"
unset GH_TOKEN

echo "[backup ${TS}] label=${LABEL} commit=$(git rev-parse --short HEAD) ${PUSH_MSG}"
