use axum::{
    extract::{Path, Query},
    http::{HeaderMap, HeaderValue, Method},
    response::{IntoResponse, Response},
    routing::{get, Router},
};
use moka::sync::Cache;
use reqwest::StatusCode;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::time::Instant;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use svg::{generate_svg, validate_input};
use thiserror::Error;
use tokio::signal;
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info, warn};
use uuid::Uuid;

// Maximum file size to fetch from GitHub, as we are only expecting small text files, we will not
// fetch anything larger than this size in bytes
const MAX_FILE_SIZE: u64 = 1024;
const USER_AGENT: &str = "AOC-Stars-Generator/0.1.0";

#[derive(Debug)]
enum AppError {
    RateLimitExceeded,
    GitHubFetchError(String),
    ValidationError(String),
    FileTooBig { size: u64, max: u64 },
    NotFound(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::RateLimitExceeded => {
                (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response()
            }
            AppError::GitHubFetchError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
            }
            AppError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            AppError::FileTooBig { size, max } => (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "File size {} bytes exceeds maximum allowed size of {} bytes",
                    size, max
                ),
            )
                .into_response(),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg).into_response(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::RateLimitExceeded => write!(f, "Rate limit exceeded"),
            AppError::GitHubFetchError(msg) => write!(f, "GitHub fetch error: {}", msg),
            AppError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            AppError::FileTooBig { size, max } => {
                write!(
                    f,
                    "File size {} bytes exceeds maximum allowed size of {} bytes",
                    size, max
                )
            }
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
        }
    }
}

impl From<&AppError> for StatusCode {
    fn from(error: &AppError) -> Self {
        match error {
            AppError::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            AppError::GitHubFetchError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ValidationError(_) => StatusCode::BAD_REQUEST,
            AppError::FileTooBig { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
        }
    }
}

type AppResult<T> = Result<T, AppError>;

fn get_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("X-Forwarded-For")
        .and_then(|hv| hv.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("unknown")
        .to_string()
}

fn create_cache_key(user: &str, repo: &str, branch: &str, file: &str) -> String {
    format!("{}/{}/{}/{}", user, repo, branch, file)
}

async fn check_success_cache(
    state: &AppState,
    cache_key: &str,
    request_id: Uuid,
) -> Option<Response> {
    state.cache.get(cache_key).map(|svg_content| {
        let mut response = (
            StatusCode::OK,
            [
                ("Content-Type", "image/svg+xml"),
                ("Cache-Control", "no-cache"),
                ("X-Cache", "HIT"),
            ],
            svg_content,
        )
            .into_response();
        add_response_headers(&mut response, request_id);
        response
    })
}

async fn check_error_cache(state: &AppState, cache_key: &str) -> Option<Response> {
    state
        .error_cache
        .get(cache_key)
        .map(|cached_error| (cached_error.status, cached_error.message).into_response())
}

async fn check_rate_limit(state: &AppState, ip: &str, cache_key: &str) -> AppResult<()> {
    let ip_key = format!("{}:{}", ip, cache_key);
    let mut counts = state.request_counts.lock().await;
    let now = Instant::now();

    let current_info = counts
        .get(&ip_key)
        .map(|(window_start, count)| (*window_start, *count));

    match current_info {
        Some((window_start, count)) => {
            if window_start.elapsed() > state.rate_limiter.window_size {
                info!(
                    client_ip = %ip,
                    cache_key = %cache_key,
                    "Rate limit window expired, resetting count"
                );
                counts.insert(ip_key, (now, 1));
                Ok(())
            } else if state.rate_limiter.is_allowed(count) {
                counts.insert(ip_key, (window_start, count + 1));
                info!(
                    client_ip = %ip,
                    cache_key = %cache_key,
                    count = count + 1,
                    "Request count incremented"
                );
                Ok(())
            } else {
                warn!(
                    client_ip = %ip,
                    cache_key = %cache_key,
                    count = count,
                    max_requests = state.rate_limiter.max_requests,
                    "Rate limit exceeded"
                );
                Err(AppError::RateLimitExceeded)
            }
        }
        None => {
            info!(
                client_ip = %ip,
                cache_key = %cache_key,
                "First request for IP"
            );
            counts.insert(ip_key, (now, 1));
            Ok(())
        }
    }
}

async fn fetch_github_metadata(
    state: &AppState,
    user: &str,
    repo: &str,
    branch: &str,
    txt_file: &str,
) -> AppResult<GitHubFileMetadata> {
    let api_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
        user, repo, txt_file, branch
    );

    let mut request = state.client.get(&api_url).header("User-Agent", USER_AGENT);

    if !state.github_token.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", state.github_token));
    }

    let response = request
        .send()
        .await
        .map_err(|e| AppError::GitHubFetchError(e.to_string()))?;

    if let (Some(remaining), Some(limit)) = (
        response.headers().get("x-ratelimit-remaining"),
        response.headers().get("x-ratelimit-limit"),
    ) {
        info!(
            "GitHub API Rate Limit - Remaining: {}, Total: {}",
            remaining.to_str().unwrap_or("unknown"),
            limit.to_str().unwrap_or("unknown")
        );
    }

    if response.status() == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound(format!("File not found: {}", api_url)));
    }

    response
        .json()
        .await
        .map_err(|e| AppError::GitHubFetchError(format!("Failed to parse metadata: {}", e)))
}

