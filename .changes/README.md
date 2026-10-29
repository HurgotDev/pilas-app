# Fragmentos de changelog

Cada cambio que va a `develop` añade aquí un archivo `<slug>.md` (por
ejemplo `instalador-hooks.md`). Al preparar un release,
`npm run release:prepare` los junta en `CHANGELOG.md`, calcula la versión y
los borra.

Formato:

```
---
bump: minor   # patch | minor | major
---
Añade el instalador de hooks con backup y diff.
```

- `bump`: `patch` para correcciones, `minor` para novedades, `major` para
  cambios incompatibles. Gana el más alto de todos los fragmentos.
- Cuerpo: una frase en español, en imperativo. Será una viñeta del changelog.

Este README lo ignora la herramienta. Más detalle en `docs/RELEASING.md`.
