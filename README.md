# unha

Descargador de audio con UI. Tauri v2 + Vue 3 + Rust. Sin bundlear binarios:
`yt-dlp` y `ffmpeg` se descargan a `AppData/libs` en el primer arranque
(la crate [`yt-dlp`](https://docs.rs/yt-dlp) resuelve el binario adecuado
para cada plataforma → cross-compile a Windows y macOS sin fricción).

## Requisitos

- Node 18+, npm
- Rust stable + toolchain de cada target
- macOS: Xcode CLT (`xcode-select --install`)
- Windows (desde macOS): `rustup target add x86_64-pc-windows-msvc` + `cargo-xwin`

## Desarrollo

```bash
npm install
npm run tauri dev
```

Primera ejecución: la app descarga `yt-dlp` + `ffmpeg` a la carpeta de datos
de la app. Después se cachean.

## Build nativo

```bash
npm run tauri build
```

> `tauri.conf.json` tiene `bundle.active = false` para que el build no exija
> iconos. Cuando añadas iconos en `src-tauri/icons/`, pon `active: true` y
> añade la propiedad `"icon"` con las rutas.

## Cross-compile

Desde macOS a Windows (recomendado, `cargo-xwin`):

```bash
cargo install cargo-xwin
rustup target add x86_64-pc-windows-msvc
npm run tauri build -- --target x86_64-pc-windows-msvc --runner "cargo xwin"
```

macOS universal:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

Los binarios de `yt-dlp`/`ffmpeg` se descargan en tiempo de ejecución para la
plataforma correcta — el ejecutable resultante es pequeño y portable.

## Uso

1. Pega URL de vídeo o playlist.
2. `Analizar` → autodetecta single/playlist y lista pistas.
3. Elige formato (WAV, AIFF, MP3) y bitrate (solo MP3: 128/192/256/320 kbps).
4. Elige carpeta destino.
5. `Descargar` — progreso por pista vía eventos Tauri.

## Notas de seguridad

- Validación de esquema HTTP/HTTPS en la URL antes de invocar `yt-dlp`.
- Uso de `--` como separador en la CLI de `yt-dlp` para evitar que URLs
  con guiones se interpreten como flags.
- Argumentos siempre como `Vec<String>` sin shell → sin inyección.
- El directorio de destino se valida como directorio existente en Rust
  antes de lanzar la descarga.
- `--ignore-config` evita cargar configuración externa de `yt-dlp`.
- `csp: null` en dev; endurecer en producción con una CSP explícita.
