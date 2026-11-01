import "./mascot/pilita";
// Every skin CSS (each rule scoped under :root[data-skin="<id>"]); index.html links styles.css.
import.meta.glob("./skins/*/*.css", { eager: true });
import { mountIsland } from "./island/island";
import { lang } from "./i18n";

document.documentElement.lang = lang;
mountIsland(document.getElementById("island")!);
