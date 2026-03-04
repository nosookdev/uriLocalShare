use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use crate::types::AppState;
use std::path::PathBuf;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct DownloadQuery {
    pub path: String,
}

pub async fn start_file_server(state: Arc<AppState>) {
    let app = Router::new()
        .route("/download", get(handle_download))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    // 포트 0으로 시작하여 OS가 가용한 포트를 할당하도록 함
    let addr = SocketAddr::from(([0, 0, 0, 0], 0));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let actual_port = listener.local_addr().unwrap().port();
    
    // 할당된 포트를 상태에 저장
    {
        let mut port = state.local_server_port.lock().unwrap();
        *port = actual_port;
        println!("File server listening on dynamic port: {}", actual_port);
    }

    axum::serve(listener, app).await.unwrap();
}

async fn handle_download(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DownloadQuery>,
) -> impl IntoResponse {
    let mut target_path = None;
    let requested_path = query.path;
    
    {
        let shared_folders = state.shared_folders.lock().unwrap();
        for folder in shared_folders.iter() {
            // 경로 대소문자나 구분자 차이를 감안하여 starts_with 확인
            if requested_path.to_lowercase().starts_with(&folder.path.to_lowercase()) {
                let p = std::path::PathBuf::from(&requested_path);
                if p.exists() && p.is_file() {
                    target_path = Some(p);
                    break;
                }
            }
        }
    }

    if let Some(p) = target_path {
        match tokio::fs::File::open(&p).await {
            Ok(file) => {
                let stream = tokio_util::io::ReaderStream::new(file);
                let body = axum::body::Body::from_stream(stream);
                let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("download");
                
                return Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/octet-stream")
                    .header("Content-Disposition", format!("attachment; filename=\"{}\"", file_name))
                    .body(body)
                    .unwrap()
                    .into_response();
            }
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to open file").into_response(),
        }
    }

    (StatusCode::NOT_FOUND, "File not found or not authorized").into_response()
}
