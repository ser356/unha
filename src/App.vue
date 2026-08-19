<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { downloadDir } from "@tauri-apps/api/path";

type Entry = { index: number; id: string; title: string; url: string | null; thumbnail: string | null };
type Probe = { kind: "single" | "playlist"; title: string; entries: Entry[] };
type Format = "wav" | "aiff" | "mp3";
type Bitrate = 128 | 192 | 256 | 320;
type ItemProgress = { index: number; percent: number; status: string; title: string };

const url = ref("");
const format = ref<Format>("mp3");
const bitrate = ref<Bitrate>(320);
const dest = ref<string | null>(null);
const probe = ref<Probe | null>(null);
const selected = ref<Set<number>>(new Set());
const progress = ref<Map<number, ItemProgress>>(new Map());
const finished = ref<Set<number>>(new Set());
const busy = ref(false);
const probing = ref(false);
const err = ref<string | null>(null);
const libsReady = ref(false);
const enteringRows = ref<Set<number>>(new Set());

let unlistenProgress: UnlistenFn | null = null;
let unlistenLog: UnlistenFn | null = null;

const canProbe = computed(() => !!url.value.trim() && !busy.value && libsReady.value);
const canDownload = computed(
  () => !!probe.value && !!dest.value && !busy.value && libsReady.value && selected.value.size > 0,
);

const bodyPhase = computed<"idle" | "active" | "done" | "error">(() => {
  if (err.value) return "error";
  if (busy.value) return "active";
  if (probe.value && finished.value.size >= probe.value.entries.length && finished.value.size > 0)
    return "done";
  return "idle";
});
watch(
  bodyPhase,
  (p) => {
    document.body.classList.remove("phase-active", "phase-done", "phase-error");
    if (p !== "idle") document.body.classList.add(`phase-${p}`);
  },
  { immediate: true },
);

function pushLog(_line: string) {
  // silencio: los logs de yt-dlp no visten esta UI. Se conservan a nivel Rust para debug.
}

function rowStateClass(idx: number): string {
  if (err.value && busy.value === false) return "error";
  if (finished.value.has(idx)) return "done";
  if (!busy.value) return "idle";
  const p = progress.value.get(idx);
  if (!p) return "queued";
  if (p.percent >= 99.9) return "done";
  return "active";
}

function rowPct(idx: number): number {
  if (finished.value.has(idx)) return 100;
  return progress.value.get(idx)?.percent ?? 0;
}

function initials(title: string): string {
  const words = title.trim().split(/\s+/).filter(Boolean);
  const first = words[0]?.[0] ?? "•";
  const last = words.length > 1 ? words[words.length - 1][0] : "";
  return (first + last).toUpperCase().slice(0, 2);
}

function folderName(p: string | null): string {
  if (!p) return "";
  const cleaned = p.replace(/[\/\\]+$/, "");
  const idx = Math.max(cleaned.lastIndexOf("/"), cleaned.lastIndexOf("\\"));
  return idx >= 0 ? cleaned.slice(idx + 1) : cleaned;
}

async function chooseDest() {
  const picked = await open({ directory: true, multiple: false });
  if (typeof picked === "string") dest.value = picked;
}

async function ensureLibs() {
  try {
    await invoke("ensure_libs");
    libsReady.value = true;
    try {
      await invoke<string>("ytdlp_version");
    } catch (e) {
      pushLog(String(e));
    }
  } catch (e) {
    err.value = String(e);
  }
}

async function doProbe() {
  err.value = null;
  probe.value = null;
  selected.value = new Set();
  progress.value = new Map();
  finished.value = new Set();
  busy.value = true;
  probing.value = true;
  try {
    const result = await invoke<Probe>("probe", {
      url: url.value.trim(),
      cookiesFromBrowser: null,
    });
    probe.value = result;
    selected.value = new Set(result.entries.map((e) => e.index));
    enteringRows.value = new Set(result.entries.map((e) => e.index));
    setTimeout(() => (enteringRows.value = new Set()), 800);
  } catch (e) {
    err.value = friendlyErr(String(e));
  } finally {
    busy.value = false;
    probing.value = false;
  }
}

async function doDownload() {
  if (!probe.value || !dest.value) return;
  err.value = null;
  busy.value = true;
  progress.value = new Map();
  finished.value = new Set();
  const indices = [...selected.value].sort((a, b) => a - b);
  try {
    await invoke("download", {
      url: url.value.trim(),
      format: format.value,
      mp3Bitrate: format.value === "mp3" ? bitrate.value : null,
      destDir: dest.value,
      indices,
      cookiesFromBrowser: null,
    });
    finished.value = new Set(indices);
  } catch (e) {
    err.value = friendlyErr(String(e));
  } finally {
    busy.value = false;
  }
}

function friendlyErr(raw: string): string {
  if (/403|Forbidden/i.test(raw))
    return `YouTube rechazó la descarga. Reintenta en un momento — se refresca yt-dlp automáticamente.`;
  if (/URL inválida|esquema/i.test(raw)) return `Esa URL no vale. Pega el enlace completo.`;
  if (/carpeta destino/i.test(raw)) return `Elige carpeta destino antes de descargar.`;
  return raw.replace(/^Error:\s*/, "");
}

function toggle(idx: number) {
  if (selected.value.has(idx)) selected.value.delete(idx);
  else selected.value.add(idx);
  selected.value = new Set(selected.value);
}

async function onEnter(ev: KeyboardEvent) {
  if (ev.isComposing) return;
  if (canProbe.value) await doProbe();
}

