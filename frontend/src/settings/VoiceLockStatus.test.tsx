import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { VoiceLockStatus, VerifiedSaveStatus } from "./VoiceLockStatus";
import { VoiceLockState } from "./voiceLockState";
describe("Voice Lock readback display", () => {
  it("never presents an unsaved On draft as effective On", () => {
    const model = new VoiceLockState(async () => false);
    model.state.effective = false;
    const html = renderToStaticMarkup(<VoiceLockStatus model={model} draft={true} />);
    expect(html).toContain("Effective Voice Lock: Off"); expect(html).toContain("Click Save");
  });
  it("shows unknown status and persistent read errors", () => {
    const model = new VoiceLockState(async () => false); model.state.error = "Readback failed";
    const html = renderToStaticMarkup(<VoiceLockStatus model={model} draft={false} />);
    expect(html).toContain("Unknown"); expect(html).toContain('role="alert"'); expect(html).toContain("Readback failed");
  });
  it("warns that missing profile does not unlock actions", () => {
    const model = new VoiceLockState(async () => true); model.state.effective = true;
    expect(renderToStaticMarkup(<VoiceLockStatus model={model} draft={true} />)).toContain("Voice actions remain blocked");
  });
});

describe("accessible verified Save feedback", () => {
  function render(saved: boolean, effective: boolean | null, draft: boolean, error: string | null = null, saving = false) {
    const model = new VoiceLockState(async () => false);
    model.state = { ...model.state, effective, error, saving, saveVerified: saved };
    return renderToStaticMarkup(<VerifiedSaveStatus model={model} saved={saved} draft={draft} />);
  }
  it("contains no Saved announcement before a Save", () => {
    expect(render(false, true, true)).not.toContain("✓ Saved");
    expect(render(false, true, true)).toContain('aria-hidden="true"');
  });
  it("contains no Saved announcement for an unverified draft", () => { expect(render(true, true, false)).not.toContain("✓ Saved"); });
  it("contains no Saved announcement while status is unknown", () => { expect(render(true, null, false)).not.toContain("✓ Saved"); });
  it("contains no Saved announcement after failure or during Save", () => {
    expect(render(true, true, true, "failed")).not.toContain("✓ Saved");
    expect(render(true, true, true, null, true)).not.toContain("✓ Saved");
  });
  it("announces Saved only after matching verified state", () => {
    const html = render(true, false, false);
    expect(html).toContain("✓ Saved"); expect(html).toContain('role="status"'); expect(html).toContain('aria-hidden="false"');
  });
});
