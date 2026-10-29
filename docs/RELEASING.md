# Ramas, changelog y releases

Pilas sigue un git-flow sencillo. Cada release es un único PR manual a
`main`: al fusionarlo, GitHub Actions compila el `.dmg` universal, publica la
versión y devuelve `main` a `develop` con un back-merge automático.

## Modelo de ramas

- `main`: lo publicado. Recibe PRs desde `release/*` y `hotfix/*`.
- `develop`: integración. Recibe PRs desde `feat/*` y `fix/*`, y el
  back-merge automático de `main` tras cada release.
- `feat/<slug>`, `fix/<slug>`: trabajo diario, siempre desde `develop`.
- `release/<AAAA-MM-DD>`: prepara la versión (la fecha del día en que se
  corta). Sale de `develop` y su PR va directamente a `main`. Ya no hay PR de
  preparación a `develop` ni PR de `develop` a `main`.
- `hotfix/<slug>`: arreglo urgente. Sale de `main` y su PR vuelve a `main`.

El job `policy` de CI (`.github/workflows/ci.yml`) hace cumplir estas reglas
en cada PR.

## Fragmentos de changelog

Cada PR de `feat/*` o `fix/*` a `develop` añade un archivo
`.changes/<slug>.md`, escrito en inglés:

```
---
bump: minor   # patch | minor | major
---
Add the hook installer with backup and diff.
```

- `patch`: correcciones. `minor`: novedades. `major`: cambios incompatibles.
- El cuerpo es una frase en inglés, en imperativo; será una viñeta en
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
2. Revisa `CHANGELOG.md`, haz commit ("Prepare version X.Y.Z") y sube la rama.
3. Abre un PR de `release/<fecha>` directamente a `main`. CI comprueba que no
   queden fragmentos, que la versión sea mayor que la de `main` y que el
   changelog tenga su sección.
4. Fusiónalo con **Create a merge commit** (no squash). Al entrar en `main`,
   `.github/workflows/release.yml` compila el `.dmg`, crea la etiqueta
   `vX.Y.Z` y publica el release con las notas del changelog. Si la etiqueta
   ya existe, no hace nada.
5. Automático: el job `backmerge` del mismo workflow lleva `main` a `develop`
   (ver la sección siguiente). No tienes que abrir nada más.

## Hotfix

1. `git switch main && git pull && git switch -c hotfix/<slug>`.
2. Arregla, añade el fragmento (normalmente `bump: patch`) y ejecuta
   `npm run release:prepare` en la misma rama. Haz commit.
3. Abre un PR de `hotfix/<slug>` a `main` y fusiónalo con **Create a merge
   commit**. Se publica igual que un release normal.
4. El back-merge automático también lleva el hotfix a `develop`.

## Back-merge automático

Tras publicar un release, el job `backmerge` de `release.yml`:

1. Si `develop` ya contiene todo `main`, no hace nada.
2. Si ya hay un PR abierto de `main` a `develop`, lo reutiliza. Si no, lo
   crea con el título "Back-merge main into develop after X.Y.Z".
3. Activa el auto-merge con commit de merge. El PR se fusiona cuando pasan
   los checks de CI.

Requisitos:

- Secreto `BACKMERGE_TOKEN` (ver "Secretos y artefactos").
- Ajuste del repo **Settings → General → Pull Requests → Allow auto-merge**
  activado. Sin él, `gh pr merge --auto` falla.
- Se usa un token propio y no `GITHUB_TOKEN` porque los PRs abiertos con
  `GITHUB_TOKEN` no disparan CI: el check `policy` nunca correría y el PR
  no se podría fusionar.
- Si falta `BACKMERGE_TOKEN`, el job solo emite un aviso y el release se
  publica igual. Haz entonces el PR de `main` a `develop` a mano.

Si el back-merge tiene conflictos (ambas ramas tocaron los mismos archivos),
el auto-merge no se completa y el job falla. El release ya está publicado.
Resuélvelo a mano: crea una rama desde `develop`, ejecuta
`git merge origin/main`, resuelve los conflictos, abre un PR a `develop` y
cierra el de back-merge.

## Secretos y artefactos

El release necesita estos secretos del repo (Settings → Secrets and
variables → Actions):

- `TAURI_SIGNING_PRIVATE_KEY`: clave privada de minisign que firma el
  tarball del updater. Sin ella el job falla con un mensaje claro.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: contraseña de esa clave.
- `HOMEBREW_TAP_TOKEN`: token con permiso de escritura en
  `HurgotDev/homebrew-tap`. Si falta, el cask no se actualiza, pero el
  release se publica igual (aparece un aviso).
- `BACKMERGE_TOKEN`: token de grano fino para el back-merge automático. Si
  falta, no se hace back-merge (aparece un aviso). Para crearlo:
  1. En GitHub, ve a Settings de tu cuenta → Developer settings →
     Personal access tokens → Fine-grained tokens → Generate new token.
  2. Resource owner: `HurgotDev`. Anota la fecha de caducidad: al caducar,
     el job `backmerge` falla hasta que renueves el token y actualices el
     secreto.
  3. Repository access: **Only select repositories** y elige
     `HurgotDev/pilas-app`.
  4. Repository permissions: **Contents** → Read and write, y
     **Pull requests** → Read and write. Ningún otro permiso.
  5. Genera el token, cópialo y guárdalo como secreto `BACKMERGE_TOKEN` del
     repo.

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

- Los PRs de `release/*` y `hotfix/*` los abre una persona (desde GitHub o con
  `gh pr create`). Los PRs que abre un workflow con `GITHUB_TOKEN` no disparan
  CI, así que el check `policy` nunca correría y no se podrían fusionar. Por
  eso el back-merge usa `BACKMERGE_TOKEN`.
- Para probar la compilación sin publicar, lanza el workflow Release a mano
  desde la pestaña Actions: deja el `.dmg` y el tarball como artefactos.
- Para generar la firma local (solo hace falta si cambias la clave):
  `npx tauri signer generate -w ~/.tauri/pilas.key`. La clave pública va en
  `plugins.updater.pubkey` de `src-tauri/tauri.conf.json`. No la subas al repo.
- Publicar es definitivo; si algo sale mal, corrige con un hotfix en vez de
  reescribir la etiqueta.
