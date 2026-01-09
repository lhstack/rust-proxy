# Proxy Server

High-performance HTTP reverse proxy server built with Rust + Axum, featuring dynamic rule configuration and web admin interface.

基于 Rust + Axum 构建的高性能 HTTP 反向代理服务器，支持动态规则配置和 Web 管理界面。

## Features / 特性

- 🚀 High-performance async architecture (Tokio + Axum) / 高性能异步架构
- 🔄 Rule-based proxy and direct proxy modes / 规则代理和直接代理两种模式
- 📝 Dynamic path matching with `{param}` and `{*path}` wildcards / 动态路径匹配
- 🎛️ Web admin interface / Web 管理界面
- 🔐 Built-in authentication / 内置认证系统
- � SQLite pfersistence with WAL mode / SQLite 数据持久化
- � Auto-raotating logs / 日志自动滚动切割

## Quick Start / 快速开始

```bash
docker run -d \
  --name proxy-server \
  -p 8080:8080 \
  -p 3000:3000 \
  -v ./data:/app/data \
  -v ./logs:/app/logs \
  -e PROXY_USERNAME=admin \
  -e PROXY_PASSWORD=your_password \
  lhstack/proxy-server:latest
```

## Ports / 端口

| Port | Description |
|------|-------------|
| 8080 | Admin Web UI / 管理界面 |
| 3000 | Proxy Service / 代理服务 |

## Environment Variables / 环境变量

| Variable | Description | Default |
|----------|-------------|---------|
| `PROXY_USERNAME` | Admin username / 管理员用户名 | admin |
| `PROXY_PASSWORD` | Admin password / 管理员密码 | admin123 |
| `PROXY_ADMIN_PORT` | Admin UI port / 管理界面端口 | 8080 |
| `PROXY_PROXY_PORT` | Proxy port / 代理服务端口 | 3000 |
| `PROXY_DB_PATH` | Database path / 数据库路径 | /app/data/proxy.db |
| `PROXY_LOG_DIR` | Log directory / 日志目录 | /app/logs |
| `PROXY_DEFAULT_TIMEOUT` | Request timeout (sec) / 请求超时(秒) | 30 |

## Volumes / 数据卷

| Path | Description |
|------|-------------|
| `/app/data` | Database storage / 数据库存储 |
| `/app/logs` | Log files / 日志文件 |
| `/app/config.yaml` | Configuration file (optional) / 配置文件（可选） |

## Usage / 使用方式

### Direct Proxy / 直接代理

```
http://localhost:3000/proxy/https://api.example.com/path
```

### Rule-based Proxy / 规则代理

Configure rules in the admin UI / 在管理界面配置规则：

- `/api/{*path}` → `https://backend.com/{*path}`
- `/user/{id}` → `https://users.api.com/{id}`

## Docker Compose

```yaml
version: '3.8'
services:
  proxy-server:
    image: lhstack/proxy-server:latest
    ports:
      - "8080:8080"
      - "3000:3000"
    volumes:
      - ./data:/app/data
      - ./logs:/app/logs
    environment:
      - PROXY_USERNAME=admin
      - PROXY_PASSWORD=your_secure_password
```
