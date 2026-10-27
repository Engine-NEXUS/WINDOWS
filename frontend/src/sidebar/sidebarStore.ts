import { create } from "zustand";

/**
 * Sidebar store — shared state for the NEXUS Response Sidebar window.
 *
 * Supports:
 * - Response text and user query
 * - Font size customization (saved to localStorage)
 * - TTS playback tracking
 * - Lightbox modal for enlarged image viewing
 * - Copy confirmation feedback
 */

export type SidebarFontSize = "sm" | "md" | "lg" | "xl";

/** GitHub merge conflict data for the conflict panel. */
export interface ConflictData {
  prNumber: number;
  repo: string;
  conflictFiles: {
    filename: string;
    conflict_count: number;
    blocks: {
      start_line: number;
      head_content: string;
      branch_content: string;
    }[];
  }[];
  message: string;
}

export interface RepoAnalysis {
  repo: string;
  visibility: string;
  description: string;
  stars: number;
  forks: number;
  totalFiles: number;
  languages: { name: string; bytes: number; percentage: number }[];
  frameworks: { name: string; category: string }[];
  databases: { name: string; evidence: string }[];
  features: string[];
  tests: boolean;
  ci: string;
  docker: boolean;
  architecture: string;
  defaultBranch: string;
}

export interface ConfirmationData {
  requestId: string;
  prompt: string;
  command: any;
}

// ─── Feature 86: Spatial analysis data ──────────────────────────────

export interface SpatialDetailRow {
  title: string;
  value: string;
}

export interface SpatialItemCard {
  id: number;
  label: string;
  category: string;
  summary: string;
  details: SpatialDetailRow[];
  image_data_url?: string;
  logo_data_url?: string;
}

export interface SpatialData {
  title: string;
  overview: string;
  items: SpatialItemCard[];
  provider_used: string;
  screen_image_data_url?: string;
  review?: ForegroundReviewMeta;
}

export interface ForegroundReviewMeta {
  id: string;
  created_at: string;
  target_app: string;
  target_title: string;
  target_url?: string | null;
  completeness: string;
  persistence: string;
  pages: Array<{
    url: string;
    title: string;
    captured_at: string;
    image_data_url?: string | null;
    method: string;
    completeness: string;
    rea_summary?: string | null;
  }>;
  technologies: Array<{
    name: string;
    status: string;
    evidence: string;
    source_url: string;
  }>;
  warnings: string[];
}

import type { CodeRabbitReviewData } from "./CodeRabbitDashboard";
import type { StackPayload, StackProgress, StackLogoPatch } from "./stackTypes";

/** Overlay late-arriving logos onto a stack payload without touching anything else. */
export function applyLogoPatch(data: StackPayload, patch: StackLogoPatch): StackPayload {
  const logos = patch.logos || {};
  return {
    ...data,
    site_logo: patch.site_logo ?? data.site_logo ?? null,
    detections: (data.detections || []).map((d) =>
      logos[d.id] && !d.logo ? { ...d, logo: logos[d.id] } : d,
    ),
  };
}

interface SidebarState {
  visible: boolean;
  response: string;
  query: string;
  timestamp: number;
  fontSize: SidebarFontSize;
  speaking: boolean;
  activeImage: { src: string; alt: string } | null;
  collapsedQuery: boolean;
  analysisData: RepoAnalysis | null;
  codeRabbitData: CodeRabbitReviewData | null;
  conflictData: ConflictData | null;
  confirmationData: ConfirmationData | null;
  spatialData: SpatialData | null;
  stackData: StackPayload | null;
  stackProgress: StackProgress | null;
  stackFocus: string[];
  stackLogoPatch: StackLogoPatch | null;

  show: (query: string, text: string) => void;
  showAnalysis: (query: string, text: string, analysis: RepoAnalysis) => void;
  showCodeRabbit: (data: CodeRabbitReviewData) => void;
  showConflict: (data: ConflictData) => void;
  showConfirmation: (data: ConfirmationData) => void;
  showSpatial: (data: SpatialData) => void;
  showStack: (data: StackPayload) => void;
  setStackProgress: (p: StackProgress | null) => void;
  setStackFocus: (ids: string[]) => void;
  mergeStackLogos: (patch: StackLogoPatch) => void;
  hide: () => void;
  setFontSize: (size: SidebarFontSize) => void;
  setSpeaking: (speaking: boolean) => void;
  setActiveImage: (image: { src: string; alt: string } | null) => void;
  openImage: (src: string, alt: string) => void;
  setCollapsedQuery: (collapsed: boolean) => void;
}

const SAVED_FONT_SIZE = (localStorage.getItem("nexus_sidebar_font_size") as SidebarFontSize) || "md";

