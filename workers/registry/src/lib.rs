use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use worker::*;

#[derive(Serialize, Deserialize)]
pub struct CallbackMessage {
  callback_url: Option<String>,
  queue_name: Option<String>,
  method: String,
  payload: serde_json::Value,
  correlation_id: String,
  timestamp: u64,
}

#[derive(Serialize, Deserialize)]
pub struct AgentRegistration {
  pub name: String,
  pub callback_url: Option<String>,
  pub callback_queue: Option<String>,
  pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ApiResponse {
  pub status: String,
  pub correlation_id: String,
  pub message: String,
}

#[derive(Serialize, Deserialize)]
struct AgentProcessingResult {
  agent_id: String,
  status: String,
  result: serde_json::Value,
  error: Option<String>,
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
  console_error_panic_hook::set_once();

  let router = Router::new();

  router
    .get("/", |_, _| Response::ok("Agent Registry API"))
    .post_async("/agents", register_agent)
    .run(req, env)
    .await
}

async fn register_agent(mut req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
  let registration: AgentRegistration = req.json().await?;
  let env = ctx.env;
  let correlation_id = uuid::Uuid::now_v7().to_string();

  // Process the registration (this would typically involve D1 operations)
  let agent_id = uuid::Uuid::now_v7().to_string();

  // If a callback is specified, queue the callback message
  if registration.callback_url.is_some() || registration.callback_queue.is_some() {
    let callback_message = CallbackMessage {
      callback_url: registration.callback_url.clone(),
      queue_name: registration.callback_queue.clone(),
      method: "agent_registered".to_string(),
      payload: serde_json::json!({
        "agent_id": agent_id,
        "name": registration.name,
        "status": "registered",
        "metadata": registration.metadata
      }),
      correlation_id: correlation_id.clone(),
      timestamp: js_sys::Date::now() as u64,
    };

    // Send to callback queue for async processing
    env.queue("CALLBACK_QUEUE")?.send(&callback_message).await?;
  }

  let response = ApiResponse {
    status: "accepted".to_string(),
    correlation_id,
    message: format!(
      "Agent {} registration queued for processing",
      registration.name
    ),
  };

  Response::from_json(&response)
}

#[event(queue)]
pub async fn queue_handler(
  message_batch: MessageBatch<CallbackMessage>,
  _env: Env,
  _ctx: Context,
) -> worker::Result<()> {
  for message in message_batch.messages()? {
    let callback_msg = message.body();

    // Process the callback message
    if let Some(callback_url) = &callback_msg.callback_url {
      // Make HTTP callback
      let mut init = RequestInit::new();
      init
        .with_method(Method::Post)
        .with_headers({
          let headers = Headers::new();
          headers.set("Content-Type", "application/json")?;
          headers
        })
        .with_body(Some(JsValue::from_str(&serde_json::to_string(
          &callback_msg.payload,
        )?)));

      let response = Fetch::Request(Request::new_with_init(callback_url, &init)?)
        .send()
        .await;

      match response {
        Ok(resp) if resp.status_code() < 400 => {
          console_log!("Callback delivered successfully to {}", callback_url);
        }
        Ok(resp) => {
          console_log!(
            "Callback failed with status {}: {}",
            resp.status_code(),
            callback_url
          );
        }
        Err(e) => {
          console_log!("Failed to deliver callback to {}: {:?}", callback_url, e);
        }
      }
    }

    // If there's also a queue name, forward to that queue
    if let Some(queue_name) = &callback_msg.queue_name {
      // Note: In a real implementation, you'd need to get the target queue
      console_log!("Would forward message to queue: {}", queue_name);
    }

    message.ack();
  }

  Ok(())
}
