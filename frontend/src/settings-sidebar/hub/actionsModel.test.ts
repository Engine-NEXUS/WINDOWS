import { describe, expect, it } from "vitest";
import {
  AGENT_APPROVALS,
  agentApprovalLabel,
  auditSummary,
  baseName,
  clampPolicy,
  humanBytes,
  modeLabel,
  purgeInDays,
  ruleLabel,
  trashLabel,
  validPattern,
  validRoot,
  type AuditRow,
  type Policy,
} from "./actionsModel";

const policy = (over: Partial<Policy> = {}): Policy => ({
  version: 1,
  enabled: true,
  mode: "smart",
  use_default_roots: true,
  extra_roots: [],
  protected_paths: [],
  rules: [],
  max_items: 50,
  max_folder_files: 200,
  quarantine_days: 30,
  audit_days: 90,
  agent_approval: "edit",
  ...over,
});

const audit = (over: Partial<AuditRow>): AuditRow => ({
  ts: 0,
  phrase: "",
  tool: "delete",
  risk: "destructive",
  origin: "user_voice",
  decision: "allow",
  outcome: "verified",
  detail: "",
  ...over,
});

describe("actionsModel", () => {
  it("accepts only real folders as extra roots, never a drive, share or empty text", () => {
    expect(validRoot("D:\\projects")).toBe(true);
    expect(validRoot("C:\\Users\\me\\Documents\\Taxes")).toBe(true);
    expect(validRoot("/home/me/work")).toBe(true);
    for (const bad of ["", " ", "C:", "C:\\", "D:\\", "/", "\\\\server\\share\\x", "projects", "..\\x"]) {
      expect(validRoot(bad), bad).toBe(false);
    }
  });

  it("validates rule patterns", () => {
    expect(validPattern("*.pdf")).toBe(true);
    expect(validPattern("  id_rsa* ")).toBe(true);
    expect(validPattern("")).toBe(false);
    expect(validPattern("***")).toBe(false);
    expect(validPattern("a".repeat(201))).toBe(false);
    expect(validPattern("bad\u0001name")).toBe(false);
  });

  it("describes rules and modes in plain words", () => {
    expect(ruleLabel({ effect: "deny", tool: "delete", pattern: "*.kdbx" })).toBe("Never: delete for *.kdbx");
    expect(ruleLabel({ effect: "ask", tool: "*", pattern: "*.exe" })).toBe("Always ask: any action on *.exe");
    expect(ruleLabel({ effect: "allow", tool: "create_folder", pattern: "*" })).toContain("create folder");
    expect(modeLabel("smart")).toMatch(/asks before deleting/);
    expect(modeLabel("never_destructive")).toMatch(/refused/);
  });

  it("clamps edits to the limits Rust enforces", () => {
    const c = clampPolicy(policy({ max_items: 9999, max_folder_files: -5, quarantine_days: 0, audit_days: 1 }));
    expect([c.max_items, c.max_folder_files, c.quarantine_days, c.audit_days]).toEqual([200, 1, 1, 7]);
    expect(clampPolicy(policy({ max_items: Number.NaN })).max_items).toBe(1);
    expect(clampPolicy(policy()).max_items).toBe(50);
  });

  it("labels trash items and counts the days left before the purge", () => {
    const t = { id: "1", original: "D:\\Docs\\report.pdf", stored: "x", is_dir: false, bytes: 2048, items: 1, at: 1000 };
    expect(trashLabel(t)).toBe("report.pdf (2.0 KB)");
    expect(trashLabel({ ...t, original: "D:\\Docs\\old", is_dir: true, items: 1 })).toBe("old/ (1 file)");
    expect(trashLabel({ ...t, original: "D:\\Docs\\old", is_dir: true, items: 12 })).toBe("old/ (12 files)");
    expect(purgeInDays(t, 1000 + 10 * 86400, 30)).toBe(20);
    expect(purgeInDays(t, 1000 + 99 * 86400, 30)).toBe(0);
    expect(humanBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(baseName("D:\\a\\b\\")).toBe("b");
  });

  it("offers no autonomous or bypass choice for coding agents started by voice", () => {
    expect(AGENT_APPROVALS).toEqual(["edit", "readonly", "standard", "permissive"]);
    for (const a of AGENT_APPROVALS) {
      expect(a).not.toMatch(/autonomous|yolo|bypass|skip|full/);
      expect(agentApprovalLabel(a).length).toBeGreaterThan(10);
    }
    expect(agentApprovalLabel("edit")).toMatch(/cannot run commands/);
  });

  it("summarises audit lines without hiding refusals", () => {
    expect(auditSummary(audit({ decision: "deny", outcome: "blocked", detail: "outside my folders" }))).toBe("Refused delete: outside my folders");
    expect(auditSummary(audit({ decision: "declined", outcome: "blocked" }))).toBe("You declined delete");
    expect(auditSummary(audit({ tool: "create_folder", detail: "taxes" }))).toBe("Did create folder: taxes");
    expect(auditSummary(audit({ decision: "ask", outcome: "planned" }))).toBe("Asked before delete");
  });
});
