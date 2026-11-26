import { useMemo } from "react";
import { SvBar, SvScroll, SvView } from "../../kit";
import { averageBefore, formatMinutes, today, topRows, versusAverage, weekBars, type StPayload } from "./model";
import "./screentime.css";

function Rows({ rows }: { rows: { name: string; minutes: number; pct: number }[] }) {
  return (
    <div className="st-rows">
      {rows.map((r) => (
        <div key={r.name}>
          <div className="st-row-top"><span>{r.name}</span><span>{formatMinutes(r.minutes)}</span></div>
          <div className="st-track"><div className="st-fill" style={{ width: `${r.pct}%` }} /></div>
        </div>
      ))}
    </div>
  );
}

/** Screen time: a ledger of the day. Local, encrypted history only; nothing is shared. */
export function ScreenTimeView({ data }: { data: StPayload }) {
  const bars = useMemo(() => weekBars(data.days), [data.days]);
  const now = useMemo(() => today(data.days), [data.days]);
  const apps = useMemo(() => topRows((now?.apps ?? []).map((a) => ({ name: a.app, minutes: a.minutes }))), [now]);
  const sites = useMemo(() => topRows(data.sites.map((s) => ({ name: s.domain, minutes: s.minutes }))), [data.sites]);
  const vs = versusAverage(now?.minutes ?? 0, averageBefore(data.days));

  return (
    <SvView className="view-screentime" label="Screen time">
      <SvBar icon="monitor" title="Screen time" subtitle="This laptop" />
      <SvScroll>
        {!data.enabled ? (
          <div className="st-off">
            <b>Activity history is off</b>
            Turn on the Activity Manager in the Command Hub to see how long you use your apps. It stays encrypted on this laptop.
          </div>
        ) : (
          <>
            <div className="st-hero">
              <span className="st-cap">Today</span>
              <span className="st-big">{formatMinutes(now?.minutes ?? 0)}</span>
              {vs ? <span className="st-vs">{vs}</span> : null}
            </div>
            {bars.length > 0 ? (
              <div className="st-week" role="img" aria-label="Active time over the last days">
                {bars.map((b) => (
                  <div key={b.day} className={`st-col${b.today ? " is-today" : ""}`} title={formatMinutes(b.minutes)}>
                    <div className="st-bar-wrap"><div className="st-bar" style={{ height: `${Math.max(b.pct, 4)}%` }} /></div>
                    <span className="st-day">{b.label}</span>
                  </div>
                ))}
              </div>
            ) : null}
            <section className="st-section">
              <span className="st-cap">Top apps today</span>
              {apps.length > 0 ? <Rows rows={apps} /> : <div className="st-vs">No app time recorded yet today.</div>}
            </section>
            {data.sitesEnabled ? (
              <section className="st-section">
                <span className="st-cap">Top sites today</span>
                {sites.length > 0 ? <Rows rows={sites} /> : <div className="st-vs">No site time recorded yet today.</div>}
              </section>
            ) : null}
          </>
        )}
        <div className="st-foot">Stored encrypted on this laptop. Nothing is shared.</div>
      </SvScroll>
    </SvView>
  );
}
