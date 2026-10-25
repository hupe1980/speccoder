// The host's API. The token arrives once, in the address's fragment (never
// sent to any server), is kept for this tab only, and is removed from the
// address bar. Every request carries it as a bearer header.

const KEY = "speccoder-token";

function readToken(): string {
  const m = location.hash.match(/token=([0-9a-f]{64})/);
  if (m?.[1]) {
    sessionStorage.setItem(KEY, m[1]);
    history.replaceState(null, "", location.pathname + location.search);
    return m[1];
  }
  return sessionStorage.getItem(KEY) ?? "";
}

export const token = readToken();

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) throw new ApiError(res.status, data?.error ?? res.statusText);
  return data as T;
}

export const get = <T>(path: string) => call<T>("GET", path);
export const post = <T>(path: string, body: unknown) => call<T>("POST", path, body);

export function q(params: Record<string, string | undefined | null>): string {
  const s = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v) s.set(k, v);
  const t = s.toString();
  return t ? `?${t}` : "";
}

/** Reads the host's server-sent events with fetch (a header can carry the
 *  token; EventSource cannot), reconnecting after a drop. */
export function listen(onEvent: (e: any) => void, onState: (connected: boolean) => void): () => void {
  let stopped = false;
  let controller: AbortController | null = null;
  const run = async () => {
    while (!stopped) {
      controller = new AbortController();
      try {
        const res = await fetch("/api/events", {
          headers: { Authorization: `Bearer ${token}` },
          signal: controller.signal,
        });
        if (!res.ok || !res.body) throw new Error(String(res.status));
        onState(true);
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          let i: number;
          while ((i = buffer.indexOf("\n\n")) >= 0) {
            const chunk = buffer.slice(0, i);
            buffer = buffer.slice(i + 2);
            const data = chunk
              .split("\n")
              .filter((l) => l.startsWith("data:"))
              .map((l) => l.slice(5).trimStart())
              .join("\n");
            if (data) {
              try {
                onEvent(JSON.parse(data));
              } catch {
                /* a keep-alive or a partial line */
              }
            }
          }
        }
      } catch {
        /* dropped: reconnect */
      }
      onState(false);
      if (!stopped) await new Promise((r) => setTimeout(r, 1500));
    }
  };
  void run();
  return () => {
    stopped = true;
    controller?.abort();
  };
}
