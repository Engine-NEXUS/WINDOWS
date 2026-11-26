/**
 * NEXUS Confirmation Panel — interactive action confirmation card
 * displayed inside the Response Sidebar.
 *
 * Supports:
 * - WhatsApp message confirmations (recipient, message content preview)
 * - Swiggy / MCP actions (server, tool, parameter summaries)
 * - GitHub destructive operations (PR #, repo, destructive action badge)
 * - 1-click Confirm and Cancel buttons with loading feedback
 * - Voice assistance hint ("Say 'Yes' or 'Cancel'")
 */

import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ConfirmationData, useSidebar } from "./sidebarStore";
import { ActionBar, Pill, SvButton, SvCard, SvChip, SvIcon } from "./kit/system";

interface ConfirmationPanelProps {
  data: ConfirmationData;
  onClose?: () => void;
}

export function ConfirmationPanel({ data, onClose }: ConfirmationPanelProps) {
  const [submitting, setSubmitting] = useState(false);
  const [actionStatus, setActionStatus] = useState<string | null>(null);

  const cmd = data.command || {};
  const isMcp = cmd.kind === "mcp";
  const server = cmd.server || "";
  const tool = cmd.tool || "";
  const params = cmd.params || {};

  const isWhatsApp = isMcp && server === "whatsapp";
  const isSwiggy = isMcp && typeof server === "string" && server.startsWith("swiggy");
  const isAmazon = isMcp && server === "amazon";
  const isGitHub = !isMcp && (cmd.type || cmd.action || cmd.repo || cmd.pr_number !== undefined);

  // Handle Confirm Click
  const handleConfirm = useCallback(async () => {
    if (submitting) return;
    setSubmitting(true);
    setActionStatus("Executing...");

    try {
      if (isMcp) {
        console.log("[ConfirmationPanel] Confirming MCP action:", cmd);
        await invoke("orchestrator_mcp_confirm", {
          requestId: data.requestId || "mcp-confirm",
          confirmed: true,
          pending: cmd,
        });
      } else {
        console.log("[ConfirmationPanel] Confirming GitHub action:", cmd);
        await invoke("orchestrator_github_execute", {
          command: cmd,
          confirmed: true,
        });
      }
      useSidebar.getState().hide();
      onClose?.();
    } catch (err) {
      console.error("[ConfirmationPanel] Confirm failed:", err);
      setActionStatus("Execution failed");
      setSubmitting(false);
    }
  }, [submitting, isMcp, cmd, data.requestId, onClose]);

  // Handle Cancel Click
  const handleCancel = useCallback(async () => {
    if (submitting) return;
    setSubmitting(true);
    setActionStatus("Cancelling...");

    try {
      if (isMcp) {
        console.log("[ConfirmationPanel] Cancelling MCP action:", cmd);
        await invoke("orchestrator_mcp_confirm", {
          requestId: data.requestId || "mcp-cancel",
          confirmed: false,
          pending: cmd,
        });
      } else {
        await invoke("hide_sidebar").catch(() => {});
      }
      useSidebar.getState().hide();
      onClose?.();
    } catch (err) {
      console.warn("[ConfirmationPanel] Cancel failed:", err);
      useSidebar.getState().hide();
      onClose?.();
    }
  }, [submitting, isMcp, cmd, data.requestId, onClose]);

  // Determine button label
  let confirmLabel = "Confirm";
  if (isWhatsApp) confirmLabel = "Send message";
  else if (isSwiggy) confirmLabel = "Confirm order";
  else if (isGitHub) confirmLabel = "Confirm action";

  const appName = isWhatsApp ? "WhatsApp" : isSwiggy ? "Swiggy" : isAmazon ? "Amazon" : isGitHub ? "GitHub" : "Action";
  const headline = isWhatsApp
    ? `Send this message to ${params.recipient || "this contact"}?`
    : isSwiggy
    ? "Place this Swiggy request?"
    : isGitHub
    ? "Run this GitHub action?"
    : "Confirm this action?";

  return (
    <>
      <SvCard>
        <div className="sv-row" style={{ marginBottom: 12 }}>
          <Pill tone="solid" icon={isWhatsApp ? "chat" : isGitHub ? "pr" : "shield"}>{appName}</Pill>
          <Pill tone="outline" icon="shield">Approval required</Pill>
        </div>
        <h2 className="sv-h">{headline}</h2>
        {data.prompt ? <p className="sv-p" style={{ margin: "10px 0 14px" }}>{data.prompt}</p> : null}

        {isWhatsApp && (
          <>
            <div className="sv-bubble">{params.message || "(empty message)"}</div>
            <div className="sv-row" style={{ marginTop: 14 }}>
              <Pill icon="chat">To: {params.recipient || "Unknown contact"}</Pill>
            </div>
          </>
        )}

        {isGitHub && (
          <div className="sv-row" style={{ marginTop: 6 }}>
            {cmd.repo && <SvChip icon="repo">{cmd.repo}</SvChip>}
            {cmd.pr_number !== undefined && <SvChip icon="pr">#{cmd.pr_number}</SvChip>}
            {cmd.branch && <SvChip>{cmd.branch}</SvChip>}
          </div>
        )}

        {isSwiggy && (
          <div className="sv-row" style={{ marginTop: 6 }}>
            <Pill>{tool.replace(/_/g, " ")}</Pill>
          </div>
        )}

        {!isWhatsApp && !isGitHub && !isSwiggy && (
          <div className="sv-row" style={{ marginTop: 6 }}>
            <Pill>{server ? `${server} / ${tool}` : "System operation"}</Pill>
          </div>
        )}

        {actionStatus && (
          <p className="sv-p" role="status" style={{ marginTop: 14 }}>
            {actionStatus}
          </p>
        )}
        <p className="sv-p" style={{ marginTop: 14, display: "flex", gap: 6, alignItems: "center" }}>
          <span style={{ width: 13, height: 13, display: "inline-flex" }}><SvIcon name="mic" /></span>
          <span>
            Say <strong>“yes”</strong> or <strong>“cancel”</strong>, or use the buttons below
          </span>
        </p>
      </SvCard>

      <ActionBar sticky>
        <SvButton onClick={handleCancel} disabled={submitting}>
          <SvIcon name="x" />
          Cancel
        </SvButton>
        <SvButton primary onClick={handleConfirm} disabled={submitting}>
          <SvIcon name="check" />
          {submitting ? "Working…" : confirmLabel}
        </SvButton>
      </ActionBar>
    </>
  );
}// NOTE: default export removed (audit M5) — SidebarApp uses the named import.
