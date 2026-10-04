<script setup lang="ts">
// Accueil (planche 01) : zone de dépôt et documents récents en grille ou en liste.
import { FileText, FolderOpen, FormInput, LayoutGrid, List, Lock, MoreHorizontal, ShieldCheck, Upload } from "lucide-vue-next";
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import Dropdown from "./Dropdown.vue";
import type { RecentDoc } from "../bindings";
import { thumbUrl } from "../lib/protocol";
import { useRecents } from "../stores/recents";
import { useSettings } from "../stores/settings";
import { useTabs } from "../stores/tabs";
import { openFromDialog } from "../composables/open";
import { relativeDate } from "../lib/format";
import { kbd } from "../i18n/keys";

defineProps<{ dragging: boolean }>();
const { t, locale } = useI18n();
const recents = useRecents();
const settings = useSettings();
const tabs = useTabs();
const view = computed(() => settings.settings.recentsView);

onMounted(() => void recents.refresh());

function meta(r: RecentDoc) {
  const when = relativeDate(r.openedAt, locale.value as "fr" | "en");
  const what = r.locked ? t("home.protected") : t("home.pages", { n: r.pageCount }, r.pageCount);
  return `${when} · ${what}`;
}

function open(r: RecentDoc) {
  void tabs.openPaths([r.path]);
}
</script>

<template>
  <main class="home">
    <div class="wrap">
      <section class="drop" :class="{ over: dragging }" aria-labelledby="drop-title">
        <div class="dropic"><Upload class="ic xl" aria-hidden="true" /></div>
        <h2 id="drop-title">{{ t("home.dropTitle") }}</h2>
        <p>{{ t("home.dropHint") }}</p>
        <div class="actions">
          <button class="btn pri lg" @click="openFromDialog()">
            <FolderOpen class="ic s" aria-hidden="true" />{{ t("home.openFile") }}<span class="kbd">{{ kbd("Ctrl O") }}</span>
          </button>
        </div>
      </section>

      <section class="recents" aria-labelledby="recents-title">
        <div class="recent-h">
          <h3 id="recents-title">{{ t("home.recents") }}</h3>
          <div class="grow" />
          <div class="seg" role="radiogroup">
            <button :class="{ on: view === 'grid' }" role="radio" :aria-checked="view === 'grid'" :aria-label="t('home.gridView')" :title="t('home.gridView')" @click="settings.update({ recentsView: 'grid' })">
              <LayoutGrid class="ic s" aria-hidden="true" />
            </button>
            <button :class="{ on: view === 'list' }" role="radio" :aria-checked="view === 'list'" :aria-label="t('home.listView')" :title="t('home.listView')" @click="settings.update({ recentsView: 'list' })">
              <List class="ic s" aria-hidden="true" />
            </button>
          </div>
          <button class="btn gh" :disabled="!recents.items.length" @click="recents.clear()">{{ t("home.clearList") }}</button>
        </div>

        <p v-if="!recents.items.length" class="empty">{{ t("home.noRecents") }}</p>

        <ul v-else-if="view === 'grid'" class="grid">
          <li v-for="r in recents.items" :key="r.path" class="rc">
            <button class="rc-open" :title="r.path" @click="open(r)">
              <div class="rthumb">
                <img v-if="r.thumb" :src="`${thumbUrl(r.thumb)}?v=${recents.version}`" alt="" draggable="false" />
                <template v-else>
                  <div class="ln h" /><div class="ln" /><div class="ln" /><div class="ln" style="width: 78%" />
                  <div class="ln k" /><div class="ln" /><div class="ln" /><div class="ln" /><div class="ln" style="width: 64%" />
                  <div class="ln k" /><div class="ln" /><div class="ln" /><div class="ln" style="width: 52%" />
                </template>
                <div v-if="r.locked" class="lockc"><Lock class="ic xl" aria-hidden="true" /></div>
                <div v-if="r.signed" class="badge" :title="t('home.signed')"><ShieldCheck class="ic xs" :aria-label="t('home.signed')" /></div>
                <div v-else-if="r.form" class="badge" :title="t('home.form')"><FormInput class="ic xs" :aria-label="t('home.form')" /></div>
              </div>
              <div class="rname">{{ r.name }}</div>
              <div class="rmeta">{{ meta(r) }}</div>
            </button>
            <Dropdown class="more-dd" :width="200">
              <template #trigger="{ open: isOpen, toggle }">
                <button class="more" :class="{ shown: isOpen }" :aria-label="t('home.docOptions')" aria-haspopup="menu" :aria-expanded="isOpen" @click.stop="toggle()">
                  <MoreHorizontal class="ic s" aria-hidden="true" />
                </button>
              </template>
              <button class="mi" role="menuitem" @click="open(r)">{{ t("home.open") }}</button>
              <button class="mi" role="menuitem" @click="recents.remove(r.path)">{{ t("home.removeFromList") }}</button>
            </Dropdown>
          </li>
        </ul>

        <ul v-else class="list">
          <li v-for="r in recents.items" :key="r.path" class="row">
            <button class="row-open" :title="r.path" @click="open(r)">
              <span class="mini">
                <img v-if="r.thumb" :src="`${thumbUrl(r.thumb)}?v=${recents.version}`" alt="" draggable="false" />
                <Lock v-else-if="r.locked" class="ic xs" aria-hidden="true" />
                <FileText v-else class="ic xs" aria-hidden="true" />
              </span>
              <span class="row-main">
                <span class="rname">{{ r.name }}</span>
                <span class="rpath">{{ r.path }}</span>
              </span>
              <ShieldCheck v-if="r.signed" class="ic xs dim" :aria-label="t('home.signed')" />
              <FormInput v-else-if="r.form" class="ic xs dim" :aria-label="t('home.form')" />
              <span class="rmeta">{{ meta(r) }}</span>
            </button>
            <button class="cl" :aria-label="t('home.removeFromList')" :title="t('home.removeFromList')" @click="recents.remove(r.path)">×</button>
          </li>
        </ul>
      </section>
    </div>
  </main>
