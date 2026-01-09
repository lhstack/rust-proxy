# Proxy Server

高性能 HTTP 反向代理服务器，基于 Rust + Axum 构建，支持动态规则配置和 Web 管理界面。

## ✨ 特性

- 🚀 高性能异步架构，基于 Tokio + Axum
- 🔄 支持规则代理和直接代理两种模式
- 📝 动态路径匹配，支持 `{param}` 和 `{*path}` 通配符
- 🎛️ Web 管理界面，实时配置代理规则
- 🔐 内置认证系统，支持 Session 管理
- 📊 SQLite 数据持久化，WAL 模式高并发
- 📁 日志自动滚动切割和过期清理
- 🐳 Docker 一键部署

## 🚀 快速开始

### Docker 部署（推荐）

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

### Docker Compose

```bash
docker-compose up -d
```

### 本地编译

```bash
cargo build --release
./target/release/proxy-server
```

## 📖 使用说明

### 访问管理界面

启动后访问 `http://localhost:8080`，默认账号：`admin` / `admin123`

### 直接代理

通过配置的路径前缀直接代理任意 URL：

```
http://localhost:3000/proxy/https://api.example.com/path
```

### 规则代理

在管理界面配置规则，支持路径参数：

| 源路径 | 目标地址 | 说明 |
|--------|----------|------|
| `/api/{*path}` | `https://api.example.com/{*path}` | 多段路径匹配 |
| `/user/{id}` | `https://backend.com/users/{id}` | 单段参数匹配 |

## ⚙️ 配置

### 配置文件 (config.yaml)

```yaml
admin:
  host: "0.0.0.0"
  port: 8080

proxy:
  host: "0.0.0.0"
  port: 3000

auth:
  username: "admin"
  password: "admin123"

database:
  path: "./proxy.db"

logging:
  directory: "./logs"
  max_size_bytes: 1073741824  # 1GB
  retention_days: 30

default_timeout_secs: 30
```

### 环境变量

所有配置项均可通过环境变量覆盖：

| 环境变量 | 说明 | 默认值 |
|----------|------|--------|
| `PROXY_ADMIN_PORT` | 管理界面端口 | 8080 |
| `PROXY_PROXY_PORT` | 代理服务端口 | 3000 |
| `PROXY_USERNAME` | 管理员用户名 | admin |
| `PROXY_PASSWORD` | 管理员密码 | admin123 |
| `PROXY_DB_PATH` | 数据库路径 | ./proxy.db |
| `PROXY_LOG_DIR` | 日志目录 | ./logs |
| `PROXY_DEFAULT_TIMEOUT` | 默认超时(秒) | 30 |

## 🔌 API

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/login` | POST | 登录 |
| `/api/logout` | POST | 登出 |
| `/api/rules` | GET/POST | 获取/创建规则 |
| `/api/rules/:id` | PUT/DELETE | 更新/删除规则 |
| `/api/rules/:id/toggle` | POST | 启用/禁用规则 |
| `/api/configs` | GET | 获取配置 |
| `/api/configs/:key` | PUT | 更新配置 |
| `/api/status` | GET | 获取代理状态 |
| `/health` | GET | 健康检查 |

## 📁 项目结构

```
├── src/
│   ├── main.rs          # 入口，路由配置
│   ├── config.rs        # 配置加载
│   ├── proxy.rs         # 代理核心逻辑
│   ├── api.rs           # REST API
│   ├── auth.rs          # 认证模块
│   ├── db.rs            # 数据库操作
│   ├── logger.rs        # 日志滚动
│   └── static_files.rs  # 静态资源
├── static/              # Web 界面
├── config.yaml          # 配置文件
├── Dockerfile
└── docker-compose.yml
```

## 📄 License

MIT