onMounted(async () => {
  unlistenProgress = await listen<ItemProgress>("dl://progress", (evt) => {
    const p = evt.payload;
    progress.value.set(p.index, p);
    progress.value = new Map(progress.value);
    if (p.percent >= 99.9) finished.value.add(p.index);
  });
  unlistenLog = await listen<string>("dl://log", (evt) => pushLog(evt.payload));
  if (!dest.value) {
    try { dest.value = await downloadDir(); } catch { /* sin permiso: usuario elige */ }
  }
  await ensureLibs();
});

onUnmounted(() => {
  unlistenProgress?.();
  unlistenLog?.();
});
</script>

<template>
  <div class="dragzone" data-tauri-drag-region></div>

  <main class="shell">
    <section class="stage" data-tauri-drag-region>
      <!-- HERO: título + input pill -->
      <div class="hero">
        <h2 v-if="!probe">Pega el <em>enlace</em>.</h2>
        <h2 v-else>{{ probe.entries.length }} pista<span v-if="probe.entries.length !== 1">s</span> preparada<span v-if="probe.entries.length !== 1">s</span>.</h2>

        <div class="url-pill">
          <input
            v-model="url"
            type="url"
            spellcheck="false"
            autocomplete="off"
            :placeholder="probe ? 'Pega otra URL para reemplazar…' : 'https://youtube.com/…'"
            @keydown.enter.prevent="onEnter"
          />
          <button class="btn-cta ghost" :disabled="!canProbe || probing" @click="doProbe">
            {{ probing ? "Analizando…" : probe ? "Cambiar" : "Empezar" }}
          </button>
        </div>

        <!-- Segmented controls: formato + bitrate -->
        <div class="controls-row">
          <div class="seg" role="tablist" aria-label="Formato">
            <button
              v-for="f in (['wav', 'aiff', 'mp3'] as Format[])"
              :key="f"
              :aria-pressed="format === f"
              :disabled="busy"
              @click="format = f"
            >
              {{ f.toUpperCase() }}
            </button>
          </div>

          <div v-if="format === 'mp3'" class="seg" role="tablist" aria-label="Bitrate">
            <button
              v-for="b in ([128, 192, 256, 320] as Bitrate[])"
              :key="b"
              :aria-pressed="bitrate === b"
              :disabled="busy"
              @click="bitrate = b"
            >
              {{ b }}
            </button>
          </div>
        </div>

        <!-- Destino: CTA cuando vacío, chip cuando lleno -->
        <button v-if="!dest" class="btn-cta dest-cta" @click="chooseDest">
          <svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.6l2 2H18.5A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5z" stroke-linejoin="round"/>
          </svg>
          <span>Elegir carpeta destino</span>
        </button>
        <div v-else class="dest-shown">
          <svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.6l2 2H18.5A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5z" stroke-linejoin="round"/>
          </svg>
          <span class="name" :title="dest">{{ folderName(dest) }}</span>
          <button class="change" @click="chooseDest">Cambiar</button>
        </div>
      </div>

      <!-- ERROR -->
      <div v-if="err" class="err-card">
        <strong>Ups —</strong> {{ err }}
      </div>

      <!-- LISTA de pistas -->
      <div v-if="probe && probe.entries.length" class="tracks">
        <div class="tracks-head">
          <span>{{ selected.size }} de {{ probe.entries.length }} elegidas</span>
        </div>

        <div
          v-for="e in probe.entries"
          :key="e.index"
          class="track"
          :class="[rowStateClass(e.index), { enter: enteringRows.has(e.index) }]"
          :style="{ '--pct': rowPct(e.index) + '%' }"
        >
          <div class="avatar">
            <img
              v-if="e.thumbnail"
              :src="e.thumbnail"
              :alt="e.title"
              loading="lazy"
              referrerpolicy="no-referrer"
              @error="($event.target as HTMLImageElement).style.display = 'none'"
            />
            <span v-else>{{ initials(e.title) }}</span>
          </div>
          <div class="body">
            <div class="title" :title="e.title">{{ e.title }}</div>
            <div class="sub">#{{ String(e.index).padStart(2, "0") }} · {{ format === "mp3" ? `MP3 ${bitrate}` : format.toUpperCase() }}</div>
          </div>
          <div class="pct" v-if="busy || finished.has(e.index)">
            {{ Math.round(rowPct(e.index)) }}%
          </div>
          <button
            class="pick"
            :aria-pressed="selected.has(e.index)"
            :aria-label="`Elegir ${e.title}`"
            :disabled="busy"
            @click="toggle(e.index)"
          ></button>
        </div>

        <div style="display:flex; justify-content:center; margin-top:6px;">
          <button class="btn-cta" :disabled="!canDownload" @click="doDownload">
            {{ busy ? "Bajando…" : `Descargar${selected.size ? ` · ${selected.size}` : ""}` }}
          </button>
        </div>
      </div>

      <!-- SKELETON mientras analiza -->
      <div v-else-if="probing" class="tracks skeleton" aria-hidden="true">
        <div class="tracks-head"><span>Buscando pistas…</span></div>
        <div v-for="n in 3" :key="n" class="track skel">
          <div class="avatar"></div>
          <div class="body">
            <div class="bar w-title"></div>
            <div class="bar w-meta"></div>
          </div>
        </div>
      </div>

      <!-- EMPTY -->
      <div v-else class="empty">
        <h3>Sin cola todavía</h3>
        <p>Pega una URL arriba y pulsa Enter.</p>
      </div>
    </section>
  </main>
</template>
