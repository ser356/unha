#!/usr/bin/env bash
#
# publish.sh — release local de unha desde macOS.
#
# Camino feliz:
#   1. Preflight (herramientas, árbol limpio, rama main).
#   2. Bump semver en Cargo.toml + tauri.conf.json + package.json.
#   3. Commit + tag + push (esto dispara `.github/workflows/release.yml`,
#      que compila Windows en CI y sube el .exe a la Release).
#   4. Build local del .app macOS (aarch64) con cargo tauri.
#   5. Ditto zip preservando metadatos.
#   6. gh release create/upload con el zip macOS + sha256 (crea la
#      Release si el CI de Windows aún no la creó; usa --clobber si sí).
#
# Uso:
#   ./publish.sh patch          # 0.1.0 → 0.1.1
#   ./publish.sh minor          # 0.1.0 → 0.2.0
#   ./publish.sh major          # 0.1.0 → 1.0.0
#   ./publish.sh 1.2.0          # versión explícita
#
# Requisitos: gh (autenticado), cargo, cargo-tauri, ditto, shasum,
# target aarch64-apple-darwin, remote `origin` configurado.

set -euo pipefail

# ─── Config ────────────────────────────────────────────────────────
TARGET_TRIPLE="aarch64-apple-darwin"
APP_NAME="unha.app"                # coincide con productName en tauri.conf.json
BINARY_NAME="unha"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_ROOT"

# ─── Utilidades ────────────────────────────────────────────────────
section() { printf '\n\033[1;36m━━━ %s ━━━\033[0m\n' "$*"; }
die()     { printf '\033[1;31m[publish] %s\033[0m\n' "$*" >&2; exit 1; }

sed_inplace() {
    local file="$1" expr="$2"
    sed -E -i.bak "$expr" "$file"
    rm -f "${file}.bak"
}

