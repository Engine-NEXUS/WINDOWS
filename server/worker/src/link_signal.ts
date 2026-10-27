/** A hibernating signaling room for one approved pair of devices. */
export class LinkSignalRoom {
  constructor(private readonly state: DurableObjectState) {}

  private async charge(
    deviceId: string, kind: "message" | "connection",
    minuteLimit: number, dayLimit: number,
  ): Promise<"ok" | "limit" | "unavailable"> {
    const minute = Math.floor(Date.now() / 60_000);
    const day = Math.floor(Date.now() / 86_400_000);
    const key = `quota:${kind}:${deviceId}`;
    try {
      return await this.state.storage.transaction(async txn => {
        const quota = await txn.get<{
          minute: number; minuteCount: number; day: number; dayCount: number;
        }>(key) || { minute: 0, minuteCount: 0, day: 0, dayCount: 0 };
        const minuteCount = quota.minute === minute ? quota.minuteCount + 1 : 1;
        const dayCount = quota.day === day ? quota.dayCount + 1 : 1;
        if (minuteCount > minuteLimit || dayCount > dayLimit) return "limit";
        await txn.put(key, { minute, minuteCount, day, dayCount });
        return "ok";
      });
    } catch {
      return "unavailable";
    }
  }

  private deviceOf(socket: WebSocket): string {
    const value = socket.deserializeAttachment() as { deviceId?: string } | null;
    return value?.deviceId || "";
  }

  async fetch(request: Request): Promise<Response> {
    if (request.method === "POST" && request.headers.get("X-Nexus-Revoke") === "1") {
      for (const socket of this.state.getWebSockets()) socket.close(4003, "link revoked");
      return new Response(null, { status: 204 });
    }
    const deviceId = request.headers.get("X-Nexus-Signal-Device") || "";
    if (!deviceId || request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("upgrade required", { status: 426 });
    }
    const connection = await this.charge(deviceId, "connection", 10, 30);
    if (connection !== "ok") {
      return new Response(connection === "limit" ? "signaling rate limit" : "signaling quota unavailable",
        { status: connection === "limit" ? 429 : 503 });
    }
    const pair = new WebSocketPair();
    const client = pair[0];
    const server = pair[1];
    for (const old of this.state.getWebSockets()) {
      if (this.deviceOf(old) === deviceId) old.close(4000, "reconnected");
    }
    this.state.acceptWebSocket(server);
    server.serializeAttachment({ deviceId });
    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(sender: WebSocket, message: string | ArrayBuffer): Promise<void> {
    if (typeof message !== "string" || message.length > 16_384) {
      sender.close(1009, "signaling frame too large");
      return;
    }
    let frame: Record<string, unknown>;
    try { frame = JSON.parse(message) as Record<string, unknown>; }
    catch { sender.close(1003, "invalid signaling frame"); return; }
    // Activity frames stay disabled until both the local opt-in switch and
    // link-scoped sharing policy exist. Signaling must not activate sharing.
    if (!frame || !["offer", "answer", "ice", "presence"].includes(String(frame.type))) {
      sender.close(1003, "unsupported signaling frame");
      return;
    }
    const senderId = this.deviceOf(sender);
    if (!senderId) { sender.close(1008, "unidentified device"); return; }
    // Durable Object storage survives reconnects and hibernation. The
    // transaction keeps concurrent frames from racing the same allowance.
    const charge = await this.charge(senderId, "message", 60, 300);
    if (charge === "limit") {
      sender.close(1008, "signaling rate limit");
      return;
    }
    if (charge === "unavailable") {
      sender.close(1013, "signaling quota unavailable");
      return;
    }
    if (["offer", "answer", "ice"].includes(String(frame.type))) {
      const max = frame.type === "ice" ? 4096 : 12_000;
      if (typeof frame.sdp !== "string" || frame.sdp.length > max ||
          Object.keys(frame).some(k => !["type", "sdp"].includes(k))) {
        sender.close(1003, "invalid negotiation frame");
        return;
      }
    }
    if (frame.type === "presence" &&
        (typeof frame.active !== "boolean" || Object.keys(frame).some(k => !["type", "active"].includes(k)))) {
      sender.close(1003, "invalid presence frame");
      return;
    }
    // File bytes and activity reports never traverse this signaling room.
    for (const peer of this.state.getWebSockets()) {
      if (peer !== sender && this.deviceOf(peer) !== senderId) peer.send(message);
    }
  }
}
