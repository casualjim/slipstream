use crate::app::AppState;
use crate::error::AppError;
use crate::models::AddMessages;
use axum::http::StatusCode;

/// Service function to add messages to memory
///
/// This function is currently stubbed and will return a 501 Not Implemented error.
/// Future implementation will process the messages and add them to the graph memory.
pub async fn add_messages_service(_state: &AppState, request: AddMessages) -> Result<(), AppError> {
  tracing::info!(
    group_id = %request.group_id,
    message_count = request.messages.len(),
    "Processing add messages request"
  );

  // Log some details about the messages for debugging
  for (i, message) in request.messages.iter().enumerate() {
    tracing::debug!(
      message_index = i,
      sender_name = %message.name,
      role = ?message.role,
      content_length = message.content.len(),
      "Message details"
    );
  }

  // Return 501 Not Implemented for now
  Err(
    AppError::new("Message processing not yet implemented")
      .with_status(StatusCode::NOT_IMPLEMENTED),
  )
}
