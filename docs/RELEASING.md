# Ramas, changelog y releases

Pilas sigue un git-flow sencillo. Cada PR a `main` es un release: al
fusionarlo, GitHub Actions compila el `.dmg` universal y publica la versión.

## Modelo de ramas

- `main`: lo publicado. Solo recibe PRs desde `develop` o `hotfix/*`.
- `develop`: integración. Recibe PRs desde `feat/*`, `fix/*`, `release/*` y
  los back-merges desde `main`.
- `feat/<slug>`, `fix/<slug>`: trabajo diario, siempre desde `develop`.
- `release/<AAAA-MM-DD>`: prepara la versión (la fecha del día en que se
  corta). Sale de `develop` y vuelve a `develop`.
- `hotfix/<slug>`: arreglo urgente desde `main` que vuelve a `main`.

El job `policy` de CI (`.github/workflows/ci.yml`) hace cumplir estas reglas
en cada PR.

## Fragmentos de changelog

Cada PR de `feat/*` o `fix/*` a `develop` añade un archivo
`.changes/<slug>.md`:

```
---
bump: minor   # patch | minor | major
---
Añade el instalador de hooks con backup y diff.
```

- `patch`: correcciones. `minor`: novedades. `major`: cambios incompatibles.
- El cuerpo es una frase en español, en imperativo; será una viñeta en
  `CHANGELOG.md`.
- `node scripts/release.mjs check` valida todos los fragmentos.

La versión no se escribe a mano: `npm run release:prepare` la calcula a partir
de la actual y del bump más alto entre los fragmentos (semver estricto: con
`0.x`, un `major` lleva a `1.0.0`).

## Release normal

1. Corta la rama desde `develop` actualizado:

   ```bash
   git switch develop && git pull
   git switch -c release/$(date +%F)
   npm run release:prepare   # imprime la nueva versión
   ```

   Esto sube la versión en `src-tauri/tauri.conf.json`, `Cargo.toml`,
   `Cargo.lock`, `package.json` y `package-lock.json`, añade la sección a
   `CHANGELOG.md` y borra los fragmentos consumidos.
2. Revisa el changelog, haz commit ("Prepara la versión X.Y.Z") y abre un PR
   de `release/<fecha>` a `develop`. Fusiónalo.
3. Abre un PR de `develop` a `main`. CI comprueba que no queden fragmentos,
   que la versión sea mayor que la de `main` y que el changelog tenga su
   sección.
4. Al fusionarlo, `.github/workflows/release.yml` compila el `.dmg`, crea la
   etiqueta `vX.Y.Z` y publica el release con las notas del changelog. Si la
   etiqueta ya existe, no hace nada.
5. Abre un PR de back-merge de `main` a `develop` y fusiónalo, para que
   `develop` tenga el commit de merge de `main`.

## Hotfix

1. `git switch main && git pull && git switch -c hotfix/<slug>`.
2. Arregla, añade el fragmento (normalmente `bump: patch`) y ejecuta
   `npm run release:prepare` en la misma rama. Haz commit.
3. Abre un PR de `hotfix/<slug>` a `main`. Al fusionarlo se publica igual que
   un release normal.
4. Haz el back-merge de `main` a `develop` con un PR.

## Secretos y artefactos

El release necesita estos secretos del repo (Settings → Secrets):

- `TAURI_SIGNING_PRIVATE_KEY`: clave privada de minisign que firma el
  tarball del updater. Sin ella el job falla con un mensaje claro.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: contraseña de esa clave.
- `HOMEBREW_TOKEN`: token con permiso de escritura en
  `HurgotDev/homebrew-tap`. Si falta, el cask no se actualiza, pero el
  release se publica igual (aparece un aviso).

Cada release publica en GitHub:

- `Pilas_<versión>_universal.dmg`: instalación manual y el cask.
- `Pilas_universal.app.tar.gz`: el paquete que descarga el updater.
- `latest.json`: versión, notas, fecha y firma. El endpoint de la app es
  `.../releases/latest/download/latest.json`, así que cada release nuevo
  reemplaza al anterior.

En `workflow_dispatch` no se publica nada: solo quedan como artefactos el
`.dmg` y el tarball.

Después de publicar, el job `cask` renderiza `packaging/homebrew/pilas.rb.tmpl`
con el sha256 del `.dmg` y lo sube a `Casks/pilas.rb` de
`HurgotDev/homebrew-tap`.

**Guarda la clave privada en un gestor de contraseñas.** Si la pierdes, las
instalaciones existentes no podrán actualizarse automáticamente: el updater
solo acepta paquetes firmados con la clave que llevan incrustada. Habría que
publicar una app nueva a mano y que cada persona la reinstale. Para cambiar
la clave hay que hacerlo con cuidado y en una versión de transición.

## Notas

- Los PRs los abre una persona (desde GitHub o con `gh pr create`). Los PRs
  que abre un workflow con `GITHUB_TOKEN` no disparan CI, así que el check
  `policy` nunca correría y no se podrían fusionar.
- Para probar la compilación sin publicar, lanza el workflow Release a mano
  desde la pestaña Actions: deja el `.dmg` y el tarball como artefactos.
- Para generar la firma local (solo hace falta si cambias la clave):
  `npx tauri signer generate -w ~/.tauri/pilas.key`. La clave pública va en
  `plugins.updater.pubkey` de `src-tauri/tauri.conf.json`. No la subas al repo.
- Publicar es definitivo; si algo sale mal, corrige con un hotfix en vez de
  reescribir la etiqueta.