async fn fetch_file_content(state: &AppState, download_url: &str) -> AppResult<String> {
    let mut request = state
        .client
        .get(download_url)
        .header("User-Agent", USER_AGENT);

    if !state.github_token.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", state.github_token));
    }

    let response = request
        .send()
        .await
        .map_err(|e| AppError::GitHubFetchError(e.to_string()))?;

    if let (Some(remaining), Some(limit)) = (
        response.headers().get("x-ratelimit-remaining"),
        response.headers().get("x-ratelimit-limit"),
    ) {
        info!(
            "GitHub API Rate Limit - Remaining: {}, Total: {}",
            remaining.to_str().unwrap_or("unknown"),
            limit.to_str().unwrap_or("unknown")
        );
    }

    if response.status() == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound(format!(
            "File not found: {}",
            download_url
        )));
    }

    response
        .text()
        .await
        .map_err(|e| AppError::GitHubFetchError(format!("Failed to read response: {}", e)))
}

#[derive(Clone)]
struct CachedError {
    status: StatusCode,
    message: String,
}

#[derive(Clone)]
struct RateLimiter {
    window_size: Duration,
    max_requests: u32,
}

impl RateLimiter {
    fn new(window_size: Duration, max_requests: u32) -> Self {
        Self {
            window_size,
            max_requests,
        }
    }

    fn is_allowed(&self, count: u32) -> bool {
        count < self.max_requests
    }
}

