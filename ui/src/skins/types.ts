// Skin contract (docs/SPEC.md → Skins → Implementación). Types only, so Node can import it.
import type { Key } from "../i18n";
import type { Note, Sound } from "../sound/sfx";
import type { Skin } from "./index";

/** [month 1..12, day]. */
export type MonthDay = [number, number];

export interface SkinDef {
  id: Skin;
  label: Key;
  /** Auto window, both days included; may wrap the year (Dec 24 → Jan 6). */
  season?: { from: MonthDay; to: MonthDay };
  /** An SVG for the slot over the mascot's terminal and the tint of the lit LEDs. */
  mascot?: { accessory: string; leds?: string[] };
  /** Markup of the island ambience layer (behind the content, click-through). */
  ambience?: string;
  /** Missing ones sound like the classic ones. */
  sounds?: Partial<Record<Sound, Note[]>>;
  /** Only for what needs JS; it receives just its own ambience layer. */
  behavior?: { start(layer: HTMLElement): void; stop(): void };
}
