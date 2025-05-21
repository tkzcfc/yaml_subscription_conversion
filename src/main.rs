mod config;

use actix_web::http::header::LOCATION;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_httpauth::extractors::basic::{BasicAuth};
use base64::engine::general_purpose;
use base64::Engine;
use clap::Parser;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The address to listen on
    #[arg(long, default_value = "0.0.0.0:8000")]
    listen_addr: String,

    #[arg(long, default_value = "root")]
    username: String,

    #[arg(long, default_value = "password")]
    password: String,

    #[arg(long, default_value = "https://no8-svip.urlapi-dodo.mom/s?t=678846704c2e2db4a83af334bca5b38b")]
    url: String,

    #[arg(long, default_value = "config.yaml")]
    config_file: String,
}

#[derive(Clone)]
pub struct AppState {
    pub username: String,
    pub password: String,
    pub url: String,
    pub config_path: String,
    pub local_config: Arc<Mutex<serde_yaml::Value>>,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // 初始化本地配置
    let local_config = config::load_local_config(&args.config_file)?;

    let app_state = AppState {
        username: args.username.clone(),
        password: args.password.clone(),
        url: args.url.clone(),
        config_path: args.config_file.clone(),
        local_config: Arc::new(Mutex::new(local_config)),
    };

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .service(api_get_subscription)
            .service(api_update_config)
            .service(api_get_config)
            .service(api_login)
            .service(html_index)
    })
    .bind(&args.listen_addr)?
    .run()
    .await?;

    Ok(())
}

// 用户鉴权
pub async fn user_authentication(
    _req: &HttpRequest,
    credentials: &BasicAuth,
    app_data: &web::Data<AppState>,
) -> actix_web::Result<()> {
    let username = credentials.user_id();
    let password = credentials.password().unwrap_or_default();

    if !app_data.username.is_empty() || !app_data.password.is_empty() {
        // 验证用户名和密码
        return if username == app_data.username.as_str() && password == app_data.password.as_str() {
            Ok(())
        } else {
            Err(actix_web::error::ErrorNetworkAuthenticationRequired("Unauthenticated"))
            // let config = _req.app_data::<actix_web_httpauth::extractors::basic::Config>().cloned().unwrap_or_default();
            // Err(actix_web_httpauth::extractors::AuthenticationError::from(config).into())
        };
    }

    Ok(())
}

const FAVICON: &[u8] = include_bytes!("../html/favicon.ico");

#[get("/{filename:.*}")]
pub async fn html_index(
    req: HttpRequest,
) -> actix_web::Result<impl Responder> {
    let file_name = req.match_info().query("filename");

    if file_name == "favicon.ico" {
        return Ok(HttpResponse::Ok()
            .content_type("image/vnd.microsoft.icon")
            .body(FAVICON));
    }

    if file_name != "index.html" {
        return Ok(HttpResponse::Found()
            .append_header((LOCATION, "/index.html"))
            .finish());
    }

    let html = include_str!("../html/index.html");
    Ok(HttpResponse::Ok().content_type("text/html").body(html))
}

#[get("/api/get_subscription")]
pub async fn api_get_subscription(
    _req: HttpRequest,
    app_data: web::Data<AppState>,
) -> actix_web::Result<impl Responder> {
    let remote_config = config::fetch_remote_config(&app_data.url).await?;

    let local_config = app_data.local_config.lock().await;
    let merged_config = config::merge_configs(remote_config, &local_config)?;

    let yaml = serde_yaml::to_string(&merged_config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!(
            "serde_yaml::to_string error:{}",
            e.to_string()
        ))
    })?;

    // let mut body = String::new();
    // general_purpose::STANDARD.encode_string(yaml, &mut body);

    Ok(HttpResponse::Ok().body(yaml))
}

#[get("/api/login")]
pub async fn api_login(
    req: HttpRequest,
    credentials: BasicAuth,
    app_data: web::Data<AppState>,
) -> actix_web::Result<impl Responder> {
    user_authentication(&req, &credentials, &app_data).await?;

    Ok(HttpResponse::Ok().body("login success"))
}

#[get("/api/get_config")]
pub async fn api_get_config(
    req: HttpRequest,
    credentials: BasicAuth,
    app_data: web::Data<AppState>,
) -> actix_web::Result<impl Responder> {
    user_authentication(&req, &credentials, &app_data).await?;

    let local_config = app_data.local_config.lock().await;
    let yaml = serde_yaml::to_string(&*local_config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!(
            "serde_yaml::to_string error:{}",
            e.to_string()
        ))
    })?;

    Ok(HttpResponse::Ok().body(yaml))
}

#[derive(Deserialize, Debug)]
struct UpdateConfigRequestData {
    config_data: String,
}
#[post("/api/update_config")]
pub async fn api_update_config(
    req: HttpRequest,
    credentials: BasicAuth,
    app_data: web::Data<AppState>,
    json_data: web::Json<UpdateConfigRequestData>,
) -> actix_web::Result<HttpResponse> {
    user_authentication(&req, &credentials, &app_data).await?;

    let new_config: serde_yaml::Value =
        serde_yaml::from_str(&json_data.config_data).map_err(|e| {
            actix_web::error::ErrorExpectationFailed(format!(
                "serde_yaml::to_string error:{}",
                e.to_string()
            ))
        })?;

    // 更新内存中的配置
    let mut local_config = app_data.local_config.lock().await;
    *local_config = new_config.clone();

    // 保存到本地配置文件
    config::save_local_config(&app_data.config_path, &new_config).map_err(|e| {
        actix_web::error::ErrorExpectationFailed(format!(
            "save_local_config error:{}",
            e.to_string()
        ))
    })?;

    Ok(HttpResponse::Ok().body("Config updated successfully"))
}
