// Skin registry: consumers ask for the active skin here and use it without naming it.
// No DOM on import and no CSS imports, so node:test can load it (main.ts loads the CSS).
import type { MonthDay, SkinDef } from "./types.ts";
import { classic } from "./classic/index.ts";
import { halloween } from "./halloween/index.ts";
import { christmas } from "./christmas/index.ts";

export type Skin = "classic" | "halloween" | "christmas";
export type SkinChoice = "auto" | Skin;

export const SKINS = { classic, halloween, christmas } satisfies Record<Skin, SkinDef>;

/** Ranges may wrap around the new year. */
export function inSeason(season: { from: MonthDay; to: MonthDay }, date: Date): boolean {
  const key = ([m, d]: MonthDay) => m * 100 + d;
  const today = key([date.getMonth() + 1, date.getDate()]);
  const from = key(season.from), to = key(season.to);
  return from <= to ? today >= from && today <= to : today >= from || today <= to;
}

/** `auto` picks the skin whose season contains the local date, else `classic`. */
export function resolveSkin(choice: SkinChoice, date: Date): Skin {
  if (choice !== "auto") return choice;
  const def = Object.values(SKINS).find((s: SkinDef) => s.season && inSeason(s.season, date));
  return def?.id ?? "classic";
}
