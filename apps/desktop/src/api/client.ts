// Typed fetch wrapper around the edge (and, for the copilot, cloud) API.
// The bearer token is injected from the auth store; a 401 clears it so the app
// falls back to the login screen.

const EDGE_URL =
  (import.meta.env.VITE_MES_EDGE_URL as string | undefined)?.replace(/\/$/, "") ??
  "http://localhost:8080";

const CLOUD_URL =
  (import.meta.env.VITE_MES_CLOUD_URL as string | undefined)?.replace(/\/$/, "") ??
  "http://localhost:8081";

export const edgeUrl = EDGE_URL;
export const cloudUrl = CLOUD_URL;

const TOKEN_KEY = "mes.token";

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string | null) {
  if (token) localStorage.setItem(TOKEN_KEY, token);
  else localStorage.removeItem(TOKEN_KEY);
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

interface RequestOptions {
  method?: string;
  body?: unknown;
  base?: string;
  auth?: boolean;
}

export async function api<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const { method = "GET", body, base = EDGE_URL, auth = true } = opts;
  const headers: Record<string, string> = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (auth) {
    const token = getToken();
    if (token) headers["Authorization"] = `Bearer ${token}`;
  }

  const resp = await fetch(`${base}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (resp.status === 401) {
    setToken(null);
  }

  if (!resp.ok) {
    let code = "error";
    let message = resp.statusText;
    try {
      const parsed = (await resp.json()) as { error?: string; message?: string };
      code = parsed.error ?? code;
      message = parsed.message ?? message;
    } catch {
      // non-JSON error body; keep the status text
    }
    throw new ApiError(resp.status, code, message);
  }

  if (resp.status === 204) return undefined as T;
  const text = await resp.text();
  return (text ? JSON.parse(text) : undefined) as T;
}
