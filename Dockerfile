# 构建阶段
FROM rust:1.87-alpine AS builder

# 安装构建依赖
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /app

# 复制依赖文件先，利用缓存
COPY Cargo.toml Cargo.lock ./

# 创建虚拟 src 用于缓存依赖
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src target/release/deps/proxy_server*

# 复制源码
COPY src ./src
COPY static ./static

# 构建
RUN cargo build --release --locked

# 运行阶段
FROM alpine:3.19

# 安装运行时依赖、时区和 UTF-8 支持
RUN apk add --no-cache \
    ca-certificates \
    tzdata \
    && cp /usr/share/zoneinfo/Asia/Shanghai /etc/localtime \
    && echo "Asia/Shanghai" > /etc/timezone

# 设置 UTF-8 环境
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    TZ=Asia/Shanghai

# 创建非 root 用户
RUN addgroup -S proxy && adduser -S proxy -G proxy

WORKDIR /app

# 从构建阶段复制二进制
COPY --from=builder /app/target/release/proxy-server /app/proxy-server

# 复制默认配置
COPY config.yaml /app/config.yaml

# 创建数据目录
RUN mkdir -p /app/data /app/logs && chown -R proxy:proxy /app

USER proxy

# 暴露端口
EXPOSE 8080 3000

# 环境变量
ENV PROXY_DB_PATH=/app/data/proxy.db \
    PROXY_LOG_DIR=/app/logs

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3000/health || exit 1

CMD ["/app/proxy-server"]