export const useSidebar = create<SidebarState>((set, get) => ({
  visible: false,
  response: "",
  query: "",
  timestamp: Date.now(),
  fontSize: SAVED_FONT_SIZE,
  speaking: false,
  activeImage: null,
  collapsedQuery: false,
  analysisData: null,
  codeRabbitData: null,
  conflictData: null,
  confirmationData: null,
  spatialData: null,
  stackData: null,
  stackProgress: null,
  stackFocus: [],
  stackLogoPatch: null,

  show: (query: string, text: string) => {
    console.log("[sidebarStore] show called: query=", query?.substring(0, 50), "text=", text?.substring(0, 50));
    set({
      visible: true,
      query,
      response: text,
      timestamp: Date.now(),
      speaking: false,
      activeImage: null,
      analysisData: null,
      codeRabbitData: null,
      conflictData: null,
      confirmationData: null,
      spatialData: null,
      stackData: null,
    });
  },

  showAnalysis: (query: string, text: string, analysis: RepoAnalysis) => {
    console.log("[sidebarStore] showAnalysis called: query=", query?.substring(0, 50), "text=", text?.substring(0, 50));
    set({
      visible: true,
      query,
      response: text,
      timestamp: Date.now(),
      speaking: false,
      activeImage: null,
      analysisData: analysis,
      codeRabbitData: null,
      conflictData: null,
      confirmationData: null,
      spatialData: null,
      stackData: null,
    });
  },

  showCodeRabbit: (data: CodeRabbitReviewData) => {
    console.log("[sidebarStore] showCodeRabbit called: pr=", data.prNumber, data.repo);
    set({
      visible: true,
      query: `CodeRabbit Review — PR #${data.prNumber} in ${data.repo}`,
      response: data.summary,
      timestamp: Date.now(),
      speaking: false,
      activeImage: null,
      analysisData: null,
      codeRabbitData: data,
      conflictData: null,
      confirmationData: null,
      spatialData: null,
      stackData: null,
    });
  },

  showConflict: (data: ConflictData) => {
    set({
      visible: true,
      query: `Merge Conflict — PR #${data.prNumber}`,
      response: data.message,
      timestamp: Date.now(),
      speaking: false,
      activeImage: null,
      analysisData: null,
      codeRabbitData: null,
      conflictData: data,
      confirmationData: null,
      spatialData: null,
      stackData: null,
    });
  },

  showConfirmation: (data: ConfirmationData) => {
    console.log("[sidebarStore] showConfirmation called:", data);
    set({
      visible: true,
      query: "Action Confirmation",
      response: data.prompt,
      timestamp: Date.now(),
      speaking: false,
      activeImage: null,
      analysisData: null,
      codeRabbitData: null,
      conflictData: null,
      confirmationData: data,
      spatialData: null,
      stackData: null,
    });
  },

  showSpatial: (data: SpatialData) => {
    console.log("[sidebarStore] showSpatial called:", data.title, `${data.items?.length ?? 0} items`);
    set({
      visible: true,
      query: data.title,
      response: "",
      timestamp: Date.now(),
      speaking: false,
      activeImage: null,
      analysisData: null,
      conflictData: null,
      confirmationData: null,
      spatialData: data,
      stackData: null,
    });
  },

  showStack: (incoming: StackPayload) => {
    console.log("[sidebarStore] showStack called:", incoming?.title);
    // A new scan starts clean; later payloads of the same scan keep the logos
    // that already arrived.
    const prior = incoming?.scanning ? null : get().stackLogoPatch;
    const data = incoming && prior ? applyLogoPatch(incoming, prior) : incoming;
    set({
      stackLogoPatch: prior,
      visible: true,
      query: data?.title || "Tech Stack",
      response: "",
      timestamp: Date.now(),
      speaking: false,
      activeImage: null,
      analysisData: null,
      codeRabbitData: null,
      conflictData: null,
      confirmationData: null,
      spatialData: null,
      stackData: data,
    });
  },

  setStackProgress: (stackProgress: StackProgress | null) => set({ stackProgress }),

  setStackFocus: (stackFocus: string[]) => set({ stackFocus: stackFocus || [] }),

  mergeStackLogos: (patch: StackLogoPatch) => {
    if (!patch) return;
    const prev = get().stackLogoPatch;
    const merged: StackLogoPatch = {
      site_logo: patch.site_logo ?? prev?.site_logo ?? null,
      logos: { ...(prev?.logos || {}), ...(patch.logos || {}) },
    };
    const current = get().stackData;
    set({
      stackLogoPatch: merged,
      stackData: current ? applyLogoPatch(current, merged) : current,
    });
  },

  hide: () =>
    set({
      visible: false,
      speaking: false,
      activeImage: null,
      analysisData: null,
      conflictData: null,
      confirmationData: null,
      spatialData: null,
      stackData: null,
      stackProgress: null,
      stackFocus: [],
    }),

  setFontSize: (size: SidebarFontSize) => {
    localStorage.setItem("nexus_sidebar_font_size", size);
    set({ fontSize: size });
  },

  setSpeaking: (speaking: boolean) => set({ speaking }),

  setActiveImage: (activeImage) => set({ activeImage }),

  openImage: (src: string, alt: string) => set({ activeImage: { src, alt } }),

  setCollapsedQuery: (collapsedQuery) => set({ collapsedQuery }),
}));

/**
 * Escape-key dismissal hierarchy for NEXUS overlay windows (plan C3).
 * An open in-window overlay (image lightbox, and by extension any future
 * modal) always closes FIRST; only with nothing overlaying does Escape
 * close the window itself. Pure function so the hierarchy is unit-tested
 * and can't be silently reordered by a future edit.
 */
export type EscDismiss = "close-overlay" | "close-window";

export function resolveEscDismiss(hasOverlayOpen: boolean): EscDismiss {
  return hasOverlayOpen ? "close-overlay" : "close-window";
}
