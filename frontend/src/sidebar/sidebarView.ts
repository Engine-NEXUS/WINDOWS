/** The views the unified sidebar can show. */
export type SidebarView = "assistant" | "settings" | "architect" | "pr-list" | "spatial" | "annotate" | "stack" | "maps" | "place";

export const VIEWS: SidebarView[] = ["assistant", "settings", "architect", "pr-list", "spatial", "annotate", "stack", "maps", "place"];

/** Rust names the notification detail view "coderabbit", but that detail is drawn by the assistant view (there is no
 *  separate view for it). Without this mapping the request was ignored, so an alert clicked while another view (the
 *  Command Hub, the PR list, the architect map) was open never became visible. */
const ALIASES: Record<string, SidebarView> = { coderabbit: "assistant" };

/** The view to show for a name sent by Rust, or null for a name the sidebar does not know. */
export function resolveSidebarView(name: string | null | undefined): SidebarView | null {
  if (!name) return null;
  const v = ALIASES[name] ?? name;
  return (VIEWS as string[]).includes(v) ? (v as SidebarView) : null;
}
