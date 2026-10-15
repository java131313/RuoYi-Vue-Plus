#
# 纯后端镜像（无前端）：API 服务 + 内嵌 SQLite。
# 前端（plus-ui）由 GitHub Pages 独立部署（.github/workflows/pages.yml），
# 通过 CORS_ORIGIN 允许跨域调用本服务；如需自建前端，挂载 dist 到 WEB_DIR 即可。
#
#   docker build -t ruoyi-rs ./ruoyi-rs
#   docker run -d -p 8080:8080 -v ruoyi-data:/ruoyi/data \
#     -e CORS_ORIGIN=https://<owner>.github.io ruoyi-rs

# ---------- Stage 1: Rust backend (static musl binary) ----------
FROM rust:1.94-alpine AS server
RUN apk add --no-cache musl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY sql ./sql
COPY src ./src
RUN cargo build --release \
    && strip target/release/ruoyi-rs \
    && mkdir -p /build/data

# ---------- Stage 2: runtime ----------
FROM gcr.io/distroless/static-debian12:nonroot
WORKDIR /ruoyi
COPY --from=server --chown=nonroot:nonroot /build/target/release/ruoyi-rs /ruoyi/ruoyi-rs
COPY --from=server --chown=nonroot:nonroot /build/data                    /ruoyi/data
ENV PORT=8080 \
    DB_PATH=/ruoyi/data/ruoyi.db \
    WEB_DIR=/ruoyi/web/dist \
    UPLOAD_DIR=/ruoyi/data/uploads \
    CAPTCHA_ENABLED=true \
    CORS_ORIGIN=
# 前端可选挂载：-v $PWD/dist:/ruoyi/web/dist:ro（同时把 CORS_ORIGIN 设为前端来源）
VOLUME ["/ruoyi/data"]
EXPOSE 8080
USER nonroot
ENTRYPOINT ["/ruoyi/ruoyi-rs"]
