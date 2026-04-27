import type { CSSProperties } from "react";
import { AppConfig, DEFAULT_CONFIG } from "./store";

const HEX6 = /^#([0-9A-Fa-f]{6})$/;

export function normalizeHex(input: string, fallback: string): string {
  const s = input.trim();
  if (HEX6.test(s)) return s;
  return fallback;
}

const ANIM = new Set(["flow", "breathe", "static", "aurora"] as const);
const WAVE = new Set(["rolling", "smooth", "line"] as const);
const CORNER = new Set(["none", "round"] as const);

function pick<T extends string>(val: string, set: Set<string>, d: T): T {
  return (set.has(val) ? val : d) as T;
}

/** CSS classes для анимации / формы волны. */
export function widgetClassFromConfig(c: AppConfig): string {
  const d = DEFAULT_CONFIG;
  const a = pick(c.widgetAnimation, ANIM, d.widgetAnimation);
  const w = pick(c.widgetWaveForm, WAVE, d.widgetWaveForm);
  return `widget-anim--${a} widget-wave--${w}`;
}

/** CSS variables для .widget: цвета + радиусы. */
export function widgetStyleFromConfig(c: AppConfig): CSSProperties {
  const d = DEFAULT_CONFIG;
  const corner = pick(c.widgetCornerStyle, CORNER, d.widgetCornerStyle);
  return {
    ["--w-bg-from" as string]: normalizeHex(c.widgetBgFrom, d.widgetBgFrom),
    ["--w-bg-to" as string]: normalizeHex(c.widgetBgTo, d.widgetBgTo),
    ["--w-w1" as string]: normalizeHex(c.widgetWave1, d.widgetWave1),
    ["--w-w2" as string]: normalizeHex(c.widgetWave2, d.widgetWave2),
    ["--w-w3" as string]: normalizeHex(c.widgetWave3, d.widgetWave3),
    ["--widget-corners" as string]: corner === "round" ? "10px" : "0",
  } as CSSProperties;
}