#[derive(Deserialize)]
struct GitHubFileMetadata {
    size: u64,
    download_url: String,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Environment variable error: {0}")]
    EnvVar(#[from] std::env::VarError),
    #[error("Invalid number parse: {0}")]
    NumberParse(#[from] std::num::ParseIntError),
    #[error("Invalid address: {0}")]
    AddressError(#[from] std::net::AddrParseError),
}

#[derive(Clone)]
struct Config {
    port: u16,
    host: String,
    cache_ttl_secs: u64,
    http_timeout_secs: u64,
    max_cache_size: u64,
    error_cache_ttl_secs: u64,
    rate_limit_window_secs: u64,
    rate_limit_max_requests: u32,
    github_token: String,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        use std::env;

        Ok(Self {
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()?,
            host: env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            cache_ttl_secs: env::var("CACHE_TTL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()?,
            http_timeout_secs: env::var("HTTP_TIMEOUT_SECS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()?,
            max_cache_size: env::var("MAX_CACHE_SIZE")
                .unwrap_or_else(|_| "1000".to_string())
                .parse()?,
            error_cache_ttl_secs: env::var("ERROR_CACHE_TTL_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()?,
            rate_limit_window_secs: env::var("RATE_LIMIT_WINDOW_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()?,
            rate_limit_max_requests: env::var("RATE_LIMIT_MAX_REQUESTS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()?,
            github_token: env::var("GH_PAT").unwrap_or_default(),
        })
    }
}
#[derive(Clone)]
struct AppState {
    cache: Arc<Cache<String, String>>,
    error_cache: Arc<Cache<String, CachedError>>,
    client: reqwest::Client,
    rate_limiter: Arc<RateLimiter>,
    request_counts: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
    github_token: String,
}

async fn health() -> Response {
    StatusCode::OK.into_response()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::from_env()?;

    info!(
        "Starting server with configuration: port={}, host={}, cache_ttl={}s",
        config.port, config.host, config.cache_ttl_secs
    );

    // Initialize the cache with configuration
    let cache: Cache<String, String> = Cache::builder()
        .time_to_live(Duration::from_secs(config.cache_ttl_secs))
        .time_to_idle(Duration::from_secs(config.cache_ttl_secs * 2))
        .max_capacity(config.max_cache_size)
        .build();

    let error_cache: Cache<String, CachedError> = Cache::builder()
        .time_to_live(Duration::from_secs(config.error_cache_ttl_secs))
        .time_to_idle(Duration::from_secs(config.error_cache_ttl_secs * 2))
        .max_capacity(config.max_cache_size)
        .build();

    let rate_limiter = RateLimiter::new(
        Duration::from_secs(config.rate_limit_window_secs),
        config.rate_limit_max_requests,
    );

    // Initialize reqwest client with timeouts
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.http_timeout_secs))
        .user_agent("GitHub-Stars-Generator/1.0")
        .pool_max_idle_per_host(32)
        .build()?;

    if config.github_token.is_empty() {
        warn!("Running without GitHub token, rate limits will apply");
    } else {
        info!("GitHub API authentication enabled");
    }

    let state = AppState {
        cache: Arc::new(cache),
        error_cache: Arc::new(error_cache),
        client,
        rate_limiter: Arc::new(rate_limiter),
        request_counts: Arc::new(Mutex::new(HashMap::new())),
        github_token: config.github_token,
    };

    // Create CORS layer
    let cors = CorsLayer::new()
        .allow_methods([Method::GET])
        .allow_origin("*".parse::<HeaderValue>().unwrap());

    let app = Router::new()
        .route("/health", get(health))
        .route("/stars/:user/:repo/:branch/*file.svg", get(handle_stars))
        .with_state(state)
        .layer(
            tower::ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(cors),
        );

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, starting graceful shutdown");
}

fn add_response_headers(response: &mut Response, request_id: Uuid) {
    let headers = response.headers_mut();
    headers.insert(
        "X-Request-ID",
        HeaderValue::from_str(&request_id.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("")),
    );
}

#[derive(Deserialize)]
struct QueryParams {
    primary_color: Option<String>,
    secondary_color: Option<String>,
}

async fn handle_stars(
    Path((user, repo, branch, file)): Path<(String, String, String, String)>,
    Query(params): Query<QueryParams>,
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Response {
    let request_id = Uuid::new_v4();
    let cache_key = create_cache_key(&user, &repo, &branch, &file);
    let client_ip = get_client_ip(&headers);

    if let Some(response) = check_success_cache(&state, &cache_key, request_id).await {
        info!(
            client_ip = %client_ip,
            request_id = %request_id,
            cache_key = %cache_key,
            "Cache hit"
        );
        return response;
    }

    if let Some(response) = check_error_cache(&state, &cache_key).await {
        info!(
            client_ip = %client_ip,
            request_id = %request_id,
            cache_key = %cache_key,
            "Error cache hit"
        );
        return response;
    }

    // We only care about checking rate limits after the cache, as the rate limit is to prevent
    // excessive GitHub requests
    if let Err(e) = check_rate_limit(&state, &client_ip, &cache_key).await {
        warn!(
            client_ip = %client_ip,
            request_id = %request_id,
            cache_key = %cache_key,
            "Rate limit exceeded"
        );
        return e.into_response();
    }

    let txt_file = if let Some(name) = file.strip_suffix(".svg") {
        format!("{}.txt", name)
    } else {
        format!("{}.txt", file)
    };

    // Fetch and validate GitHub content
    let metadata = match fetch_github_metadata(&state, &user, &repo, &branch, &txt_file).await {
        Ok(metadata) => metadata,
        Err(e) => {
            error!(
                client_ip = %client_ip,
                request_id = %request_id,
                cache_key = %cache_key,
                error = %e,
                "GitHub metadata fetch failed"
            );
            state.error_cache.insert(
                cache_key,
                CachedError {
                    status: StatusCode::from(&e),
                    message: e.to_string(),
                },
            );
            return e.into_response();
        }
    };

    // Check file size
    if metadata.size > MAX_FILE_SIZE {
        let error = AppError::FileTooBig {
            size: metadata.size,
            max: MAX_FILE_SIZE,
        };
        warn!(
            client_ip = %client_ip,
            request_id = %request_id,
            cache_key = %cache_key,
            size = metadata.size,
            max_size = MAX_FILE_SIZE,
            "File too big"
        );
        state.error_cache.insert(
            cache_key,
            CachedError {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                message: error.to_string(),
            },
        );
        warn!("File too big: {}", metadata.size);
        return error.into_response();
    }

    // Fetch and process content
    let content = match fetch_file_content(&state, &metadata.download_url).await {
        Ok(content) => content,
        Err(e) => {
            error!(
                client_ip = %client_ip,
                request_id = %request_id,
                cache_key = %cache_key,
                error = %e,
                "Content fetch failed"
            );
            state.error_cache.insert(
                cache_key,
                CachedError {
                    status: StatusCode::from(&e),
                    message: e.to_string(),
                },
            );
            return e.into_response();
        }
    };

    // Validate and generate SVG
    let validated_data = match validate_input(&content) {
        Ok(data) => data,
        Err(e) => {
            let error = AppError::ValidationError(e.to_string());
            state.error_cache.insert(
                cache_key.clone(),
                CachedError {
                    status: StatusCode::BAD_REQUEST,
                    message: e.to_string(),
                },
            );
            warn!(
                client_ip = %client_ip,
                request_id = %request_id,
                cache_key = %cache_key,
                error = %e,
                "Validation error"
            );
            return error.into_response();
        }
    };

    let svg_content = generate_svg(validated_data, params.primary_color, params.secondary_color);
    state.cache.insert(cache_key.clone(), svg_content.clone());

    info!(
        client_ip = %client_ip,
        request_id = %request_id,
        cache_key = %cache_key,
        "Successfully generated SVG"
    );

    let mut response = (
        StatusCode::OK,
        [
            ("Content-Type", "image/svg+xml"),
            ("Cache-Control", "no-cache"),
            ("X-Cache", "MISS"),
        ],
        svg_content,
    )
        .into_response();

    add_response_headers(&mut response, request_id);
    response
}
