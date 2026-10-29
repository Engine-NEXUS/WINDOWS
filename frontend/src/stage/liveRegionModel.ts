export interface LiveRegionState {
  captionText?: string | null;
  loading?: boolean;
}

/**
 * Pure function to determine the announced screen-reader text for the live region.
 *
 * - When response caption text is active and non-empty, it returns the trimmed caption text
 *   so screen readers announce NEXUS's spoken reply.
 * - When loading is true and caption text is empty, it returns "Working" so screen readers
 *   announce that NEXUS is currently processing / thinking.
 * - When neither is active, it returns an empty string.
 */
export function buildLiveRegionText(
  captionOrState?: string | null | LiveRegionState,
  maybeLoading?: boolean,
): string {
  let captionText: string | null | undefined;
  let loading = false;

  if (typeof captionOrState === "object" && captionOrState !== null) {
    captionText = captionOrState.captionText;
    loading = Boolean(captionOrState.loading);
  } else {
    captionText = captionOrState;
    loading = Boolean(maybeLoading);
  }

  const trimmedCaption = captionText ? captionText.trim() : "";
  if (trimmedCaption.length > 0) {
    return trimmedCaption;
  }
  if (loading) {
    return "Working";
  }
  return "";
}

/**
 * Extract the displayable text from a CaptionLineEvent, returning an empty string
 * if the event is null or has phase "cleared".
 */
export function extractCaptionText(event?: { text?: string; phase?: string } | null): string {
  if (!event || event.phase === "cleared") {
    return "";
  }
  return (event.text || "").trim();
}
