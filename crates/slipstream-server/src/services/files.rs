use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};

use axum::{
  Json, Router,
  body::Body,
  extract::{DefaultBodyLimit, Multipart, Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::{get, post},
};
use futures::StreamExt;
use http_body_util::{BodyExt, StreamBody};
use tracing::instrument;

use crate::{
  app::AppState,
  error::AppError,
  storage::{StorageError, StorageObject},
};

// Combined routes for all file operations
pub fn routes() -> Router<AppState> {
  Router::new()
    .route("/api/v1/workspaces/{workspace_id}/files", get(list_files))
    .route(
      "/api/v1/workspaces/{workspace_id}/files",
      post(handle_upload).layer(DefaultBodyLimit::max(55 * 1024 * 1024)),
    )
    .route(
      "/api/v1/workspaces/{workspace_id}/files/{*key}",
      get(download_file).delete(delete_file),
    )
}

// Thin handler for uploads, delegates to the service layer
async fn handle_upload(
  State(app): State<AppState>,
  Path(workspace_id): Path<String>,
  multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
  let results = upload_files(&app, &workspace_id, multipart).await?;
  Ok((StatusCode::CREATED, Json(results)))
}

#[instrument(level = "info", skip_all, fields(workspace_id = %workspace_id))]
async fn upload_files(
  app: &AppState,
  workspace_id: &str,
  mut multipart: axum::extract::multipart::Multipart,
) -> Result<Vec<serde_json::Value>, AppError> {
  let mut results = Vec::new();

  while let Some(field) = multipart.next_field().await.map_err(|e| {
    AppError::new(&format!("invalid multipart: {e}")).with_status(StatusCode::BAD_REQUEST)
  })? {
    if field.name() != Some("file") {
      continue;
    }

    let filename = field
      .file_name()
      .map(|s| s.to_string())
      .ok_or_else(|| AppError::new("missing filename").with_status(StatusCode::BAD_REQUEST))?;

    let content_type = field.content_type().map(|s| s.to_string());
    let key = format!("workspaces/{}/{}", workspace_id, filename);

    match app.storage.exists(&app.bucket, &key).await {
      Ok(true) => {
        return Err(AppError::new("file already exists").with_status(StatusCode::CONFLICT));
      }
      Ok(false) => {}
      Err(e) => {
        tracing::error!(err=?e, "storage.exists failed");
        return Err(AppError::new("storage unavailable").with_status(StatusCode::BAD_GATEWAY));
      }
    }

    let (tx, rx) = flume::bounded::<Result<bytes::Bytes, std::io::Error>>(16);
    let size_counter = Arc::new(AtomicUsize::new(0));

    let producer_task = {
      let size_counter = size_counter.clone();
      let max_bytes = app.max_file_size_bytes;

      async move {
        let mut field_stream = field; // take ownership
        let mut total_read: usize = 0;

        while let Some(chunk_res) = field_stream.chunk().await.transpose() {
          match chunk_res {
            Ok(chunk) => {
              total_read = total_read.saturating_add(chunk.len());
              if total_read > max_bytes {
                let err =
                  std::io::Error::new(std::io::ErrorKind::PermissionDenied, "file too large");
                tx.send_async(Err(err)).await.ok(); // Inform receiver
                return;
              }

              size_counter.store(total_read, Ordering::Relaxed);

              if tx.send_async(Ok(chunk)).await.is_err() {
                // Receiver was dropped, probably because the upload failed.
                return;
              }
            }
            Err(e) => {
              let err = std::io::Error::new(std::io::ErrorKind::Other, e);
              tx.send_async(Err(err)).await.ok();
              return;
            }
          }
        }
      }
    };

    let consumer_task = {
      let storage = Arc::clone(&app.storage);
      let bucket = app.bucket.clone();
      let key = key.clone();
      let content_type_clone = content_type.clone();

      async move {
        let body = BodyExt::boxed(StreamBody::new(
          rx.into_stream().map(|res| res.map(http_body::Frame::data)),
        ));
        storage
          .put_body(&bucket, &key, content_type_clone.as_deref(), body)
          .await
      }
    };

    let (_, consumer_res) = tokio::join!(producer_task, consumer_task);

    if let Err(e) = consumer_res {
      if e.to_string().contains("file too large") {
        return Err(AppError::new("file too large").with_status(StatusCode::PAYLOAD_TOO_LARGE));
      }
      tracing::error!(error = %e, "put_body failed");
      return Err(AppError::new("storage unavailable").with_status(StatusCode::BAD_GATEWAY));
    }

    let content_len = size_counter.load(Ordering::Relaxed);

    results.push(serde_json::json!({
      "filename": filename,
      "key": key,
      "size": content_len,
      "content_type": content_type,
    }));
  }

  if results.is_empty() {
    return Err(AppError::new("missing 'file' field").with_status(StatusCode::BAD_REQUEST));
  }

  Ok(results)
}

async fn list_files(
  State(app): State<AppState>,
  Path(ws): Path<String>,
) -> Result<Json<Vec<StorageObject>>, AppError> {
  let prefix = format!("workspaces/{}/", ws);
  let objects = app.storage.list(&app.bucket, &prefix).await?;
  Ok(Json(objects))
}

async fn download_file(
  State(app): State<AppState>,
  Path((ws, key)): Path<(String, String)>,
) -> Result<Response, AppError> {
  let key = format!("workspaces/{}/", ws) + &key;
  let body = app.storage.get(&app.bucket, &key).await?;
  Ok(Response::new(Body::new(body)))
}

async fn delete_file(
  State(app): State<AppState>,
  Path((ws, key)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
  let key = format!("workspaces/{}/", ws) + &key;
  app.storage.delete(&app.bucket, &key).await?;
  Ok(StatusCode::NO_CONTENT)
}

impl From<StorageError> for AppError {
  fn from(err: StorageError) -> Self {
    match err {
      StorageError::NotFound => AppError::new("not found").with_status(StatusCode::NOT_FOUND),
      StorageError::AlreadyExists => {
        AppError::new("already exists").with_status(StatusCode::CONFLICT)
      }
      StorageError::Unavailable(e) => {
        AppError::new(&format!("storage unavailable: {e}")).with_status(StatusCode::BAD_GATEWAY)
      }
      StorageError::Timeout => {
        AppError::new("storage timeout").with_status(StatusCode::GATEWAY_TIMEOUT)
      }
      StorageError::Internal(e) => AppError::new(&format!("internal storage error: {e}"))
        .with_status(StatusCode::INTERNAL_SERVER_ERROR),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::testing::TestCtx;
  use axum::body::to_bytes;
  use tower::ServiceExt;

  async fn setup_test_app() -> (Router, TestCtx) {
    let ctx = TestCtx::new().await;
    let state = AppState {
      storage: Arc::clone(&ctx.storage),
      bucket: ctx.bucket.clone(),
      max_file_size_bytes: 10 * 1024 * 1024,
    };

    let app = Router::new().merge(routes()).with_state(state.clone());

    (app, ctx)
  }

  #[tokio::test]
  async fn test_file_operations_flow() {
    let (app, ctx) = setup_test_app().await;
    let workspace = "test-ws";
    let filename = "test-file.txt";
    let file_content = b"Hello, world!";

    // 1. Upload a file
    let part = reqwest::multipart::Part::bytes(file_content.to_vec())
      .file_name(filename.to_string())
      .mime_str("text/plain")
      .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let boundary = form.boundary().to_string();
    let stream = form.into_stream();

    let body = axum::body::Body::from_stream(stream);

    let upload_req = axum::http::Request::builder()
      .method("POST")
      .uri(format!("/api/v1/workspaces/{}/files", workspace))
      .header(
        "Content-Type",
        format!("multipart/form-data; boundary={}", boundary),
      )
      .body(body)
      .unwrap();

    let upload_resp = app.clone().oneshot(upload_req).await.unwrap();
    assert_eq!(upload_resp.status(), StatusCode::CREATED);

    // 2. List files
    let list_req = axum::http::Request::builder()
      .method("GET")
      .uri(format!("/api/v1/workspaces/{}/files", workspace))
      .body(axum::body::Body::empty())
      .unwrap();

    let list_resp = app.clone().oneshot(list_req).await.unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let body_bytes = to_bytes(list_resp.into_body(), usize::MAX).await.unwrap();
    let objects: Vec<StorageObject> = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(
      objects[0].key,
      format!("workspaces/{}/{}", workspace, filename)
    );
    assert_eq!(objects[0].size, file_content.len() as u64);

    // 3. Download file
    let download_req = axum::http::Request::builder()
      .method("GET")
      .uri(format!(
        "/api/v1/workspaces/{}/files/{}",
        workspace,
        objects[0]
          .key
          .strip_prefix(&format!("workspaces/{}/", workspace))
          .unwrap()
      ))
      .body(axum::body::Body::empty())
      .unwrap();

    let download_resp = app.clone().oneshot(download_req).await.unwrap();
    assert_eq!(download_resp.status(), StatusCode::OK);
    let body_bytes = to_bytes(download_resp.into_body(), usize::MAX)
      .await
      .unwrap();
    assert_eq!(&body_bytes[..], file_content);

    // 4. Delete file
    let delete_req = axum::http::Request::builder()
      .method("DELETE")
      .uri(format!(
        "/api/v1/workspaces/{}/files/{}",
        workspace,
        objects[0]
          .key
          .strip_prefix(&format!("workspaces/{}/", workspace))
          .unwrap()
      ))
      .body(axum::body::Body::empty())
      .unwrap();

    let delete_resp = app.clone().oneshot(delete_req).await.unwrap();
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

    // 5. Verify deletion by listing again
    let list_req_after_delete = axum::http::Request::builder()
      .method("GET")
      .uri(format!("/api/v1/workspaces/{}/files", workspace))
      .body(axum::body::Body::empty())
      .unwrap();

    let list_resp_after_delete = app.clone().oneshot(list_req_after_delete).await.unwrap();
    assert_eq!(list_resp_after_delete.status(), StatusCode::OK);
    let body_bytes = to_bytes(list_resp_after_delete.into_body(), usize::MAX)
      .await
      .unwrap();
    let objects_after_delete: Vec<StorageObject> = serde_json::from_slice(&body_bytes).unwrap();
    assert!(objects_after_delete.is_empty());

    // 6. Try to download deleted file
    let download_deleted_req = axum::http::Request::builder()
      .method("GET")
      .uri(format!(
        "/api/v1/workspaces/{}/files/{}",
        workspace,
        objects[0]
          .key
          .strip_prefix(&format!("workspaces/{}/", workspace))
          .unwrap()
      ))
      .body(axum::body::Body::empty())
      .unwrap();

    let download_deleted_resp = app.oneshot(download_deleted_req).await.unwrap();
    assert_eq!(download_deleted_resp.status(), StatusCode::NOT_FOUND);

    ctx.stop().await;
  }
}
