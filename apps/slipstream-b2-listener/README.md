Slipstream B2 Listener

Small Axum web service that receives Backblaze B2 Event Notification webhooks and forwards the raw payload to a NATS subject.

Env Vars
- NATS_URL: NATS server URL (default: nats://localhost:4222)
- NATS_SINK_TOPIC or NATS_SINK_SUBJECT: subject to publish to (default: slipstream.events.backblaze)
- PORT: HTTP listen port (default: 8080)
- B2_WEBHOOK_SECRET or BACKBLAZE_WEBHOOK_SECRET: shared secret for signature validation; if unset, signatures are not verified
- AWS_S3_BUCKET: optional bucket-name filter; events for other buckets are ignored
- AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY/AWS_REGION: not used directly by the listener today; kept for future enhancements and to keep env shape consistent with S3-compatible tooling

Run Locally
```
cargo run --package slipstream-b2-listener
# or
PORT=8787 NATS_SINK_TOPIC=slipstream.events.b2 cargo run --package slipstream-b2-listener
```

HTTP Endpoints
- POST /webhook: Backblaze target for event notifications (also used as fallback for unknown paths)
- GET/POST /healthz: health check

Signature Validation
If a secret is configured, requests must include the `X-Bz-Event-Notification-Signature` header. The service verifies an HMAC-SHA256 signature of the raw request body using the shared secret. It accepts either a raw hex digest or the form `v1=<hex>`.

Backblaze Setup (Console)
1. Open B2 Cloud Storage → Buckets → (your bucket) → Event Notifications.
2. Add a rule:
   - Destination: Webhook URL → `http://<host>:<port>/webhook`
   - Secret: choose a strong value and set it as `B2_WEBHOOK_SECRET` in this service
   - Events: choose the events you care about (e.g., Object Created, Object Deleted)
3. Save. Send a test event from the console if available to validate 200 OK.

Backblaze Setup (API)
Use `b2_set_bucket_notification_rules` on the B2 Native API to create/replace rules. This is separate from the S3-compatible API. See Backblaze docs for the exact JSON shape and supported event types.

NATS Message
The service publishes the raw JSON payload to the configured subject. Consumers can parse according to Backblaze’s documented event schema.

