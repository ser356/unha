# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

DJs y músicos no técnicos que necesitan audio de YouTube (temas sueltos, sets, álbumes completos en formato playlist) para pinchar. Trabajan sobre macOS, arrastran los ficheros a Rekordbox, Serato, rekordbox-USB, o carpetas de biblioteca de iTunes/Music. No abren la terminal. No conocen `yt-dlp`.

## Product Purpose

Convertir una URL de YouTube (vídeo suelto o playlist) en ficheros de audio DJ-ready (WAV / AIFF / MP3 con bitrate elegido), en la carpeta que el usuario indique, con metadatos ID3 (título, artista, álbum, año, cover art cuando existe) rellenados automáticamente a partir de la ficha del vídeo. Éxito = el fichero aparece en Rekordbox/Serato con título y artista correctos sin que el usuario los haya tecleado.

## Positioning

Wrapper GUI de `yt-dlp` diseñado explícitamente para el público que hoy no puede usar `yt-dlp`: el que se rinde ante la terminal y el que descarta las apps GUI existentes por feas, obsoletas, de pago o porque generan ficheros sin metadatos y hay que renombrar a mano. Ventaja no copiable trivialmente por vecinos: `yt-dlp` nightly bundleado y auto-actualizado dentro de la app + tags ID3 automáticos + look nativo mac, sin login, sin cuenta, sin CLI.

## Operating Context

Escenario típico: DJ prepara un set el día antes de un bolo. Copia una URL de YouTube desde Safari/Chrome, la pega en unha, elige formato (MP3 320 kbps para uso rápido, WAV/AIFF para producción), elige carpeta destino (Desktop, carpeta de librería, un USB montado), le da a Descargar. Cuando termina, arrastra los ficheros al software DJ, ya con los tags rellenos. Puede repetir esto varias veces por sesión y con playlists de 50+ pistas.

## Capabilities and Constraints

Confirmado ya implementado:
- Detección automática single vs playlist (`probe` con `yt-dlp --dump-single-json --flat-playlist`).
- Selección de pistas concretas en una playlist.
- Formatos WAV, AIFF, MP3 (128 / 192 / 256 / 320 kbps).
- Selector opcional de cookies del navegador (Safari / Chrome / Firefox / Edge / Brave) para vídeos protegidos.
- Elegir carpeta destino con diálogo nativo.
- Barra de progreso por pista y log crudo de `yt-dlp`.
- `yt-dlp` nightly bundleado en `AppData/libs/yt-dlp`, auto-refresh cada 24h desde `yt-dlp/yt-dlp-nightly-builds`, con botón de refresh manual.
- `ffmpeg` bundleado (fallback al instalador del crate `yt-dlp` si no hay uno de sistema).

Pendiente / a añadir (compromiso confirmado con el usuario):
- Metadatos ID3 automáticos obligatorios (título, artista, álbum, año, cover art embebido). No opt-in: se aplican siempre en formatos que lo soportan (MP3, AIFF; WAV no lleva ID3 canónico, decidir si RIFF INFO o dejar sin tags).

Restricciones técnicas:
- Backend Rust con Tauri v2, frontend Vue 3 + TS + Vite.
- Cross-compile objetivo: macOS (Intel + Apple Silicon) y Windows x86_64.
- La app no debe requerir `yt-dlp` o `ffmpeg` en el PATH del usuario final.
- La app no debe pedir login ni credenciales para casos no autenticados; las cookies del navegador son opt-in por sesión, no persistidas por unha.
- Sandbox macOS + Hardened Runtime al firmar: red saliente permitida (nightly download, tráfico de `yt-dlp`).

Undecided / no confirmar todavía:
- Empaquetado: DMG firmado + notarizado (macOS) y MSI (Windows). Certificados de firma no adquiridos.
- Actualización de la propia app (auto-update binario unha, distinta del auto-update de yt-dlp). No decidido si se usa el updater de Tauri.
- Historial persistente de descargas / reintentos automáticos.
- Cola con pausa/reanudación.

## Brand Commitments

- Nombre: `unha`. Origen no confirmado por el usuario en esta sesión; conservar en minúsculas.
- Look confirmado: **macOS-native**. Se espera translucidez (materiales del sistema, tipo `NSVisualEffectView` — en Tauri v2 se logra vía `windowEffects: ["hudWindow" | "sidebar" | "menu"]` en `tauri.conf.json` + `titleBarStyle: "Overlay"`). Controles del sistema (SF Symbols o glifos equivalentes, tipografía SF, spacings y radios acordes a HIG de macOS).
- Sin logotipo definitivo, sin paleta corporativa fijada, sin material gráfico existente. Cualquier decisión visual queda a resolver en `new-work` sobre este brief.

## Evidence on Hand

Ninguna. No hay copy oficial, ni logotipos, ni capturas de marketing, ni testimonios, ni benchmarks, ni cuentas de descargas, ni comparativas publicadas. Futuro trabajo NO debe inventar métricas, testimonios de DJs, marcas de eventos, ni compatibilidades con software concreto (Rekordbox, Serato) más allá del hecho técnico de que MP3/WAV/AIFF con tags ID3 funcionan universalmente en ellos.

## Product Principles

1. **Cero terminal, cero cuenta.** Un usuario que nunca abrió `yt-dlp` ni sabe qué es un binario debe terminar una descarga en menos de 30 segundos desde primer arranque.
2. **Fichero DJ-ready o no vale.** Un MP3 sin título ni artista es basura para pinchar; unha nunca entrega audio sin metadatos cuando el formato los admite.
3. **La app se cura sola.** yt-dlp y ffmpeg son responsabilidad de unha, no del usuario. Rotos por YouTube = un clic (o cero) para recuperarse.
4. **Look mac nativo, no "web app oscura genérica".** Ventana con material del sistema, tipografía SF, spacing y densidad de una app AppKit, no de una landing.
5. **Fallar en voz alta, no a medias.** Un error 403, una URL inválida o una carpeta sin permisos se comunican con lenguaje humano; no se esconden y no se silencian con reintentos ciegos.

## Accessibility & Inclusion

No confirmado en esta sesión. Como app dirigida a público general no técnico, adherirse por defecto a lo razonable en macOS: contraste AA sobre materiales translúcidos, targets táctiles ≥ 24 px, respeto a Reduce Motion y Dark Mode del sistema, foco visible. Preguntar al usuario si hay un estándar concreto (WCAG 2.2 AA, algo específico de cliente) antes de comprometerse por escrito.
