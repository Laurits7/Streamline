# Streamline: one static binary (API + embedded web app) in an empty image.
# Works with both BuildKit and the classic builder.

# 1. Web app
FROM node:24-alpine AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

# 2. Server: a static musl binary with the web app embedded
FROM rust:1-alpine AS server
RUN apk add --no-cache musl-dev
# Low-memory hosts (e.g. a Raspberry Pi with 1-2 GB) can build with
# --build-arg LTO=thin --build-arg CODEGEN_UNITS=16: slightly larger binary, much less RAM.
ARG LTO=true
ARG CODEGEN_UNITS=1
ENV CARGO_PROFILE_RELEASE_LTO=$LTO CARGO_PROFILE_RELEASE_CODEGEN_UNITS=$CODEGEN_UNITS
WORKDIR /src
# Build dependencies first against stub sources, so code changes don't recompile them.
COPY Cargo.toml Cargo.lock ./
COPY crates/domain/Cargo.toml crates/domain/
COPY crates/server/Cargo.toml crates/server/
RUN mkdir -p crates/domain/src crates/server/src \
 && touch crates/domain/src/lib.rs crates/server/src/lib.rs \
 && echo 'fn main() {}' > crates/server/src/main.rs \
 && cargo build --release --locked -p streamline \
 && rm -rf crates/*/src target/release/.fingerprint/streamline*
COPY crates crates
COPY migrations migrations
COPY --from=web /web/dist web/dist
RUN cargo build --release --locked -p streamline \
 && cp target/release/streamline /streamline \
 && mkdir /data

# 3. Runtime: nothing but the binary and CA certificates (for CalDAV/integrations later)
FROM scratch
COPY --from=server /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=server /streamline /streamline
COPY --from=server /data /data
ENV DATA_DIR=/data PORT=3000
EXPOSE 3000
VOLUME /data
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s CMD ["/streamline", "healthcheck"]
ENTRYPOINT ["/streamline"]
