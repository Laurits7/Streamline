# Developer shortcuts. Run `make dev-server` and `make dev-web` in two terminals,
# then open http://localhost:5173 (Vite proxies /api to the Rust server).

.PHONY: dev-server dev-web test check types build docker up measure clean-data

dev-server:
	DATA_DIR=./data LOG_LEVEL=debug,sqlx=warn cargo run -p streamline

dev-web:
	cd web && npm run dev

test:
	cargo test --workspace
	cd web && npm run check && npm test

check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cd web && npm run check

# Regenerate TypeScript API types from the Rust structs (ts-rs).
types:
	cargo test -p streamline --lib export_bindings

build:
	cd web && npm ci && npm run build
	cargo build --release -p streamline

docker:
	docker compose build

up:
	docker compose up -d --build

measure:
	./scripts/measure.sh
