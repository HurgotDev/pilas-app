# Pilas

La pilita del notch que te avisa cuando Claude Code te necesita. Aprueba permisos, responde preguntas y salta a
la terminal correcta sin cambiar de ventana.

![La pilita](assets/icon.png)

> En desarrollo. Ver `docs/SPEC.md` para el alcance de la v1 y `docs/PLAN.md` para el avance.

## Instalación

Pilas es para macOS 13 o superior.

Con Homebrew (recomendado). El cask quita la marca de cuarentena por ti, así
que no hace falta `xattr`:

```bash
brew install --cask hurgotdev/tap/pilas
```

O descarga el `.dmg`:

1. Descarga el `.dmg` de la última versión en
   [GitHub Releases](https://github.com/HurgotDev/pilas-app/releases).
2. Abre el `.dmg` y arrastra **Pilas** a **Applications**.

Pilas no está firmada ni notarizada, así que macOS la bloqueará la primera
vez. Hay dos formas de permitirla:

- En Terminal (recomendado):

  ```bash
  xattr -dr com.apple.quarantine /Applications/Pilas.app
  ```

  Quita la marca de cuarentena de la app y también del binario `pilas-hook`
  que incluye, antes de que Pilas lo copie a su carpeta de datos.

- En **Ajustes del Sistema → Privacidad y seguridad**, pulsa **Abrir de todas
  formas** tras el primer intento. En versiones recientes de macOS, clic
  derecho → Abrir ya no salta este bloqueo.

En el primer arranque se abre el onboarding, que instala los hooks en Claude
Code. Después, el icono de la pilita en la barra de menú da acceso a
**Settings…** (Ajustes).

Pilas busca actualizaciones al arrancar y cada 24 horas, y avisa en el
menú de la barra y en Ajustes. No instala nada sin que lo pidas. Para
desactivar la búsqueda, quita la casilla **Buscar actualizaciones
automáticamente** en **Settings…**.

Para desinstalar, abre **Settings…** y pulsa **Disconnect** (Desconectar) para
quitar los hooks de `~/.claude/settings.json`. Luego borra `Pilas.app` y la
carpeta `~/Library/Application Support/Pilas`.

## Requisitos para desarrollar

- macOS 13 o superior, Xcode Command Line Tools (`xcode-select --install`)
- Rust estable (`curl https://sh.rustup.rs -sSf | sh`)
- Node 20 o superior
- Claude Code y Ghostty ≥ 1.3 para probar de punta a punta

```bash
npm install
npm run app          # abre Pilas en modo desarrollo
cargo test --workspace --exclude pilas
```

Para probar los hooks sin afectar tus propias sesiones de Claude Code, sigue `docs/HOOKS-DEV.md`.

## Contribuir

Crea tu rama (`feat/<slug>` o `fix/<slug>`) desde `develop`, añade un
fragmento en `.changes/` (ver `.changes/README.md`) y abre el PR a `develop`.

## Publicar una versión

Cada PR fusionado en `main` publica una versión automáticamente. El proceso
completo (ramas, changelog, releases y hotfixes) está en
`docs/RELEASING.md`.

## Licencia

MIT. Ver `LICENSE`.
