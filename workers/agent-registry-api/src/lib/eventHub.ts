export class EventHub {
  private clients: Set<ReadableStreamDefaultController<Uint8Array>> = new Set();

  constructor(private state: DurableObjectState, private env: any) {}

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const encoder = new TextEncoder();

    // SSE subscribe
    if (request.method === 'GET' && url.pathname === '/events') {
      const stream = new ReadableStream<Uint8Array>({
        start: (controller) => {
          this.clients.add(controller);
        },
        cancel: (reason) => {
          // Remove on disconnect
          for (const ctrl of this.clients) {
            if (ctrl === reason) {
              this.clients.delete(ctrl);
            }
          }
        },
      });

      return new Response(stream, {
        headers: {
          'Content-Type': 'text/event-stream',
          'Cache-Control': 'no-cache',
          Connection: 'keep-alive',
        },
      });
    }

    // Broadcast event
    if (request.method === 'POST' && url.pathname === '/events') {
      const payload = await request.text();
      const data = `data: ${payload}\n\n`;
      for (const controller of this.clients) {
        controller.enqueue(encoder.encode(data));
      }
      return new Response(null, { status: 204 });
    }

    return new Response('Not Found', { status: 404 });
  }
}
