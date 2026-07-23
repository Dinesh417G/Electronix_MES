// Subscribe to the edge `/ws` live event bus (§10). Reconnects with a small
// backoff so a dropped LAN link recovers on its own — the kiosk must keep
// working offline-ish and pick events back up when the edge returns.

import { useEffect, useRef, useState } from "react";
import { edgeUrl } from "./client";
import type { WsEvent } from "./types";

export function useWebSocket(onEvent: (e: WsEvent) => void): { connected: boolean } {
  const [connected, setConnected] = useState(false);
  const cbRef = useRef(onEvent);
  cbRef.current = onEvent;

  useEffect(() => {
    let ws: WebSocket | null = null;
    let closed = false;
    let retry = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const wsUrl = edgeUrl.replace(/^http/, "ws") + "/ws";

    const connect = () => {
      ws = new WebSocket(wsUrl);
      ws.onopen = () => {
        retry = 0;
        setConnected(true);
      };
      ws.onclose = () => {
        setConnected(false);
        if (!closed) {
          retry = Math.min(retry + 1, 6);
          timer = setTimeout(connect, 500 * retry);
        }
      };
      ws.onerror = () => ws?.close();
      ws.onmessage = (msg) => {
        try {
          cbRef.current(JSON.parse(msg.data) as WsEvent);
        } catch {
          // ignore malformed frames
        }
      };
    };

    connect();
    return () => {
      closed = true;
      if (timer) clearTimeout(timer);
      ws?.close();
    };
  }, []);

  return { connected };
}
