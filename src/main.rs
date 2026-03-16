use axum::{
    extract::{Query, State, FromRequest},
    // http::HeaderValue as AxumHeaderValue, // 重命名避免冲突
    Json, Router, routing::get,
};
use reqwest::{Client, header::HeaderValue as ReqwestHeaderValue}; // 区分reqwest的HeaderValue
use serde::{Deserialize, Serialize};
use semver::Version; // 删除未使用的VersionReq
use std::sync::Arc;
use once_cell::sync::OnceCell;
use log::{debug, trace, error, info}; // 引入log宏
use dotenv;

// ===================== 配置项（从环境变量读取）=====================
fn get_env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

// 配置项默认值（仅作为 fallback 使用）
const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com/repos";
const DEFAULT_FORK_OWNER: &str = "wuyuzaddd";
const DEFAULT_FORK_REPO: &str = "rustdesk";
const DEFAULT_GITHUB_PAT: &str = "";
const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:3000";
// =====================================================================

#[derive(Debug, Clone)] // 添加Debug trait，修复OnceCell unwrap错误
struct AppState {
    http_client: Client,
}

// RustDesk客户端请求参数（支持两种格式）
#[derive(Debug, Deserialize)]
struct UpdateQuery {
    // 格式1：RustDesk原生格式
    os: Option<String>, // 操作系统
    os_version: Option<String>, // 操作系统版本
    arch: Option<String>, // 架构
    device_id: Option<Vec<u8>>, // 设备ID
    typ: Option<String>, // 类型
    
    // 格式2：当前API格式
    platform: Option<String>, // windows/macos/linux/android
    version: Option<String>, // 客户端当前版本（可选）
}

// 匹配RustDesk原生期望的完整响应格式
#[derive(Serialize)]
struct UpdateResponse {
    url: String,                // 下载链接（原有）
    version: String,            // 最新版本号（新增，对应tag_name）
    digest: String,             // SHA256校验和（去掉sha256:前缀）
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,      // 错误信息（兼容原生错误格式）
}

// GitHub Releases API 结构体（匹配返回格式）
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,                // 版本标签（如v1.4.5）
    assets: Vec<GithubAsset>,        // 发布资产列表
}

#[derive(Deserialize)]
struct GithubAsset {
    browser_download_url: String,    // 浏览器下载链接
    name: String,                    // 资产名（如rustdesk-1.4.5-x86_64.exe）
    digest: String,                  // SHA256校验和（如sha256:74379f36f014656c4056696a63465360bd20379f936190d858b9c009789e12b1）
}

// 全局状态（复用HTTP客户端）
static APP_STATE: OnceCell<Arc<AppState>> = OnceCell::new();

/// 版本对比：判断是否需要更新（处理v前缀，语义化版本对比）
fn should_update(client_version: &Option<String>, latest_tag: &str) -> bool {
    // 客户端未传版本 → 强制更新
    let Some(client_ver_str) = client_version else {
        return true;
    };

    // 处理tag的v前缀（如v1.4.5 → 1.4.5）
    let latest_ver_str = latest_tag.trim_start_matches('v');
    let Ok(latest_ver) = Version::parse(latest_ver_str) else {
        return false;
    };

    // 解析客户端版本（兼容带/不带v前缀）
    let client_ver_str = client_ver_str.trim_start_matches('v');
    let Ok(client_ver) = Version::parse(client_ver_str) else {
        return false;
    };

    // 最新版本 > 客户端版本 → 需要更新
    latest_ver > client_ver
}

