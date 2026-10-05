import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  SECTION_META,
  addOutcomeMessage,
  emptyOverview,
  isValidFactInput,
  normalizeOverview,
  totalProfileEntries,
  type HabitSuggestion,
  type MemoryOverview,
  type ProfileSection,
} from "./memoryModel";

/**
 * Memory tab — everything NEXUS remembers about you, in one place you control:
 * your about-me profile (who you are / what you can do / what you need / your
 * priorities), remembered facts, what it has learned from how you use it
 * (with suggestions you confirm), and the stored conversation.
 *
 * Everything shown here is a local file. Entries flagged 🔒 contain private
 * details (phone, e-mail, card…) and are never sent to a cloud model.
 */
export function MemoryTab() {
  const [data, setData] = useState<MemoryOverview>(emptyOverview());
  const [loaded, setLoaded] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [factKey, setFactKey] = useState("");
  const [factValue, setFactValue] = useState("");
  const [confirmAll, setConfirmAll] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setData(normalizeOverview(await invoke("memory_overview")));
    } catch {
      /* outside Tauri / command unavailable — keep the empty view */
    }
    setLoaded(true);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!note) return;
    const t = setTimeout(() => setNote(null), 4000);
    return () => clearTimeout(t);
  }, [note]);

  const addEntry = async (section: ProfileSection, text: string) => {
    if (!text.trim()) return;
    try {
      const outcome = await invoke<string>("memory_profile_add", { section, text });
      setNote(addOutcomeMessage(outcome));
      setDrafts((d) => ({ ...d, [section]: "" }));
    } catch {
      setNote(addOutcomeMessage("error"));
    }
    await refresh();
  };

  const removeEntry = async (section: ProfileSection, index: number) => {
    await invoke("memory_profile_remove", { section, index }).catch(() => {});
    await refresh();
  };

  const acceptSuggestion = async (s: HabitSuggestion) => {
    await addEntry("priorities", s.text);
    await invoke("memory_habit_dismiss", { key: s.key }).catch(() => {});
    await refresh();
  };

  const dismissSuggestion = async (s: HabitSuggestion) => {
    await invoke("memory_habit_dismiss", { key: s.key }).catch(() => {});
    await refresh();
  };

  const addFact = async () => {
    if (!isValidFactInput(factKey, factValue)) return;
    try {
      await invoke("memory_fact_set", { key: factKey, value: factValue });
      setFactKey("");
      setFactValue("");
      setNote("Saved.");
    } catch {
      setNote("Couldn't save that.");
    }
    await refresh();
  };

  const removeFact = async (key: string) => {
    await invoke("memory_forget", { key }).catch(() => {});
    await refresh();
  };

  const clear = async (scope: string, done: string) => {
    try {
      await invoke("memory_clear", { scope });
      setNote(done);
    } catch {
      setNote("Couldn't clear everything — try again.");
    }
    setConfirmAll(false);
    await refresh();
  };

  return (
    <div className="settings-section">
      <div className="settings-section-title">About you</div>
      <div className="setting-desc">
        NEXUS keeps this on your PC and uses it to personalise answers. You can also just say it:
        “my priority is …”, “remember that I am …”, “what do you know about me”.
      </div>
      {note && <div className="memory-note">{note}</div>}

      {SECTION_META.map((sec) => (
        <div key={sec.id} className="memory-block">
          <div className="memory-sub">
            {sec.title} <span className="memory-hint">{sec.hint}</span>
          </div>
          <ul className="memory-list">
            {data.profile[sec.id].length === 0 && loaded && <li className="memory-empty">Nothing yet.</li>}
            {data.profile[sec.id].map((e, i) => (
              <li key={`${e.ts}-${i}`} className="memory-item">
                <span className="memory-text">
                  {e.sensitive && <span title="Private — stays on this PC" className="memory-lock">🔒 </span>}
                  {e.text}
                </span>
                <button
                  type="button"
                  className="memory-x"
                  title="Forget this"
                  onClick={() => void removeEntry(sec.id, i)}
                >
                  ✕
                </button>
              </li>
            ))}
          </ul>
          <div className="memory-add">
            <input
              className="settings-input"
              placeholder={sec.placeholder}
              value={drafts[sec.id] ?? ""}
              onChange={(ev) => setDrafts((d) => ({ ...d, [sec.id]: ev.target.value }))}
              onKeyDown={(ev) => {
                if (ev.key === "Enter") void addEntry(sec.id, drafts[sec.id] ?? "");
              }}
            />
            <button
              type="button"
              className="settings-btn"
              disabled={!(drafts[sec.id] ?? "").trim()}
              onClick={() => void addEntry(sec.id, drafts[sec.id] ?? "")}
            >
              Add
            </button>
          </div>
        </div>
      ))}

      {data.habits.suggestions.length > 0 && (
        <>
          <div className="settings-section-title">Learned from how you use NEXUS</div>
          <div className="setting-desc">
            Suggestions only — nothing is added to your profile unless you accept it.
          </div>
          <ul className="memory-list">
            {data.habits.suggestions.map((s) => (
              <li key={s.key} className="memory-item memory-item--suggest">
                <span className="memory-text">
                  <strong>{s.text}</strong>
                  <span className="memory-hint"> — {s.reason}</span>
                </span>
                <span className="memory-actions">
                  <button type="button" className="settings-btn" onClick={() => void acceptSuggestion(s)}>
                    Add as priority
                  </button>
                  <button type="button" className="settings-btn" onClick={() => void dismissSuggestion(s)}>
                    Dismiss
                  </button>
                </span>
              </li>
            ))}
          </ul>
        </>
      )}

      {data.habits.top.length > 0 && (
        <div className="setting-desc">
          Most used: {data.habits.top.map((t) => `${t.name} (${t.count})`).join(" · ")}
        </div>
      )}

      <div className="settings-section-title">Remembered facts</div>
      <ul className="memory-list">
        {data.facts.length === 0 && loaded && <li className="memory-empty">Nothing yet.</li>}
        {data.facts.map((f) => (
          <li key={f.key} className="memory-item">
            <span className="memory-text">
              {f.sensitive && <span title="Private — stays on this PC" className="memory-lock">🔒 </span>}
              <strong>{f.key.replace(/_/g, " ")}</strong>: {f.value}
            </span>
            <button type="button" className="memory-x" title="Forget this" onClick={() => void removeFact(f.key)}>
              ✕
            </button>
          </li>
        ))}
      </ul>
      <div className="memory-add">
        <input
          className="settings-input"
          placeholder="name (e.g. favorite color)"
          value={factKey}
          onChange={(e) => setFactKey(e.target.value)}
        />
        <input
          className="settings-input"
          placeholder="value (e.g. blue)"
          value={factValue}
          onChange={(e) => setFactValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void addFact();
          }}
        />
        <button
          type="button"
          className="settings-btn"
          disabled={!isValidFactInput(factKey, factValue)}
          onClick={() => void addFact()}
        >
          Add
        </button>
      </div>

      <div className="settings-section-title">Privacy</div>
      <div className="setting-row">
        <div>
          <div className="setting-label">Conversation history</div>
          <div className="setting-desc">{data.conversationTurns} stored turn{data.conversationTurns === 1 ? "" : "s"}</div>
        </div>
        <button
          type="button"
          className="settings-btn"
          onClick={() => void clear("conversation", "Conversation history cleared.")}
        >
          Clear
        </button>
      </div>
      <div className="setting-row">
        <div>
          <div className="setting-label">Forget everything</div>
          <div className="setting-desc">
            Profile ({totalProfileEntries(data)}), facts ({data.facts.length}), learned habits and conversation.
          </div>
        </div>
        {confirmAll ? (
          <span className="memory-actions">
            <button type="button" className="settings-btn settings-btn--primary" onClick={() => void clear("all", "Everything forgotten.")}>
              Yes, forget it all
            </button>
            <button type="button" className="settings-btn" onClick={() => setConfirmAll(false)}>
              Cancel
            </button>
          </span>
        ) : (
          <button type="button" className="settings-btn" onClick={() => setConfirmAll(true)}>
            Forget everything…
          </button>
        )}
      </div>
    </div>
  );
}
