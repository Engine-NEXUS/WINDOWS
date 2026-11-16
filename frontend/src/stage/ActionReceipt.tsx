import { useEffect, useRef, useState } from "react";
import { actionReceiptSlot } from "./layoutSlots";
import {
  formatReceiptHeadline,
  normalizeReceiptPayload,
  RECEIPT_AUTO_HIDE_MS,
  type ActionReceipt as ActionReceiptType,
} from "./actionReceiptModel";
import { receiptDisplayName, receiptLogoId } from "./receiptLogo";
import { ProviderLogo } from "../notify-toast/ProviderLogo";

interface OrbRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ActionReceiptCardProps {
  receipt: ActionReceiptType;
}

/**
 * Pure card view: small header "<headline> • <subject>", then a large logo tile (or monogram
 * fallback) beside a bold title, then a muted detail line. No status dot; a failed action tints the header.
 * Non-interactive: pointer-events: none.
 * Accessibility: role="status" aria-live="polite".
 */
export function ActionReceiptCard({ receipt }: ActionReceiptCardProps) {
  const logoId = receiptLogoId(receipt.subject);
  const bigTitle = receipt.title || receiptDisplayName(receipt.subject);

  return (
    <div
      className={`action-receipt${receipt.ok ? "" : " action-receipt--fail"}`}
      role="status"
      aria-live="polite"
    >
      <div className="action-receipt-header">{formatReceiptHeadline(receipt)}</div>
      <div className="action-receipt-main">
        <div className="action-receipt-tile" aria-hidden="true">
          <ProviderLogo icon={logoId} provider={receipt.subject} />
        </div>
        <div className="action-receipt-title">{bigTitle}</div>
      </div>
      {receipt.detail && (
        <div className="action-receipt-detail">{receipt.detail}</div>
      )}
    </div>
  );
}

/**
 * ActionReceipt — stage overlay card for operations and observed outcomes.
 *
 * Appears under the orb/island area (layoutSlots.ts).
 * Fades in/out, auto-hides after 6 s, newer receipt replaces and restarts the timer,
 * hides on `orb:cancel`.
 * Non-interactive: pointer-events none, no hitbox registration.
 */
export function ActionReceipt() {
  const [rect, setRect] = useState<OrbRect | null>(null);
  const [receipt, setReceipt] = useState<ActionReceiptType | null>(null);
  const [visible, setVisible] = useState(false);
  const [renderedReceipt, setRenderedReceipt] = useState<ActionReceiptType | null>(null);

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  const clearTimer = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  // Listen to action:receipt and orb:cancel
  useEffect(() => {
    let unlistenReceipt: (() => void) | null = null;
    let unlistenCancel: (() => void) | null = null;

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");

      unlistenReceipt = await listen<unknown>("action:receipt", (event) => {
        const parsed = normalizeReceiptPayload(event.payload);
        if (parsed) {
          clearTimer();
          setReceipt(parsed);
          setVisible(true);
          timerRef.current = setTimeout(() => {
            setVisible(false);
            timerRef.current = null;
          }, RECEIPT_AUTO_HIDE_MS);
        }
      });

      unlistenCancel = await listen("orb:cancel", () => {
        clearTimer();
        setVisible(false);
      });
    })();

    return () => {
      unlistenReceipt?.();
      unlistenCancel?.();
      clearTimer();
    };
  }, []);

  // Track stage orb rect
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<OrbRect>("stage:orb_rect", (event) => {
        if (event.payload) setRect(event.payload);
      });
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const pending = await invoke<OrbRect | null>("get_pending_orb_rect");
        if (pending) setRect(pending);
      } catch {
        /* non-Tauri (tests) — no-op */
      }
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // Direct DOM mutation for positioning (CSP-safe, inline style props are dropped)
  const applyPosition = (el: HTMLDivElement | null) => {
    if (!el) return;
    const dpr = window.devicePixelRatio || 1;
    const topY = rect ? rect.y / dpr : 0;
    const isTop = topY < (typeof window !== "undefined" ? window.innerHeight / 2 : 540);

    if (isTop) {
      const slot = actionReceiptSlot({
        w: typeof window !== "undefined" ? window.innerWidth : 1920,
        h: typeof window !== "undefined" ? window.innerHeight : 1080,
      });
      const centerX = slot.x + slot.w / 2;
      el.style.transform = `translate(${centerX}px, ${slot.y}px) translateX(-50%)`;
    } else if (rect) {
      const centerX = typeof window !== "undefined" ? window.innerWidth / 2 : rect.x / dpr + rect.w / dpr / 2;
      const bottomY = (rect.y + rect.h) / dpr;
      el.style.transform = `translate(${centerX}px, ${bottomY + 20}px) translateX(-50%)`;
    }
  };

  // Retention for fade-out transition before unmounting
  useEffect(() => {
    if (visible && receipt) {
      setRenderedReceipt(receipt);
    } else if (!visible) {
      const t = setTimeout(() => {
        setRenderedReceipt(null);
      }, 300);
      return () => clearTimeout(t);
    }
  }, [visible, receipt]);

  useEffect(() => {
    applyPosition(containerRef.current);
  }, [rect, renderedReceipt, visible]);

  useEffect(() => {
    const handleResize = () => applyPosition(containerRef.current);
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [rect]);

  if (!renderedReceipt) {
    return null;
  }

  return (
    <div
      className={`action-receipt-container ${visible ? "action-receipt-container--visible" : ""}`}
      ref={(el) => {
        containerRef.current = el;
        applyPosition(el);
      }}
    >
      <ActionReceiptCard receipt={renderedReceipt} />
    </div>
  );
}
