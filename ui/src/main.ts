import "./mascot/pilita";
import { mountIsland } from "./island/island";
import { lang } from "./i18n";

document.documentElement.lang = lang;
mountIsland(document.getElementById("island")!);
