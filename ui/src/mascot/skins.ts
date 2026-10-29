// Resolución pura de la skin de la pilita: sin DOM ni Tauri, para poder probarla con node:test.
export type SkinChoice = "auto" | "classic" | "halloween";
export type Skin = "classic" | "halloween";

/** `auto` es halloween del 15 de octubre al 2 de noviembre (fecha local), y clásica el resto. */
export function resolveSkin(choice: SkinChoice, date: Date): Skin {
  if (choice !== "auto") return choice;
  const month = date.getMonth();
  const day = date.getDate();
  const inSeason = (month === 9 && day >= 15) || (month === 10 && day <= 2);
  return inSeason ? "halloween" : "classic";
}