/// 核心：匹配平台+架构对应的下载包（精准对齐GitHub Assets命名）
// 添加生命周期标注，修复E0106错误
fn match_asset<'a>(
    assets: &'a [GithubAsset],
    platform: &str,
    arch: &str,
) -> Result<&'a GithubAsset, String> {
    // 标准化平台名称，处理RustDesk客户端发送的不同格式
    let platform_lower = platform.to_lowercase();
    let normalized_platform = match platform_lower.as_str() {
        "windows" => "windows",
        "linux" => "linux",
        "mac os" | "macos" => "macos",
        "android" => "android",
        p => p,
    };
    
    let target_suffix = match (normalized_platform, arch.to_lowercase().as_str()) {
        // Windows：覆盖exe/msi/x86-sciter
        ("windows", "x86_64") => "x86_64.exe",
        ("windows", "x86_64_msi") => "x86_64.msi",
        ("windows", "x86") => "x86-sciter.exe",

        // macOS：匹配dmg命名
        ("macos", "aarch64") => "aarch64-aarch64.dmg",
        ("macos", "x86_64") => "x86_64-x86_64.dmg",

        // Linux：全覆盖deb/rpm/appimage/flatpak/sciter
        ("linux", "x86_64") => "x86_64.deb",
        ("linux", "x86_64_suse_rpm") => "x86_64-suse.rpm",
        ("linux", "x86_64_rpm") => "x86_64.rpm",
        ("linux", "x86_64_appimage") => "x86_64.AppImage",
        ("linux", "x86_64_flatpak") => "x86_64.flatpak",
        ("linux", "x86_64_sciter_deb") => "x86_64-sciter.deb",
        ("linux", "x86_64_sciter_flatpak") => "x86_64-sciter.flatpak",
        ("linux", "aarch64") => "aarch64.deb",
        ("linux", "aarch64_suse_rpm") => "aarch64-suse.rpm",
        ("linux", "aarch64_rpm") => "aarch64.rpm",
        ("linux", "aarch64_appimage") => "aarch64.AppImage",
        ("linux", "aarch64_flatpak") => "aarch64.flatpak",
        ("linux", "armv7_sciter_deb") => "armv7-sciter.deb",
        ("linux", "x86_64_pkg_tar_zst") => "x86_64.pkg.tar.zst",

        // Android：覆盖所有apk类型
        ("android", "aarch64") => "aarch64.apk",
        ("android", "armv7") => "armv7.apk",
        ("android", "universal") => "universal.apk",
        ("android", "x86_64") => "x86_64.apk",

        (p, a) => return Err(format!("不支持的平台/架构: {} {}", p, a)),
    };

    debug!("匹配资产：platform={}, normalized_platform={}, arch={}, 期望后缀={}", platform, normalized_platform, arch, target_suffix);
    // 遍历资产列表，精准匹配后缀
    for asset in assets {
        trace!("检查资产：{}", asset.name);
        if asset.name.ends_with(target_suffix) {
            return Ok(asset);
        }
    }

    Err(format!(
        "未找到匹配的资产: platform={}, arch={}, 期望后缀={}",
        platform, arch, target_suffix
    ))
}

