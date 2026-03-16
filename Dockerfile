# 使用基于 Alpine 的 Rust 镜像作为构建环境
FROM rust:1.94.0-alpine3.23 AS builder

# 切换 Alpine 镜像源为阿里云
RUN echo 'https://mirrors.aliyun.com/alpine/v3.23/main/' > /etc/apk/repositories && \
    echo 'https://mirrors.aliyun.com/alpine/v3.23/community/' >> /etc/apk/repositories

# 安装必要的构建依赖
RUN apk add --no-cache zstd zlib openssl openssl-dev pkgconfig openssl-libs-static

# 设置工作目录
WORKDIR /app

# 配置 Cargo 使用清华大学镜像源和稀疏索引
RUN mkdir -vp ${CARGO_HOME:-$HOME/.cargo} && \
    cat << EOF | tee -a ${CARGO_HOME:-$HOME/.cargo}/config.toml
[source.crates-io]
replace-with = 'mirror'

[source.mirror]
registry = "sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/"

[registries.mirror]
index = "sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/"
EOF

# 复制项目文件
COPY . .

# 构建应用程序（动态链接）
RUN cargo build --release && ls -la /app/target/release/

# 使用轻量级的 Alpine 镜像作为运行环境
FROM alpine:3.23 AS runner

# 切换 Alpine 镜像源为阿里云
RUN echo 'https://mirrors.aliyun.com/alpine/v3.23/main/' > /etc/apk/repositories && \
    echo 'https://mirrors.aliyun.com/alpine/v3.23/community/' >> /etc/apk/repositories

# 安装必要的依赖
RUN apk add --no-cache ca-certificates nginx bash zstd openssl zlib curl

# 创建 Nginx 配置目录
RUN mkdir -p /etc/nginx/conf.d /etc/nginx/ssl /app/nginx/conf.d

# 从构建环境复制可执行文件
COPY --from=builder /app/target/release/rustdesk-update-adapter /usr/local/bin/

# 复制 Nginx 配置文件和生成脚本
COPY nginx/nginx.conf /etc/nginx/nginx.conf
COPY nginx/conf.d/rustdesk.conf /app/nginx/conf.d/rustdesk.conf
COPY nginx/conf.d/generate_config.sh /app/nginx/conf.d/generate_config.sh

# 检查复制是否成功
RUN ls -la /usr/local/bin/ && chmod +x /usr/local/bin/rustdesk-update-adapter && chmod +x /app/nginx/conf.d/generate_config.sh

# 设置环境变量
ENV DOMAIN=update.example.com
ENV SSL_ENABLED=false
ENV HTTP_PORT=8080
ENV HTTPS_PORT=8443
# 日志配置
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# 暴露 Nginx 端口
EXPOSE 8080 8443

# 创建启动脚本
RUN cat > /start.sh << 'EOF'
#!/bin/bash

# 生成 Nginx 配置
/app/nginx/conf.d/generate_config.sh

# 启动 API 服务
/usr/local/bin/rustdesk-update-adapter &

# 启动 Nginx
nginx -g "daemon off;"
EOF

RUN chmod +x /start.sh

# 运行启动脚本
CMD ["/start.sh"]