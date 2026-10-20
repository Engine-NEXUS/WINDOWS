import { describe, expect, it } from "vitest";
import {
  agentApprovalLabel,
  auditSummary,
  baseName,
  clampPolicy,
  effectLabel,
  fmtWhen,
  humanBytes,
  modeLabel,
  purgeInDays,
  ruleLabel,
  trashLabel,
  validPattern,
  validRoot,
  type AuditRow,
  type Policy,
  type TrashRow,
} from "./actionsModel";

describe("validPattern boundaries", () => {
  it("rejects empty, whitespace-only, and overlong patterns", () => {
    expect(validPattern("")).toBe(false);
    expect(validPattern("   ")).toBe(false);
    expect(validPattern("a".repeat(200))).toBe(true); // exactly 200 is allowed
    expect(validPattern("a".repeat(201))).toBe(false); // over 200 rejected
  });

  it("requires at least one alphanumeric character", () => {
    expect(validPattern("*.txt")).toBe(true);
    expect(validPattern("*.*")).toBe(false);
    expect(validPattern("---")).toBe(false);
    expect(validPattern("???")).toBe(false);
  });

  it("rejects patterns containing control characters", () => {
    expect(validPattern("file\u0000.txt")).toBe(false);
    expect(validPattern("file\u001f.txt")).toBe(false);
    expect(validPattern("file\t.txt")).toBe(false);
    expect(validPattern("file\n.txt")).toBe(false);
  });
});

describe("validRoot boundaries", () => {
  it("rejects bare drive roots and POSIX root", () => {
    expect(validRoot("C:")).toBe(false);
    expect(validRoot("C:\\")).toBe(false);
    expect(validRoot("D:")).toBe(false);
    expect(validRoot("D:\\")).toBe(false);
    expect(validRoot("/")).toBe(false);
    expect(validRoot("")).toBe(false);
    expect(validRoot("   ")).toBe(false);
  });

  it("rejects network shares", () => {
    expect(validRoot("\\\\nas\\share")).toBe(false);
    expect(validRoot("\\\\192.168.1.1\\storage")).toBe(false);
  });

  it("accepts nested Windows and POSIX directories", () => {
    expect(validRoot("C:\\Users\\alice\\Documents")).toBe(true);
    expect(validRoot("D:\\code\\repo")).toBe(true);
    expect(validRoot("/home/user/project")).toBe(true);
    expect(validRoot("/etc/nginx")).toBe(true);
  });
});

describe("humanBytes, baseName, and trashLabel boundaries", () => {
  it("formats byte values across boundaries (B, KB, MB, GB)", () => {
    expect(humanBytes(0)).toBe("0 B");
    expect(humanBytes(500)).toBe("500 B");
    expect(humanBytes(1023)).toBe("1023 B");
    expect(humanBytes(1024)).toBe("1.0 KB");
    expect(humanBytes(1.5 * 1024 * 1024)).toBe("1.5 MB");
    expect(humanBytes(2.5 * 1024 * 1024 * 1024)).toBe("2.5 GB");
  });

  it("extracts baseName from Windows and POSIX paths with and without trailing slashes", () => {
    expect(baseName("C:\\a\\b\\file.txt")).toBe("file.txt");
    expect(baseName("C:\\a\\b\\folder\\")).toBe("folder");
    expect(baseName("/usr/local/bin/node")).toBe("node");
    expect(baseName("/usr/local/bin/")).toBe("bin");
    expect(baseName("isolated_filename")).toBe("isolated_filename");
  });

  it("formats trash row labels for files and directories (singular and plural)", () => {
    const fileRow: TrashRow = {
      id: "t1",
      original: "C:\\Docs\\notes.txt",
      stored: "s1",
      is_dir: false,
      bytes: 2048,
      items: 1,
      at: 1000,
    };
    expect(trashLabel(fileRow)).toBe("notes.txt (2.0 KB)");

    const singleDirRow: TrashRow = {
      ...fileRow,
      original: "C:\\Docs\\archive",
      is_dir: true,
      items: 1,
    };
    expect(trashLabel(singleDirRow)).toBe("archive/ (1 file)");

    const multiDirRow: TrashRow = {
      ...fileRow,
      original: "C:\\Docs\\archive",
      is_dir: true,
      items: 7,
    };
    expect(trashLabel(multiDirRow)).toBe("archive/ (7 files)");
  });
});

describe("purgeInDays and clampPolicy boundaries", () => {
  it("computes remaining quarantine days, clamping at zero", () => {
    const row: TrashRow = {
      id: "1",
      original: "file",
      stored: "s",
      is_dir: false,
      bytes: 10,
      items: 1,
      at: 1_000_000,
    };
    // exactly at time of deletion
    expect(purgeInDays(row, 1_000_000, 30)).toBe(30);

    // halfway through
    expect(purgeInDays(row, 1_000_000 + 15 * 86400, 30)).toBe(15);

    // expired
    expect(purgeInDays(row, 1_000_000 + 35 * 86400, 30)).toBe(0);
  });

  it("clamps policy limits strictly within rust-enforced bounds", () => {
    const base: Policy = {
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
    };

    const clamped = clampPolicy({
      ...base,
      max_items: -50,
      max_folder_files: 1_000_000,
      quarantine_days: 500,
      audit_days: 2,
    });

    expect(clamped.max_items).toBe(1);
    expect(clamped.max_folder_files).toBe(100000);
    expect(clamped.quarantine_days).toBe(365);
    expect(clamped.audit_days).toBe(7);
  });
});

describe("auditSummary, fmtWhen, ruleLabel, and modeLabel", () => {
  it("formats audit summary rows", () => {
    const row: AuditRow = {
      ts: 0,
      phrase: "",
      tool: "move",
      risk: "safe",
      origin: "voice",
      decision: "deny",
      outcome: "blocked",
      detail: "forbidden directory",
    };
    expect(auditSummary(row)).toBe("Refused move: forbidden directory");

    expect(auditSummary({ ...row, decision: "declined" })).toBe("You declined move");
    expect(auditSummary({ ...row, decision: "allow", outcome: "verified", detail: "done" })).toBe("Did move: done");
    expect(auditSummary({ ...row, decision: "ask", outcome: "planned", detail: "" })).toBe("Asked before move");
  });

  it("formats timestamp into DD/MM HH:MM format", () => {
    // 2026-01-02 03:04 UTC (local time will pad properly)
    const formatted = fmtWhen(1700000000);
    expect(formatted).toMatch(/^\d{2}\/\d{2} \d{2}:\d{2}$/);
  });

  it("formats labels for rules, modes, and approval tiers", () => {
    expect(ruleLabel({ effect: "allow", tool: "*", pattern: "*.md" })).toBe("Allow: any action on *.md");
    expect(ruleLabel({ effect: "deny", tool: "delete", pattern: "*.key" })).toBe("Never: delete for *.key");

    expect(effectLabel("deny")).toBe("Never");
    expect(effectLabel("ask")).toBe("Always ask");
    expect(effectLabel("allow")).toBe("Allow");

    expect(modeLabel("smart")).toContain("Smart");
    expect(modeLabel("ask")).toContain("Ask first");
    expect(modeLabel("never_destructive")).toContain("Never delete");

    expect(agentApprovalLabel("edit")).toContain("Edit files only");
    expect(agentApprovalLabel("readonly")).toContain("Read only");
    expect(agentApprovalLabel("standard")).toContain("Ask first");
    expect(agentApprovalLabel("permissive")).toContain("Edit freely");
  });
});
