export type StackDetectionStatus = "detected" | "likely" | "inferred";

export interface StackDetection {
  id: string;
  name: string;
  category: string;
  confidence: number;
  status: StackDetectionStatus;
  evidence: string[];
  website?: string | null;
  logo?: string | null;
}

export interface StackGroup {
  category: string;
  label: string;
  ids: string[];
}

export interface StackSecurity {
  hsts: boolean;
  csp: boolean;
  x_frame_options: boolean;
  x_content_type_options: boolean;
  referrer_policy: boolean;
  permissions_policy: boolean;
}

export interface StackStats {
  html_bytes: number;
  script_count: number;
  inline_script_count: number;
  stylesheet_count: number;
  image_count: number;
  js_bytes_fetched: number;
  css_bytes_fetched: number;
  third_party_hosts: string[];
}

export interface StackSection {
  idx: number;
  title: string;
  summary: string;
  stack_refs: string[];
}

export interface StackPayload {
  title: string;
  url: string;
  final_url: string;
  status_code: number;
  lang: string;
  description: string;
  scanning: boolean;
  detections: StackDetection[];
  groups: StackGroup[];
  security: StackSecurity;
  stats: StackStats;
  notes: string[];
  sections: StackSection[];
  markdown: string;
  summary: string;
  site_logo?: string | null;
}

export type StackProgressPhase =
  | "starting"
  | "scanning"
  | "layout"
  | "touring"
  | "summary"
  | "done"
  | "cancelled"
  | "error";

export interface StackProgress {
  phase: StackProgressPhase;
  text: string;
  step?: number;
  total?: number;
}

/** Logos that arrive after the first paint (`sidebar:stack_logos`). */
export interface StackLogoPatch {
  site_logo?: string | null;
  logos?: Record<string, string>;
}

export interface StackFocusPayload {
  ids: string[];
}

export interface DisplayGroup {
  category: string;
  label: string;
  detections: StackDetection[];
}

export interface SecurityScore {
  passed: number;
  total: number;
  missing: string[];
}
