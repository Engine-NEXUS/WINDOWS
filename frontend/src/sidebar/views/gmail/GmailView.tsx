import { useMemo } from "react";
import { Pill, SvBar, SvScroll, SvView } from "../../kit";
import { deadlineCount, groupMail, hasDeadline, senderName, summaryLine, type MailPayload } from "./model";
import "./gmail.css";

/** Gmail review: the important mail NEXUS filed, grouped by topic, deadlines first. Read only. */
export function GmailView({ data }: { data: MailPayload }) {
  const groups = useMemo(() => groupMail(data.items), [data.items]);
  const total = data.items.length;
  const due = deadlineCount(data.items);

  return (
    <SvView className="view-gmail" label="Important email">
      <SvBar icon="file" title="Gmail" subtitle="Important mail" />
      <SvScroll>
        {total === 0 ? (
          <div className="gm-empty">Nothing important in your inbox right now.</div>
        ) : (
          <>
            <div className="gm-top">
              <div className="gm-stat"><b>{total}</b><span>{total === 1 ? "important email" : "important emails"}</span></div>
              <div className="gm-stat"><b>{due}</b><span>{due === 1 ? "has a deadline" : "have deadlines"}</span></div>
            </div>
            {groups.map((g) => (
              <section key={g.key} className="gm-group">
                <div className="gm-group-hd">
                  <span>{g.label}</span>
                  <span>{g.items.length}</span>
                </div>
                {g.items.map((m, i) => {
                  const explain = summaryLine(m);
                  return (
                    <article key={`${g.key}-${i}`} className={`gm-mail${hasDeadline(m) ? " has-deadline" : ""}`}>
                      <span className="gm-from">{senderName(m.sender)}</span>
                      <span className="gm-subject">{m.subject}</span>
                      {explain ? <span className="gm-explain">{explain}</span> : null}
                      {hasDeadline(m) ? (
                        <span className="gm-due">
                          <Pill tone="solid" icon="alert">{m.deadline}</Pill>
                        </span>
                      ) : null}
                    </article>
                  );
                })}
              </section>
            ))}
          </>
        )}
        <div className="gm-foot">Read only. Nothing is sent, archived or opened.</div>
      </SvScroll>
    </SvView>
  );
}
