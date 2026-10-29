// Registro de skins: los consumidores piden la activa aquí y la usan sin nombrarla.
// Sin DOM al importar ni imports de CSS, para poder probarlo con node:test (los CSS los carga main.ts).
import type { MonthDay, SkinDef } from "./types.ts";
import { classic } from "./classic/index.ts";
import { halloween } from "./halloween/index.ts";
import { christmas } from "./christmas/index.ts";

export type Skin = "classic" | "halloween" | "christmas";
export type SkinChoice = "auto" | Skin;

export const SKINS = { classic, halloween, christmas } satisfies Record<Skin, SkinDef>;

/** ¿Cae `date` (fecha local) en la temporada? Admite rangos que cruzan el año. */
export function inSeason(season: { from: MonthDay; to: MonthDay }, date: Date): boolean {
  const key = ([m, d]: MonthDay) => m * 100 + d;
  const today = key([date.getMonth() + 1, date.getDate()]);
  const from = key(season.from), to = key(season.to);
  return from <= to ? today >= from && today <= to : today >= from || today <= to;
}

/** `auto` elige la skin cuya temporada contiene la fecha local; si ninguna, `classic`. */
export function resolveSkin(choice: SkinChoice, date: Date): Skin {
  if (choice !== "auto") return choice;
  const def = Object.values(SKINS).find((s: SkinDef) => s.season && inSeason(s.season, date));
  return def?.id ?? "classic";
}