# ─── Args ──────────────────────────────────────────────────────────
if [[ $# -ne 1 ]]; then
    echo "uso: $0 <patch|minor|major|X.Y.Z>" >&2
    exit 2
fi
BUMP_KIND="$1"

# ─── 1. Preflight ──────────────────────────────────────────────────
section "1. Preflight"

command -v gh          >/dev/null || die "gh no está en PATH"
command -v cargo       >/dev/null || die "cargo no está en PATH"
command -v cargo-tauri >/dev/null || die "cargo-tauri no instalado (\`cargo install tauri-cli --locked --version '^2'\`)"
command -v shasum      >/dev/null || die "shasum no está en PATH"
command -v ditto       >/dev/null || die "ditto no está en PATH (necesitas macOS)"

gh auth status >/dev/null 2>&1 || die "gh no autenticado (\`gh auth login\`)"

rustup target list --installed | grep -q "^${TARGET_TRIPLE}$" \
    || die "target ${TARGET_TRIPLE} no instalado (\`rustup target add ${TARGET_TRIPLE}\`)"

if ! git diff --quiet || ! git diff --cached --quiet; then
    git status --short
    die "árbol git sucio — commitea o stashea antes"
fi

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$CURRENT_BRANCH" != "main" && "${UNHA_PUBLISH_FORCE_BRANCH:-0}" != "1" ]]; then
    die "no estás en main (estás en '$CURRENT_BRANCH'); usa UNHA_PUBLISH_FORCE_BRANCH=1 para saltar"
fi

git remote get-url origin >/dev/null 2>&1 \
    || die "no hay remote 'origin' configurado"

ORIGIN_URL="$(git remote get-url origin | sed -E 's#(git@github\.com:|https://github\.com/)([^/]+/[^/.]+).*#\2#')"
echo "  rama:   ${CURRENT_BRANCH}"
echo "  repo:   ${ORIGIN_URL}"

# ─── 2. Bump versión ───────────────────────────────────────────────
section "2. Bump versión"

CARGO_TOML="src-tauri/Cargo.toml"
TAURI_CONF="src-tauri/tauri.conf.json"
PKG_JSON="package.json"

CURRENT="$(awk '/^\[package\]/{p=1;next} /^\[/{p=0} p && /^version[[:space:]]*=/{gsub(/"/,"",$3); print $3; exit}' "$CARGO_TOML")"
[[ "$CURRENT" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "no pude leer versión actual de ${CARGO_TOML} (leí '$CURRENT')"

IFS='.' read -r MAJ MIN PAT <<< "$CURRENT"
case "$BUMP_KIND" in
    patch) NEW="${MAJ}.${MIN}.$((PAT + 1))" ;;
    minor) NEW="${MAJ}.$((MIN + 1)).0" ;;
    major) NEW="$((MAJ + 1)).0.0" ;;
    *)
        [[ "$BUMP_KIND" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] \
            || die "versión inválida: '$BUMP_KIND'"
        NEW="$BUMP_KIND"
        ;;
esac

TAG="v${NEW}"
if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
    die "el tag ${TAG} ya existe local; borra con \`git tag -d ${TAG}\` si es dangling"
fi
if git ls-remote --tags origin "${TAG}" | grep -q "${TAG}"; then
    die "el tag ${TAG} ya existe en origin — bump a otra versión"
fi

echo "  ${CURRENT} → ${NEW}   (tag: ${TAG})"

sed_inplace "$CARGO_TOML"  "s/^version = \"${CURRENT}\"/version = \"${NEW}\"/"
sed_inplace "$TAURI_CONF"  "s/\"version\": \"${CURRENT}\"/\"version\": \"${NEW}\"/"
if [[ -f "$PKG_JSON" ]] && grep -q "\"version\": \"${CURRENT}\"" "$PKG_JSON"; then
    sed_inplace "$PKG_JSON" "s/\"version\": \"${CURRENT}\"/\"version\": \"${NEW}\"/"
fi

( cd src-tauri && cargo check --quiet )
echo "  Cargo.lock regenerado"

# ─── 3. Commit + tag + push (dispara CI de Windows) ────────────────
section "3. Commit + tag + push"

git add "$CARGO_TOML" "$TAURI_CONF" src-tauri/Cargo.lock
[[ -f "$PKG_JSON" ]] && git add "$PKG_JSON" || true

git commit --no-verify -m "chore(release): ${TAG}"
git tag -a "${TAG}" -m "${TAG}"
git push
git push --tags

echo "  → CI de Windows arrancando en:"
echo "    https://github.com/${ORIGIN_URL}/actions/workflows/release.yml"

# ─── 4. Build local macOS ──────────────────────────────────────────
section "4. Build local macOS (${TARGET_TRIPLE})"

STALE_APP="src-tauri/target/${TARGET_TRIPLE}/release/bundle/macos/${APP_NAME}"
if [[ -d "$STALE_APP" ]]; then
    echo "  removing stale ${STALE_APP}"
    rm -rf "$STALE_APP"
fi

# Fuerza recompilación del binario para que `--version` refleje el
# nuevo Cargo.toml. Sin esto cargo re-usa el binario compilado con
# la versión anterior.
( cd src-tauri && cargo clean -p "${BINARY_NAME}" 2>/dev/null || true )

echo "  npm ci"
npm ci --silent
echo "  npm run build"
npm run build

echo "  cargo tauri build --target ${TARGET_TRIPLE} --bundles app"
( cd src-tauri && cargo tauri build --target "${TARGET_TRIPLE}" --bundles app )

APP_PATH="src-tauri/target/${TARGET_TRIPLE}/release/bundle/macos/${APP_NAME}"
[[ -d "$APP_PATH" ]] || die "no encontré ${APP_PATH}"

# ─── 5. Empaquetar ZIP ─────────────────────────────────────────────
section "5. Empaquetar ZIP"

ZIP_NAME="unha-${TAG}-macos-arm64.zip"
rm -f "${ZIP_NAME}" "${ZIP_NAME}.sha256"
ditto -c -k --sequesterRsrc --keepParent "$APP_PATH" "$ZIP_NAME"

ZIP_SIZE="$(du -h "$ZIP_NAME" | cut -f1)"
ZIP_SHA="$(shasum -a 256 "$ZIP_NAME" | awk '{print $1}')"
echo "${ZIP_SHA}  ${ZIP_NAME}" > "${ZIP_NAME}.sha256"
echo "  ${ZIP_NAME}  (${ZIP_SIZE})"
echo "  sha256: ${ZIP_SHA}"

# ─── 6. Subir a GitHub Release ─────────────────────────────────────
section "6. Subir a GitHub Release"

# Si CI de Windows ya creó la Release, usamos upload --clobber.
# Si no, la creamos nosotros — el CI hará upload cuando termine.
if gh release view "${TAG}" >/dev/null 2>&1; then
    echo "  release ${TAG} ya existe (posiblemente creada por CI) — subiendo con --clobber"
    gh release upload "${TAG}" "${ZIP_NAME}" "${ZIP_NAME}.sha256" --clobber
else
    echo "  creando release ${TAG}"
    gh release create "${TAG}" "${ZIP_NAME}" "${ZIP_NAME}.sha256" \
        --title "unha ${TAG}" \
        --generate-notes
fi

# ─── 7. Cleanup ────────────────────────────────────────────────────
section "7. Cleanup"
rm -f "${ZIP_NAME}" "${ZIP_NAME}.sha256"
echo "  removed ${ZIP_NAME}"

# ─── Fin ───────────────────────────────────────────────────────────
section "✅ Release ${TAG} publicada (macOS)"
echo "  release: https://github.com/${ORIGIN_URL}/releases/tag/${TAG}"
echo "  el .exe de Windows aparecerá cuando termine el CI:"
echo "    https://github.com/${ORIGIN_URL}/actions/workflows/release.yml"
