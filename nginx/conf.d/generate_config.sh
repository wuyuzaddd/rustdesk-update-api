#!/bin/bash

# 读取环境变量
DOMAIN=${DOMAIN:-update.example.com}
SSL_ENABLED=${SSL_ENABLED:-false}

# 生成 SSL 服务器块
if [ "$SSL_ENABLED" = "true" ]; then
    SSL_SERVER_BLOCK='server {
    listen 8443 ssl http2;
    server_name ${DOMAIN};

    ssl_certificate /etc/nginx/ssl/cert.pem;
    ssl_certificate_key /etc/nginx/ssl/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_prefer_server_ciphers off;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:DHE-RSA-AES128-GCM-SHA256:DHE-RSA-AES256-GCM-SHA384;
    ssl_session_cache shared:SSL:10m;
    ssl_session_timeout 10m;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}'
else
    SSL_SERVER_BLOCK=''
fi

# 生成配置文件
if [ "$SSL_ENABLED" = "true" ]; then
    # 启用 SSL 时的配置
    sed -e "s|\${DOMAIN}|$DOMAIN|g" \
        -e "s|\$ssl_enabled|true|g" \
        -e "s|\$ssl_enabled_server_block|$SSL_SERVER_BLOCK|g" \
        /app/nginx/conf.d/rustdesk.conf > /etc/nginx/conf.d/rustdesk.conf
else
    # 禁用 SSL 时的配置（移除重定向规则）
    cat > /etc/nginx/conf.d/rustdesk.conf << 'EOF'
server {
    listen 8080;
    server_name DOMAIN_PLACEHOLDER;

    # 直接处理 HTTP 请求
    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
EOF

# 替换域名占位符
sed -i "s|DOMAIN_PLACEHOLDER|$DOMAIN|g" /etc/nginx/conf.d/rustdesk.conf
fi

# 测试配置
nginx -t