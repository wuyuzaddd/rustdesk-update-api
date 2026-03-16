# RustDesk Update API 适配层

这是一个基于 Rust + Axum 构建的 RustDesk 更新 API 适配层，用于将从 GitHub Release 获取到的 latest 版本信息处理为 RustDesk 客户端可读格式，实现 RustDesk 从 fork 仓库获取更新。

## 功能特性

- 将 GitHub Release 最新版本信息转换为 RustDesk 客户端可读格式
- 支持 Docker 容器化部署（集成 Nginx 反向代理）
- 详细的日志输出，便于监控和调试
- 环境变量配置，支持 .env 文件
- 支持 GitHub PAT 认证，提高 API 调用限额

## 快速开始

### 1. 构建 Docker 镜像

```bash
docker build -t rustdesk-update-api .
```

### 2. 运行容器

#### 使用 Docker Compose

```bash
# 复制环境变量模板
cp .env.example .env

# 编辑 .env 文件，设置必要的环境变量

# 启动服务
docker-compose up -d
```

#### 使用 Docker 命令

```bash
docker run -d \
  --name rustdesk-update-api \
  -p 8080:8080 \
  -p 8443:8443 \
  -v ./nginx/ssl:/etc/nginx/ssl:ro \
  -e GITHUB_API_BASE=https://api.github.com/repos \
  -e FORK_OWNER=your_github_username \
  -e FORK_REPO=rustdesk \
  -e DOMAIN=update.yourdomain.com \
  -e SSL_ENABLED=false \
  rustdesk-update-api:latest
```

## 环境变量配置

| 环境变量 | 默认值 | 说明 |
|---------|-------|------|
| `GITHUB_API_BASE` | `https://api.github.com/repos` | GitHub API 基础 URL |
| `FORK_OWNER` | `wuyuzaddd` | GitHub 仓库所有者（你的 GitHub 用户名） |
| `FORK_REPO` | `rustdesk` | GitHub 仓库名称 |
| `GITHUB_PAT` | `` | GitHub 个人访问令牌（可选，提高 API 调用限额） |
| `DOMAIN` | `update.example.com` | 域名 |
| `SSL_ENABLED` | `false` | 是否启用 SSL |
| `HTTP_PORT` | `8080` | HTTP 端口 |
| `HTTPS_PORT` | `8443` | HTTPS 端口 |
| `LISTEN_ADDR` | `0.0.0.0:3000` | 内部 API 服务监听地址 |
| `RUST_LOG` | `info` | 日志级别（debug, info, warn, error） |

## 目录结构

```
rustdesk-update-api/
├── src/                 # Rust 源码
│   └── main.rs          # 主程序
├── nginx/               # Nginx 配置
│   ├── nginx.conf       # Nginx 主配置
│   ├── conf.d/          # Nginx 站点配置
│   │   ├── rustdesk.conf       # 站点配置模板
│   │   └── generate_config.sh  # 配置生成脚本
│   └── ssl/             # SSL 证书目录
├── .env                 # 环境变量文件
├── .env.example         # 环境变量模板
├── Cargo.toml           # Rust 项目配置
├── Dockerfile           # Docker 构建文件
└── docker-compose.yaml  # Docker Compose 配置
```

## 配置 RustDesk 客户端

要使用此 API 作为 RustDesk 的更新源，需要修改 RustDesk 客户端的配置：

1. 打开 `rustdesk/libs/hbb_common/src/lib.rs` 文件
2. 找到第 496 行左右的 URL 配置
3. 将 URL 修改为你的域名，例如：

```rust
// 原配置
const URL: &str = "https://rustdesk.com/api/update/check";

// 修改为你的域名
const URL: &str = "https://update.yourdomain.com/version/latest";
```

## 测试 API

```bash
# 测试最新版本
curl http://localhost:8080/version/latest?os=windows&arch=x86_64

# 测试特定版本
curl http://localhost:8080/version/latest?os=macos&arch=aarch64&version=1.4.0
```

## 容器管理

```bash
# 查看容器状态
docker-compose ps

# 查看容器日志
docker-compose logs -f

# 停止服务
docker-compose down

# 重启服务
docker-compose restart
```

## SSL 配置

1. 生成 SSL 证书：

```bash
mkdir -p nginx/ssl
# 使用 OpenSSL 生成自签名证书
openssl req -x509 -newkey rsa:4096 -keyout nginx/ssl/privkey.pem -out nginx/ssl/cert.pem -days 365 -nodes
```

2. 在 `.env` 文件中设置 `SSL_ENABLED=true`

3. 启动服务

## 非 Docker 环境运行

### 1. 安装 Rust 和 Cargo

```bash
# 安装 Rust 和 Cargo
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 加载环境变量
source $HOME/.cargo/env
```

### 2. 构建和运行服务

```bash
# 克隆仓库
git clone https://github.com/yourusername/rustdesk-update-api.git
cd rustdesk-update-api

# 复制环境变量模板
cp .env.example .env

# 编辑 .env 文件，设置必要的环境变量

# 构建项目
cargo build --release

# 运行服务
cargo run --release
```

### 3. 测试 API

```bash
# 测试最新版本
curl http://localhost:3000/version/latest?os=windows&arch=x86_64
```

## 注意事项

1. **GitHub 仓库 tag 格式**：必须使用纯数字版本格式，如 `1.4.6`，不要包含 `v` 前缀（如 `v1.4.6`），否则 RustDesk 客户端无法正常解析
2. 确保你的 GitHub 仓库有正确的 Release 版本和资产文件
3. 资产文件命名应遵循 RustDesk 的命名规范，例如：
   - Windows: `rustdesk-1.4.6-x86_64.exe`
   - macOS: `rustdesk-1.4.6-aarch64-aarch64.dmg`
   - Linux: `rustdesk-1.4.6-x86_64.deb`
4. 每个资产文件应包含 SHA256 校验和
5. 建议使用 GitHub PAT 以提高 API 调用限额
6. 生产环境中建议启用 SSL
7. **Nginx 配置**：仅在 Docker 环境下集成 Nginx 反向代理，非 Docker 环境下需要自行配置 Nginx 或其他反向代理

## 技术栈

- Rust + Axum
- Nginx
- Docker
- Docker Compose
- GitHub API

## 常见问题

### Q: 客户端无法获取更新

A: 检查以下几点：
1. 确保 API 服务正在运行
2. 检查 GitHub 仓库是否有最新的 Release
3. 确保资产文件命名正确
4. 检查网络连接和防火墙设置
5. 查看 API 日志以获取详细错误信息

### Q: API 返回 404 错误

A: 检查以下几点：
1. 确保 GitHub 仓库存在
2. 确保仓库有 Release 版本
3. 检查环境变量配置是否正确

### Q: 构建失败

A: 检查以下几点：
1. 确保 Rust 环境正确安装
2. 检查网络连接，确保可以访问 crates.io
3. 查看详细的构建错误信息

## 许可证

MIT
