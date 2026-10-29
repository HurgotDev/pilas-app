import "./mascot/pilita";
// CSS de todas las skins (cada regla va bajo :root[data-skin="<id>"]); styles.css lo enlaza index.html.
import.meta.glob("./skins/*/*.css", { eager: true });
import { mountIsland } from "./island/island";
import { lang } from "./i18n";

document.documentElement.lang = lang;
mountIsland(document.getElementById("island")!);
