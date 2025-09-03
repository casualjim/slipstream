use axum::http::StatusCode;
use bytes::BytesMut;
use http_body_util::{BodyExt, Full};
use tracing::instrument;

use crate::app::AppState;
use crate::error::AppError;

#[instrument(level = "info", skip_all, fields(workspace_id = %workspace_id))]
pub async fn upload_files(
  app: &AppState,
  workspace_id: &str,
  mut multipart: axum::extract::multipart::Multipart,
) -> Result<Vec<serde_json::Value>, AppError> {
  let mut results = Vec::new();

  loop {
    let next = multipart.next_field().await.map_err(|e| {
      AppError::new(&format!("invalid multipart: {e}")).with_status(StatusCode::BAD_REQUEST)
    })?;
    let field = match next {
      Some(f) => f,
      None => break,
    };
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

    // Enforce per-file size limit while reading the field
    let max = app.max_file_size_bytes;
    let mut buf = BytesMut::new();
    let mut read: usize = 0;
    let mut f = field;
    while let Some(chunk) = f.chunk().await.map_err(|e| {
      AppError::new(&format!("error reading file part: {e}")).with_status(StatusCode::BAD_REQUEST)
    })? {
      read = read.saturating_add(chunk.len());
      if read > max {
        return Err(AppError::new("file too large").with_status(StatusCode::PAYLOAD_TOO_LARGE));
      }
      buf.extend_from_slice(&chunk);
    }
    let content = buf.freeze();
    let content_len = content.len();

    let body = Full::new(content)
      .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "error converting to body"))
      .boxed();

    app
      .storage
      .put_body(&app.bucket, &key, content_type.as_deref(), body)
      .await
      .map_err(|e| {
        tracing::error!(error = %e, "put_body failed");
        AppError::new("storage unavailable").with_status(StatusCode::BAD_GATEWAY)
      })?;

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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::testing::TestCtx;
  use axum::{
    Router,
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
  };
  use bytes::Bytes;

  #[tokio::test]
  async fn minio_container_is_reachable_and_operable() {
    let ctx = TestCtx::new().await;

    // Put an object
    let key = "hello.txt";
    ctx
      .client
      .put_object()
      .bucket(&ctx.bucket)
      .key(key)
      .body(aws_sdk_s3::primitives::ByteStream::from_static(
        b"hello world",
      ))
      .send()
      .await
      .expect("put_object");

    // Head the object to ensure it exists
    ctx
      .client
      .head_object()
      .bucket(&ctx.bucket)
      .key(key)
      .send()
      .await
      .expect("head_object");

    // Get and verify contents
    let out = ctx
      .client
      .get_object()
      .bucket(&ctx.bucket)
      .key(key)
      .send()
      .await
      .expect("get_object");
    let bytes = out.body.collect().await.expect("collect body").into_bytes();
    assert_eq!(&bytes[..], b"hello world");
  }

  async fn route_upload(
    State(app): State<crate::app::AppState>,
    Path(ws): Path<String>,
    multipart: Multipart,
  ) -> Result<impl IntoResponse, crate::error::AppError> {
    let results = upload_files(&app, &ws, multipart).await?;
    Ok((StatusCode::CREATED, axum::Json(results)))
  }

  #[tokio::test]
  async fn upload_happy_path_streams_to_minio() {
    let ctx = TestCtx::new().await;

    // App state using shared test context
    let state = crate::app::AppState {
      storage: ctx.storage.clone(),
      bucket: ctx.bucket.clone(),
      max_file_size_bytes: 6 * 1024 * 1024, // match DefaultBodyLimit layer above for this test
    };

    // Router and server
    let app = Router::new()
      .route("/api/v1/workspaces/{ws}/uploads", post(route_upload))
      .layer(axum::extract::DefaultBodyLimit::max(
        state.max_file_size_bytes,
      ))
      .with_state(state);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
      .await
      .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
      axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
    });

    // Build multipart with reqwest using a real file so per-part Content-Length is set
    let file_bytes = Bytes::from_static(b"hello world");
    let file_len = file_bytes.len() as u64;
    let part = reqwest::multipart::Part::stream_with_length(file_bytes, file_len)
      .file_name("a.txt")
      .mime_str("text/plain")
      .expect("invalid mime for file");
    let form = reqwest::multipart::Form::new().part("file", part);

    let url = format!("http://{}/api/v1/workspaces/ws1/uploads", addr);
    let client = reqwest::Client::new();
    let resp = client.post(&url).multipart(form).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);

    // Shutdown server task
    server.abort();
    ctx.stop().await;
  }

  #[tokio::test]
  async fn upload_rejects_oversized_file() {
    let ctx = TestCtx::new().await;

    // Configure per-file limit lower than overall body limit to ensure our per-file check triggers
    let per_file_limit = 1 * 1024 * 1024; // 1 MiB

    let state = crate::app::AppState {
      storage: ctx.storage.clone(),
      bucket: ctx.bucket.clone(),
      max_file_size_bytes: per_file_limit,
    };

    // Body limit larger than per-file limit so the request is accepted at HTTP layer
    let app = Router::new()
      .route("/api/v1/workspaces/{ws}/uploads", post(route_upload))
      .layer(axum::extract::DefaultBodyLimit::max(per_file_limit * 2))
      .with_state(state);

    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
      .await
      .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
      axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
    });

    // Build multipart with a part just over the per-file limit
    let oversized_len = per_file_limit + 1;
    let file_bytes = Bytes::from(vec![0u8; oversized_len]);
    let part = reqwest::multipart::Part::stream_with_length(file_bytes, oversized_len as u64)
      .file_name("big.bin")
      .mime_str("application/octet-stream")
      .expect("invalid mime for file");
    let form = reqwest::multipart::Form::new().part("file", part);

    let url = format!("http://{}/api/v1/workspaces/ws1/uploads", addr);
    let client = reqwest::Client::new();
    let resp = client.post(&url).multipart(form).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);

    server.abort();
    ctx.stop().await;
  }

  #[tokio::test]
  async fn upload_rejects_oversized_among_multiple_files() {
    let ctx = TestCtx::new().await;

    let per_file_limit = 1 * 1024 * 1024; // 1 MiB
    let state = crate::app::AppState {
      storage: ctx.storage.clone(),
      bucket: ctx.bucket.clone(),
      max_file_size_bytes: per_file_limit,
    };

    // Allow total body up to 3 MiB
    let app = Router::new()
      .route("/api/v1/workspaces/{ws}/uploads", post(route_upload))
      .layer(axum::extract::DefaultBodyLimit::max(per_file_limit * 3))
      .with_state(state);

    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
      .await
      .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
      axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
    });

    // Parts: small (100 KiB), big (per_file_limit+1), small2 (100 KiB) => total < body limit, but one part too big
    let small_a = Bytes::from(vec![0u8; 100 * 1024]);
    let big_len = per_file_limit + 1;
    let big = Bytes::from(vec![0u8; big_len]);
    let small_b = Bytes::from(vec![0u8; 100 * 1024]);

    let p1 = reqwest::multipart::Part::stream_with_length(small_a, 100 * 1024)
      .file_name("a.txt")
      .mime_str("application/octet-stream")
      .unwrap();
    let p2 = reqwest::multipart::Part::stream_with_length(big, big_len as u64)
      .file_name("b.bin")
      .mime_str("application/octet-stream")
      .unwrap();
    let p3 = reqwest::multipart::Part::stream_with_length(small_b, 100 * 1024)
      .file_name("c.txt")
      .mime_str("application/octet-stream")
      .unwrap();

    // Use the same field name "file" for each part
    let form = reqwest::multipart::Form::new()
      .part("file", p1)
      .part("file", p2)
      .part("file", p3);

    let url = format!("http://{}/api/v1/workspaces/ws1/uploads", addr);
    let client = reqwest::Client::new();
    let resp = client.post(&url).multipart(form).send().await.unwrap();

    assert_eq!(resp.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);

    server.abort();
    ctx.stop().await;
  }

  #[tokio::test]
  async fn upload_succeeds_when_multiple_files_each_under_limit_but_total_large() {
    let ctx = TestCtx::new().await;

    let per_file_limit = 1 * 1024 * 1024; // 1 MiB
    let state = crate::app::AppState {
      storage: ctx.storage.clone(),
      bucket: ctx.bucket.clone(),
      max_file_size_bytes: per_file_limit,
    };

    // Allow total body up to ~3 MiB
    let app = Router::new()
      .route("/api/v1/workspaces/{ws}/uploads", post(route_upload))
      .layer(axum::extract::DefaultBodyLimit::max(per_file_limit * 3))
      .with_state(state);

    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
      .await
      .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
      axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
    });

    // Three files each 900 KiB < 1 MiB => per-file OK, total ~2.6 MiB < 3 MiB body limit
    let mk = |n: usize| Bytes::from(vec![n as u8; 900 * 1024]);
    let p1 = reqwest::multipart::Part::stream_with_length(mk(1), 900 * 1024)
      .file_name("p1.bin")
      .mime_str("application/octet-stream")
      .unwrap();
    let p2 = reqwest::multipart::Part::stream_with_length(mk(2), 900 * 1024)
      .file_name("p2.bin")
      .mime_str("application/octet-stream")
      .unwrap();
    let p3 = reqwest::multipart::Part::stream_with_length(mk(3), 900 * 1024)
      .file_name("p3.bin")
      .mime_str("application/octet-stream")
      .unwrap();

    let form = reqwest::multipart::Form::new()
      .part("file", p1)
      .part("file", p2)
      .part("file", p3);

    let url = format!("http://{}/api/v1/workspaces/ws1/uploads", addr);
    let client = reqwest::Client::new();
    let resp = client.post(&url).multipart(form).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);

    server.abort();
    ctx.stop().await;
  }
}
