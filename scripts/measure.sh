#!/usr/bin/env bash
# Phase-gate resource report: image size, idle memory, web payload (see WORKPLAN Part D).
set -euo pipefail
cd "$(dirname "$0")/.."

image=${IMAGE:-streamline:latest}
echo "== Docker image"
docker image inspect "$image" --format '{{.Size}}' | awk '{printf "  size: %.1f MB\n", $1/1000/1000}'

echo "== Idle memory (fresh container, 20 s after start)"
name=streamline-measure-$$
tmp=$(mktemp -d)
docker run -d --rm --name "$name" -v "$tmp:/data" -p 127.0.0.1::3000 "$image" >/dev/null
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
sleep 20
docker stats --no-stream --format '  mem: {{.MemUsage}}  cpu: {{.CPUPerc}}' "$name"

echo "== Web payload (gzip)"
if [ -d web/dist ]; then
  total=0
  for f in web/dist/index.html web/dist/assets/*.js web/dist/assets/*.css; do
    s=$(gzip -9c "$f" | wc -c)
    total=$((total + s))
    printf '  %-40s %6.1f KB\n' "${f#web/dist/}" "$(echo "$s/1024" | bc -l)"
  done
  printf '  %-40s %6.1f KB\n' "total" "$(echo "$total/1024" | bc -l)"
else
  echo "  web/dist missing: run 'cd web && npm run build'"
fi
