/** What Rust (`place_card.rs`) sends to the card. Every field but the name and position is optional. */
export interface PlaceCardData {
  name: string;
  category?: string | null;
  address?: string | null;
  lat: number;
  lon: number;
  distanceKm?: number | null;
  carMin?: number | null;
  bikeMin?: number | null;
  bikeApprox?: boolean;
  travelApprox?: boolean;
  photo?: { dataUri: string; kind: "photo" | "map"; credit?: string | null } | null;
  hoursToday?: string | null;
  openNow?: boolean | null;
  phone?: string | null;
  rating?: number | null;
  summary?: string | null;
  fares?: { kind: string; label: string; low: number; high: number }[];
}

/** A payload from the event bus is trusted only when it has the shape the card renders. */
export function isPlaceCard(v: unknown): v is PlaceCardData {
  if (!v || typeof v !== "object") return false;
  const c = v as Record<string, unknown>;
  return typeof c.name === "string" && c.name.length > 0 && typeof c.lat === "number" && typeof c.lon === "number";
}

/** The photo is shown only as an inline image data URI (Rust sends nothing else). */
export function safeImageSrc(uri: unknown): string | null {
  return typeof uri === "string" && /^data:image\/(jpeg|png|webp|gif);base64,[A-Za-z0-9+/=]+$/.test(uri) ? uri : null;
}

export function formatKm(km: number): string {
  return km < 10 ? `${km.toFixed(1)} km` : `${Math.round(km)} km`;
}

export function formatMinutes(min: number): string {
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

export function formatRupees(low: number, high: number): string {
  return low === high ? `₹${low}` : `₹${low}–${high}`;
}
