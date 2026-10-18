import { invoke } from "@tauri-apps/api/core";

/**
 * Calls to the NEXUS Cloudflare Worker's authenticated `/oauth/*` routes.
 *
 * The Worker only answers an approved device: every request needs the device bearer token
 * plus the profile and device ids. Those live in Rust (the token is in the OS keyring), so
 * the web view never makes these requests itself — it asks the `worker_request` command,
 * which attaches the credentials and only talks to the configured Worker.
 *
 * Do not send `user_id`: the Worker derives the owner from the credentials (Rust strips it).
 */

export interface WorkerResponse<T = unknown> {
  status: number;
  body: T;
  /** Plain-language reason for a 401/403/503, ready to show or speak. */
  message: string | null;
}

/** The device is not linked to, or not allowed by, the NEXUS cloud service. */
export class WorkerAuthError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = "WorkerAuthError";
    this.status = status;
  }
}

export async function workerRequest<T = unknown>(
  method: "GET" | "POST" | "DELETE",
  path: string,
  body?: unknown,
): Promise<WorkerResponse<T>> {
  return invoke<WorkerResponse<T>>("worker_request", { method, path, body: body ?? null });
}

/** Like {@link workerRequest} but returns the body of a 2xx and throws otherwise. */
export async function workerJson<T = unknown>(
  method: "GET" | "POST" | "DELETE",
  path: string,
  body?: unknown,
): Promise<T> {
  const res = await workerRequest<T>(method, path, body);
  if (res.status >= 200 && res.status < 300) return res.body;
  if (res.message) throw new WorkerAuthError(res.status, res.message);
  const err = (res.body as { error?: unknown } | null)?.error;
  throw new Error(typeof err === "string" ? err : `Request to the NEXUS service failed (${res.status})`);
}
