# Pilas

La pilita del notch que te avisa cuando Claude Code te necesita. Aprueba permisos, responde preguntas y salta a
la terminal correcta sin cambiar de ventana.

![La pilita](assets/icon.png)

> En desarrollo. Ver `docs/SPEC.md` para el alcance de la v1 y `docs/PLAN.md` para el avance.

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

## Licencia

MIT. Ver `LICENSE`.
