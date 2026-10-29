// Contrato de una skin (docs/SPEC.md → Skins → Implementación). Solo tipos: importable desde Node.
import type { Key } from "../i18n";
import type { Note, Sound } from "../sound/sfx";
import type { Skin } from "./index";

/** [mes 1..12, día]. */
export type MonthDay = [number, number];

export interface SkinDef {
  id: Skin;
  label: Key; // clave de i18n
  /** Ventana de Automático, ambos días incluidos; puede cruzar el año (24 dic → 6 ene). */
  season?: { from: MonthDay; to: MonthDay };
  /** Vestuario de la pilita: un SVG para su ranura sobre el borne y el tinte de los LEDs encendidos. */
  mascot?: { accessory: string; leds?: string[] };
  /** Marcado de la capa de ambiente de la isla (detrás del contenido, sin clics). */
  ambience?: string;
  /** Los que faltan suenan como los clásicos. */
  sounds?: Partial<Record<Sound, Note[]>>;
  /** Solo para lo que necesita JS; recibe únicamente su capa de ambiente. */
  behavior?: { start(layer: HTMLElement): void; stop(): void };
}