</template>

<style scoped>
.home { flex: 1; display: flex; flex-direction: column; align-items: center; padding: 52px 24px 32px; background: var(--bg); border-top: 1px solid var(--line); overflow: auto; }
.wrap { width: 100%; max-width: 900px; display: flex; flex-direction: column; gap: 36px; }
.drop { position: relative; min-height: 250px; border-radius: 16px; border: 1.5px dashed var(--line-2); background: var(--surface-2); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; text-align: center; padding: 24px; transition: background .12s, border-color .12s; }
:root.dark .drop { background: rgba(255, 255, 255, .03); }
.drop.over { border: 2px solid var(--accent); background: var(--accent-soft); }
.dropic { display: grid; place-items: center; width: 56px; height: 56px; border-radius: 16px; background: var(--accent-soft); color: var(--accent-text); margin-bottom: 6px; }
.drop h2 { margin: 0; font-size: 17px; font-weight: 600; }
.drop p { margin: 0; color: var(--text-2); }
.actions { display: flex; align-items: center; gap: 10px; margin-top: 10px; }
.recents { display: flex; flex-direction: column; gap: 18px; }
.recent-h { display: flex; align-items: center; gap: 10px; }
.recent-h h3 { margin: 0; font-size: 13px; font-weight: 600; }
.empty { margin: 0; color: var(--text-3); }
ul { list-style: none; margin: 0; padding: 0; }

.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(126px, 1fr)); gap: 20px; }
.rc { position: relative; min-width: 0; }
.rc-open { display: flex; flex-direction: column; gap: 9px; width: 100%; min-width: 0; border-radius: 10px; padding: 8px; margin: -8px; border: 0; background: transparent; text-align: left; width: calc(100% + 16px); }
.rc-open:hover, .rc:hover .rc-open { background: var(--hover); }
.rthumb { position: relative; height: 178px; background: #fff; border-radius: 4px; box-shadow: 0 0 0 1px rgba(0, 0, 0, .09), 0 2px 8px rgba(0, 0, 0, .08); padding: 16px 14px; display: flex; flex-direction: column; gap: 5px; overflow: hidden; }
.rthumb img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; object-position: top; }
.ln { height: 4px; border-radius: 2px; background: #d6d6dc; flex: none; }
.ln.h { height: 6px; width: 62%; background: #8f8f98; }
.ln.k { height: 4px; width: 40%; background: #a7a7b0; margin-top: 6px; }
.badge { position: absolute; top: 8px; right: 8px; display: grid; place-items: center; width: 24px; height: 24px; border-radius: 7px; background: rgba(25, 25, 30, .78); color: #fff; }
.lockc { position: absolute; inset: 0; display: grid; place-items: center; background: rgba(240, 240, 244, .6); backdrop-filter: blur(3px); color: #4a4a52; }
.rname { font-size: 12.5px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.rmeta { font-size: 11.5px; color: var(--text-3); }
.rc-open .rmeta { margin-top: -6px; }
.more-dd { position: absolute; top: 6px; left: 6px; }
.more { display: grid; place-items: center; width: 26px; height: 26px; border-radius: 7px; background: var(--surface); color: var(--text); border: 0; box-shadow: 0 0 0 1px var(--line), 0 2px 6px rgba(0, 0, 0, .12); padding: 0; opacity: 0; }
.rc:hover .more, .more.shown, .more:focus-visible { opacity: 1; }

.list { display: flex; flex-direction: column; gap: 2px; }
.row { display: flex; align-items: center; gap: 6px; border-radius: 8px; }
.row:hover { background: var(--hover); }
.row-open { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; padding: 6px 8px; border: 0; background: transparent; text-align: left; border-radius: 8px; }
.mini { width: 28px; height: 36px; flex: none; display: grid; place-items: center; background: #fff; border-radius: 3px; box-shadow: 0 0 0 1px rgba(0, 0, 0, .09); overflow: hidden; color: #696973; }
.mini img { width: 100%; height: 100%; object-fit: cover; object-position: top; }
.row-main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.rpath { font-size: 11.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; direction: rtl; text-align: left; }
.dim { color: var(--text-3); }
.row .cl { width: 26px; height: 26px; margin-right: 6px; font-size: 16px; opacity: 0; }
.row:hover .cl, .row .cl:focus-visible { opacity: 1; }
</style>
