// Run on the Administrator's spare Android phone with Termux + Termux:API.
// Requires NEXUS_WORKER_URL and NEXUS_SMS_GATEWAY_TOKEN in the environment.
// The carrier may charge for SMS; this bridge does not use a paid cloud API.
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const run = promisify(execFile);
const origin = process.env.NEXUS_WORKER_URL?.replace(/\/$/, "");
const token = process.env.NEXUS_SMS_GATEWAY_TOKEN;
if (!origin?.startsWith("https://") || !token || token.length < 24) {
  throw new Error("Set an HTTPS NEXUS_WORKER_URL and a long NEXUS_SMS_GATEWAY_TOKEN");
}

async function post(path, body = {}) {
  const response = await fetch(`${origin}${path}`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(10_000),
  });
  if (!response.ok) throw new Error(`${path} returned ${response.status}`);
  return response.json();
}

let lastHeartbeat = 0;
for (;;) {
  try {
    if (Date.now() - lastHeartbeat > 4 * 60_000) {
      await post("/v1/links/sms/heartbeat");
      lastHeartbeat = Date.now();
    }
    const job = await post("/v1/links/sms/next");
    if (job.pending === true && /^[0-9]{6}$/.test(job.code) && /^\+[1-9][0-9]{7,14}$/.test(job.phone)) {
      await run("termux-sms-send", ["-n", job.phone, `Your NEXUS device verification code is ${job.code}. It expires in 10 minutes.`],
        { timeout: 15_000 });
      await post("/v1/links/sms/sent", { challenge_id: job.challenge_id });
    }
  } catch (error) {
    // No number or code is printed to logs. Failed jobs are available again
    // after their short claim lease and expire after ten minutes.
    console.error("SMS gateway unavailable:", error instanceof Error ? error.message : String(error));
  }
  await new Promise(resolve => setTimeout(resolve, 30_000));
}
