/**
 * Centralized color/display config for all game concepts.
 * Adding a new tag, zone, domain, or task type = one edit here.
 */

// --- Fallback for unknown types ---
export function hashColor(value: string): string {
  let hash = 0;
  for (let i = 0; i < value.length; i++) {
    hash = value.charCodeAt(i) + ((hash << 5) - hash);
  }
  const hue = Math.abs(hash) % 360;
  // Convert to hex so callers can safely append alpha bytes
  const h = hue / 360;
  const s = 0.55;
  const l = 0.55;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => {
    const k = (n + h * 12) % 12;
    const color = l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
    return Math.round(255 * color).toString(16).padStart(2, '0');
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}

// --- Element / material colors ---
export const ELEMENT_COLORS: Record<string, string> = {
  Fe: '#c47038',
  Si: '#8a9aaf',
  He: '#c4a038',
  H2O: '#4a90d9',
  LH2: '#5cc8e8',
  LOX: '#e08a6a',
};

export function elementColor(element: string): string {
  return ELEMENT_COLORS[element] ?? hashColor(element);
}

/** Elements that experience boiloff — show loss indicator in inventory */
export const CRYO_ELEMENTS = new Set(['LH2', 'LOX']);

// --- Asteroid anomaly tags ---
export const TAG_COLORS: Record<string, { bg: string; text: string }> = {
  IronRich: { bg: 'rgba(196, 112, 56, 0.15)', text: '#c47038' },
  VolatileRich: { bg: 'rgba(56, 160, 196, 0.15)', text: '#38a0c4' },
  Carbonaceous: { bg: 'rgba(180, 140, 60, 0.15)', text: '#b48c3c' },
};

export function tagColor(tag: string): string {
  return TAG_COLORS[tag]?.text ?? hashColor(tag);
}

// --- Zone fill/stroke ---
export const ZONE_COLORS: Record<string, string> = {
  MetalRich: 'rgba(217, 158, 60, 0.15)',
  Mixed: 'rgba(160, 160, 180, 0.12)',
  VolatileRich: 'rgba(80, 140, 220, 0.15)',
};

export const ZONE_STROKES: Record<string, string> = {
  MetalRich: 'rgba(217, 158, 60, 0.3)',
  Mixed: 'rgba(160, 160, 180, 0.2)',
  VolatileRich: 'rgba(80, 140, 220, 0.3)',
};

// --- Celestial body types ---
export const BODY_COLORS: Record<string, string> = {
  Star: '#f5c842',
  Planet: '#6b9dba',
  Moon: '#999',
  Belt: '#888',
};

// --- Ship task types ---
export const SHIP_TASK_COLORS: Record<string, string> = {
  Survey: '#5b9bd5',
  DeepScan: '#7b68ee',
  Mine: '#d4a44c',
  Deposit: '#4caf7d',
  Transit: '#5ca0c8',
};

/** Color for idle/unknown tasks — works in both CSS and canvas contexts. */
export const IDLE_COLOR = '#8a8e98';

export function shipTaskColor(taskKind: string | null): string {
  if (!taskKind) {return IDLE_COLOR;}
  return SHIP_TASK_COLORS[taskKind] ?? IDLE_COLOR;
}

// --- Research data kinds ---
export const DATA_KIND_COLORS: Record<string, string> = {
  SurveyData: '#5ca0c8',
  AssayData: '#c89a4a',
  ManufacturingData: '#4caf7d',
  TransitData: '#a78bfa',
  OpticalData: '#7ec8e3',
  RadioData: '#e0a84c',
};

export const DATA_KIND_LABELS: Record<string, string> = {
  SurveyData: 'Survey',
  AssayData: 'Assay',
  ManufacturingData: 'Manufacturing',
  TransitData: 'Transit',
  OpticalData: 'Optical',
  RadioData: 'Radio',
};

// --- Research domains ---
export const DOMAIN_COLORS: Record<string, string> = {
  Survey: '#5ca0c8',
  Materials: '#c89a4a',
  Manufacturing: '#4caf7d',
  Propulsion: '#a78bfa',
};

// --- Lab statuses ---
export const LAB_STATUS_STYLES: Record<string, { bg: string; text: string; label: string }> = {
  active: { bg: 'rgba(76,175,125,0.15)', text: '#4caf7d', label: 'active' },
  starved: { bg: 'rgba(224,82,82,0.15)', text: '#e05252', label: 'starved' },
  idle: { bg: 'rgba(90,96,110,0.2)', text: '#6b7280', label: 'idle' },
};

// --- Map rendering colors (canvas-safe hex, no CSS vars) ---
export const MAP_COLORS = {
  orbitRing: '#2a2e38',
  stationAccent: '#5ca0c8',
  scanSiteBg: '#1a1d26',
  scanSiteStroke: '#5c6070',
  scanSiteText: '#8a8e98',
  bodyLabelStar: '#c8ccd4',
  bodyLabelOther: '#6b7080',
  starGlow: 'rgba(245,200,66,0.12)',
  starGlowMid: 'rgba(245,200,66,0.04)',
  stationPulse: 'rgba(92,160,200,',
} as const;

// --- Semantic colors (positive/negative indicators) ---
export const SEMANTIC_COLORS = {
  positive: '#4caf7d',
  negative: '#e05252',
} as const;

// --- Manufacturing DAG item types ---
export const ITEM_TYPE_COLORS: Record<string, string> = {
  raw: '#f59e0b',
  refined: '#eab308',
  component: '#3b82f6',
  ship: '#22c55e',
};

export function itemTypeColor(type: string): string {
  return ITEM_TYPE_COLORS[type] ?? hashColor(type);
}

export const RECIPE_STATUS_COLORS: Record<string, string> = {
  active: '#4caf7d',
  available: '#60a5fa',
};
