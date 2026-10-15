# syntax=docker/dockerfile:1
#
# All-in-one image: plus-ui frontend (built from the official repo, unchanged)
# + Rust backend (musl static binary) + embedded SQLite.
#
#   docker build -t ruoyi-rs ./ruoyi-rs
#   docker run -d -p 8080:8080 -v ruoyi-data:/ruoyi/data ruoyi-rs

# ---------- Stage 1: frontend ----------
FROM node:20-alpine AS web
RUN apk add --no-cache git
ARG PLUS_UI_REPO=https://github.com/JavaLionLi/plus-ui.git
ARG PLUS_UI_REF=5.X
RUN git clone --depth 1 --branch "${PLUS_UI_REF}" "${PLUS_UI_REPO}" /ui
WORKDIR /ui
# 5.X does not pin a packageManager, so corepack resolves a recent pnpm whose
# default blocks dependency build scripts (ERR_PNPM_IGNORED_BUILDS).
# strict-dep-builds=false keeps install non-interactive; unknown in older
# pnpm versions it is simply ignored (they only warn).
RUN corepack enable \
    && pnpm install --no-frozen-lockfile --config.strict-dep-builds=false \
    && NODE_OPTIONS=--max-old-space-size=4096 pnpm run build:prod

# ---------- Stage 2: Rust backend (static musl binary) ----------
FROM rust:1.94-alpine AS server
RUN apk add --no-cache musl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY sql ./sql
COPY src ./src
RUN cargo build --release \
    && strip target/release/ruoyi-rs \
    && mkdir -p /build/data

# ---------- Stage 3: runtime ----------
FROM gcr.io/distroless/static-debian12:nonroot
WORKDIR /ruoyi
COPY --from=server --chown=nonroot:nonroot /build/target/release/ruoyi-rs /ruoyi/ruoyi-rs
COPY --from=server --chown=nonroot:nonroot /build/data                    /ruoyi/data
COPY --from=web    --chown=nonroot:nonroot /ui/dist                       /ruoyi/web/dist
ENV PORT=8080 \
    DB_PATH=/ruoyi/data/ruoyi.db \
    WEB_DIR=/ruoyi/web/dist \
    UPLOAD_DIR=/ruoyi/data/uploads \
    CAPTCHA_ENABLED=true
VOLUME ["/ruoyi/data"]
EXPOSE 8080
USER nonroot
ENTRYPOINT ["/ruoyi/ruoyi-rs"]
