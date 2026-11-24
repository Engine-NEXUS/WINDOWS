import { useMemo } from "react";
import { Pill, SvBar, SvIcon, SvScroll, SvView } from "../../kit";
import { groupMessages, incomingCount, initials, type WaPayload } from "./model";
import "./whatsapp.css";

/** WhatsApp messages: a chat thread. Read-only; reading here never marks the chat as read. */
export function WhatsAppView({ data, nowSecs }: { data: WaPayload; nowSecs?: number }) {
  const now = nowSecs ?? Math.floor(Date.now() / 1000);
  const groups = useMemo(() => groupMessages(data.messages, now), [data.messages, now]);
  const incoming = incomingCount(data.messages);
  const count = data.messages.length;

  return (
    <SvView className="view-whatsapp" label={`WhatsApp chat with ${data.name}`}>
      <SvBar icon="chat" title="WhatsApp" subtitle={`${count} ${count === 1 ? "message" : "messages"}`} />
      <SvScroll>
        <div className="wa-head">
          <div className="wa-avatar" aria-hidden="true">{initials(data.name)}</div>
          <div className="wa-who">
            <span className="wa-name">{data.name}</span>
            <span className="wa-tags">
              {incoming > 0 ? <Pill tone="solid">{incoming} from them</Pill> : <Pill>No new messages</Pill>}
              {data.vip ? <Pill tone="outline">VIP</Pill> : null}
              {data.muted ? <Pill tone="dash">Muted</Pill> : null}
            </span>
          </div>
        </div>
        {count === 0 ? (
          <div className="wa-empty">No recent messages in this chat.</div>
        ) : (
          <div className="wa-thread">
            {groups.map((g) => (
              <div key={g.key} style={{ display: "contents" }}>
                <div className="wa-day">{g.label}</div>
                {g.rows.map((r) => (
                  <div key={r.id} className={`wa-row ${r.fromMe ? "is-me" : "is-them"}${r.first ? " is-first" : ""}${r.last ? " is-last" : ""}`}>
                    <div className="wa-bubble">{r.text}</div>
                    {r.last && r.time ? <span className="wa-time">{r.time}</span> : null}
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}
        <div className="wa-foot">
          <SvIcon name="shield" />
          Read only. Reading does not mark the chat as read.
        </div>
      </SvScroll>
    </SvView>
  );
}
