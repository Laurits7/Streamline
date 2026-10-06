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

echo "== Web payload (gzip -9)"
if [ -d web/dist ]; then
  initial=0
  for f in web/dist/index.html web/dist/assets/*; do
    s=$(gzip -9c "$f" | wc -c)
    case "$(basename "$f")" in
      index.html | index-*.js | style-*.css) kind=initial; initial=$((initial + s)) ;;
      *) kind=lazy ;;
    esac
    LC_ALL=C awk -v f="${f#web/dist/}" -v s="$s" -v k="$kind" 'BEGIN { printf "  %-40s %6.1f KB  %s\n", f, s / 1024, k }'
  done
  LC_ALL=C awk -v s="$initial" 'BEGIN { printf "  %-40s %6.1f KB  (budget 150 KB)\n", "initial load total", s / 1024 }'
else
  echo "  web/dist missing: run 'cd web && npm run build'"
fi