/// 处理更新请求
// 同时支持GET和POST请求
async fn handle_update(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<Json<UpdateResponse>, Json<UpdateResponse>> {
    // 记录请求开始
    info!("处理更新请求，方法: {:?}", req.method());
    
    // 解析请求参数（支持GET查询参数和POST请求体/查询参数）
    let query = match req.method() {
        &axum::http::Method::GET => {
            let (parts, body) = req.into_parts();
            let req = axum::http::Request::from_parts(parts, body);
            match Query::<UpdateQuery>::from_request(req, &()).await {
                Ok(Query(query)) => {
                    debug!("GET请求参数: {:?}", query);
                    query
                },
                Err(e) => {
                    error!("GET请求参数解析失败: {}", e);
                    return Err(Json(UpdateResponse {
                        url: "".into(),
                        version: "".into(),
                        digest: "".into(),
                        error: Some(format!("缺少必要的请求参数: {}", e)),
                    }));
                }
            }
        }
        &axum::http::Method::POST => {
            let (parts, body) = req.into_parts();
            // 尝试从请求体解析JSON
            let req_json = axum::http::Request::from_parts(parts.clone(), body);
            match Json::<UpdateQuery>::from_request(req_json, &()).await {
                Ok(Json(query)) => {
                    debug!("POST请求体参数: {:?}", query);
                    query
                }
                Err(_) => {
                    // 如果JSON解析失败，尝试从查询参数解析
                    let req_query = axum::http::Request::from_parts(parts, axum::body::Body::empty());
                    match Query::<UpdateQuery>::from_request(req_query, &()).await {
                        Ok(Query(query)) => {
                            debug!("POST查询参数: {:?}", query);
                            query
                        }
                        Err(e) => {
                            error!("POST请求参数解析失败: {}", e);
                            return Err(Json(UpdateResponse {
                                url: "".into(),
                                version: "".into(),
                                digest: "".into(),
                                error: Some(format!("缺少必要的请求参数: {}", e)),
                            }));
                        }
                    }
                }
            }
        }
        _ => {
            error!("不支持的请求方法: {:?}", req.method());
            return Err(Json(UpdateResponse {
                url: "".into(),
                version: "".into(),
                digest: "".into(),
                error: Some("不支持的请求方法".into()),
            }));
        }
    };
    
    // 处理参数映射：优先使用RustDesk原生格式，否则使用当前API格式
    let platform = if let Some(os) = query.os {
        os
    } else if let Some(platform) = query.platform {
        platform
    } else {
        error!("缺少平台信息");
        return Err(Json(UpdateResponse {
            url: "".into(),
            version: "".into(),
            digest: "".into(),
            error: Some("缺少平台信息".into()),
        }));
    };
    
    let arch = if let Some(arch) = query.arch {
        arch
    } else {
        error!("缺少架构信息");
        return Err(Json(UpdateResponse {
            url: "".into(),
            version: "".into(),
            digest: "".into(),
            error: Some("缺少架构信息".into()),
        }));
    };
    
    info!("请求参数: platform={}, arch={}, version={:?}", platform, arch, query.version);
    
    // 1. 从环境变量读取配置
    let github_api_base = get_env("GITHUB_API_BASE", DEFAULT_GITHUB_API_BASE);
    let fork_owner = get_env("FORK_OWNER", DEFAULT_FORK_OWNER);
    let fork_repo = get_env("FORK_REPO", DEFAULT_FORK_REPO);
    let github_pat = get_env("GITHUB_PAT", DEFAULT_GITHUB_PAT);

    debug!("配置信息: github_api_base={}, fork_owner={}, fork_repo={}, pat_set={}", 
           github_api_base, fork_owner, fork_repo, !github_pat.is_empty());

    // 2. 构建GitHub请求URL
    let github_url = format!(
        "{}/{}/{}/releases/latest",
        github_api_base, fork_owner, fork_repo
    );

    debug!("GitHub API URL: {}", github_url);

    // 3. 构建GitHub API请求（处理PAT认证）
    let mut req_builder = state.http_client.get(&github_url)
        .header("User-Agent", "rustdesk-update-adapter");
    
    if !github_pat.is_empty() {
        // 修复HeaderValue类型不兼容问题：先转字符串再构建reqwest的HeaderValue
        let auth_str = format!("token {}", github_pat);
        let auth_header = ReqwestHeaderValue::from_str(&auth_str)
            .map_err(|e| {
                error!("构建认证头失败: {}", e);
                Json(UpdateResponse {
                    url: "".into(),
                    version: "".into(),
                    digest: "".into(),
                    error: Some(format!("构建认证头失败: {}", e)),
                })
            })?;
        req_builder = req_builder.header("Authorization", auth_header);
        debug!("已添加GitHub PAT认证头");
    }

    // 3. 发送请求并处理响应
    info!("发送GitHub API请求");
    let resp = req_builder.send().await.map_err(|e| {
        error!("GitHub API请求失败: {}", e);
        Json(UpdateResponse {
            url: "".into(),
            version: "".into(),
            digest: "".into(),
            error: Some(format!("GitHub API请求失败: {}", e)),
        })
    })?;

    debug!("GitHub API响应状态: {}", resp.status());
    
    if !resp.status().is_success() {
        error!("GitHub API返回错误: {}", resp.status());
        return Err(Json(UpdateResponse {
            url: "".into(),
            version: "".into(),
            digest: "".into(),
            error: Some(format!("GitHub API返回错误: {}", resp.status())),
        }));
    }

    // 4. 解析GitHub Release数据
    info!("解析GitHub Release数据");
    let release: GithubRelease = resp.json().await.map_err(|e| {
        error!("解析GitHub响应失败: {}", e);
        Json(UpdateResponse {
            url: "".into(),
            version: "".into(),
            digest: "".into(),
            error: Some(format!("解析GitHub响应失败: {}", e)),
        })
    })?;

    info!("获取到最新版本: {}", release.tag_name);
    debug!("资产数量: {}", release.assets.len());

    // 5. 版本对比：无需更新则返回
    if !should_update(&query.version, &release.tag_name) {
        info!("当前已是最新版本: {}", release.tag_name);
        return Ok(Json(UpdateResponse {
            url: "".into(),
            version: release.tag_name,
            digest: "".into(),
            error: Some("当前已是最新版本".into()),
        }));
    }

    // 6. 匹配对应平台/架构的资产
    info!("匹配对应平台/架构的资产: platform={}, arch={}", platform, arch);
    let asset = match_asset(&release.assets, &platform, &arch).map_err(|e| {
        error!("匹配资产失败: {}", e);
        Json(UpdateResponse {
            url: "".into(),
            version: release.tag_name.clone(),
            digest: "".into(),
            error: Some(e),
        })
    })?;

    info!("找到匹配的资产: {}", asset.name);
    debug!("资产下载链接: {}", asset.browser_download_url);

    // 7. 处理digest（去掉sha256:前缀，适配RustDesk客户端）
    let digest = asset.digest.trim_start_matches("sha256:").to_string();
    if digest.len() != 64 {
        error!("无效的 SHA256 校验和：{}", asset.digest);
        return Err(Json(UpdateResponse {
            url: "".into(),
            version: release.tag_name.clone(),
            digest: "".into(),
            error: Some(format!("无效的 SHA256 校验和：{}", asset.digest)),
        }));
    }
    
    debug!("处理后的校验和: {}", digest);
    
    // 8. 返回标准响应（返回 releases tag 页面 URL，以便客户端提取版本号）
    let releases_tag_url = format!(
        "https://github.com/{}/{}/releases/tag/{}",
        fork_owner,
        fork_repo,
        release.tag_name
    );
    
    info!("返回更新响应: version={}, url={}", release.tag_name, releases_tag_url);
    
    Ok(Json(UpdateResponse {
        url: releases_tag_url,
        version: release.tag_name.clone(),
        digest,
        error: None,
    }))
}

#[tokio::main]
async fn main() {
    // 加载 .env 文件
    dotenv::dotenv().ok();
    
    // 初始化日志
    env_logger::init();
    
    info!("正在启动 RustDesk 更新 API 适配层");

    // 初始化全局状态
    let state = Arc::new(AppState {
        http_client: Client::new(),
    });
    APP_STATE.set(state.clone()).unwrap();
    info!("全局状态初始化完成");

    // 构建路由
    let app = Router::new()
        .route("/version/latest", get(handle_update).post(handle_update)) // 同时支持GET和POST请求
        .route("/health", get(|| async { "OK" })) // 添加健康检查端点
        .with_state(state);
    info!("路由构建完成");

    // 从环境变量读取监听地址，默认为 0.0.0.0:3000
    let listen_addr = get_env("LISTEN_ADDR", DEFAULT_LISTEN_ADDR);
    info!("监听地址: {}", listen_addr);
    
    // 启动服务
    let listener = tokio::net::TcpListener::bind(listen_addr.clone()).await
        .unwrap_or_else(|e| {
            error!("启动服务失败: {}", e);
            panic!("启动服务失败: {}", e);
        });
    
    info!("适配层API已启动: http://{}", listen_addr);
    axum::serve(listener, app).await.unwrap();
}