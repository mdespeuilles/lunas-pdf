<script setup lang="ts">
// Document protégé (planche 11).
import { Eye, EyeOff, Lock, TriangleAlert } from "lucide-vue-next";
import { nextTick, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { type DocTab, useTabs } from "../stores/tabs";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const tabs = useTabs();
const password = ref("");
const show = ref(false);
const remember = ref(false);
const busy = ref(false);
const input = ref<HTMLInputElement>();

onMounted(() => input.value?.focus());
watch(
  () => props.tab.wrongPassword,
  async (w) => {
    if (w) {
      await nextTick();
      input.value?.select();
    }
  },
);

async function submit() {
  if (!password.value || busy.value) return;
  busy.value = true;
  await tabs.unlock(props.tab, password.value, remember.value);
  busy.value = false;
}
</script>

<template>
  <div class="locked">
    <div class="page" aria-hidden="true">
      <div class="ln h" /><div class="ln" /><div class="ln" style="width: 80%" /><div class="ln k" /><div class="ln" /><div class="ln" />
      <div class="ln" style="width: 66%" /><div class="ln k" /><div class="ln" /><div class="ln" /><div class="ln" /><div class="ln" style="width: 50%" />
    </div>
    <div class="over">
      <form class="card" role="dialog" aria-modal="true" aria-labelledby="lk-title" aria-describedby="lk-body" @submit.prevent="submit">
        <span class="lic"><Lock class="ic l" aria-hidden="true" /></span>
        <h2 id="lk-title">{{ t("unlock.title") }}</h2>
        <p id="lk-body">{{ t("unlock.body", { name: tab.name }) }}</p>
        <div class="inp" :class="{ bad: tab.wrongPassword }">
          <input
            ref="input"
            v-model="password"
            :type="show ? 'text' : 'password'"
            autocomplete="current-password"
            :aria-label="t('unlock.password')"
            :aria-invalid="tab.wrongPassword"
            :aria-describedby="tab.wrongPassword ? 'lk-err' : undefined"
            @input="tab.wrongPassword = false"
          />
          <button type="button" :aria-label="show ? t('unlock.hide') : t('unlock.show')" :title="show ? t('unlock.hide') : t('unlock.show')" @click="show = !show">
            <EyeOff v-if="show" class="ic s" aria-hidden="true" />
            <Eye v-else class="ic s" aria-hidden="true" />
          </button>
        </div>
        <span v-if="tab.wrongPassword" id="lk-err" class="err" role="alert">
          <TriangleAlert class="ic xs" aria-hidden="true" />{{ t("unlock.wrong") }}
        </span>
        <label class="row remember">
          <input v-model="remember" type="checkbox" class="sr-only" />
          <span class="cb" :class="{ on: remember }" aria-hidden="true">
            <svg v-if="remember" class="ic xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6L9 17l-5-5" /></svg>
          </span>
          {{ t("unlock.remember") }}
        </label>
        <div class="row actions">
          <button type="button" class="btn gh" @click="tabs.close(tab.key)">{{ t("unlock.closeTab") }}</button>
          <button type="submit" class="btn pri" :disabled="!password || busy">{{ t("unlock.unlock") }}</button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.locked { flex: 1; position: relative; overflow: hidden; background: var(--canvas); display: flex; flex-direction: column; align-items: center; }
.page { flex: none; width: 600px; height: 848px; margin-top: 28px; background: #fff; padding: 52px 66px; display: flex; flex-direction: column; gap: 8px; filter: blur(7px); opacity: .75; }
:root.dark .page { opacity: .25; }
.ln { height: 6px; border-radius: 3px; background: #cfcfd6; flex: none; }
.ln.h { height: 14px; width: 60%; background: #8f8f98; margin-bottom: 10px; }
.ln.k { width: 35%; background: #a7a7b0; margin-top: 14px; }
.over { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; padding: 16px; }
.card { width: 400px; max-width: 100%; padding: 30px 30px 24px; border-radius: 16px; background: var(--surface); box-shadow: 0 0 0 1px var(--line), var(--shadow-lg); display: flex; flex-direction: column; align-items: center; text-align: center; gap: 6px; }
.lic { display: grid; place-items: center; width: 56px; height: 56px; border-radius: 16px; background: var(--accent-soft); color: var(--accent-text); margin-bottom: 10px; }
.card h2 { margin: 0; font-size: 17px; font-weight: 600; }
.card p { margin: 0 0 14px; color: var(--text-2); font-size: 13px; line-height: 1.5; }
.inp { display: flex; align-items: center; gap: 8px; width: 100%; height: 36px; border-radius: 8px; background: var(--surface); color: var(--text); padding: 0 6px 0 12px; box-shadow: inset 0 0 0 1px var(--line-2); }
.inp:focus-within { box-shadow: inset 0 0 0 1px var(--accent), var(--ring); }
.inp.bad { box-shadow: inset 0 0 0 1px var(--bad), 0 0 0 3px color-mix(in oklab, var(--bad) 22%, transparent); }
.inp input { flex: 1; min-width: 0; border: 0; background: transparent; color: inherit; font: inherit; font-size: 14px; letter-spacing: .14em; padding: 0; }
.inp input:focus-visible { box-shadow: none; }
.inp button { display: grid; place-items: center; width: 26px; height: 26px; border-radius: 6px; border: 0; background: transparent; color: var(--text-3); padding: 0; }
.inp button:hover { background: var(--hover); color: var(--text); }
.err { display: flex; align-items: center; gap: 6px; align-self: flex-start; margin-top: 6px; font-size: 12.5px; color: var(--bad); text-align: left; }
.row { display: flex; align-items: center; gap: 8px; }
.remember { align-self: flex-start; margin-top: 12px; font-size: 12.5px; color: var(--text-2); }
.remember:focus-within .cb { box-shadow: inset 0 0 0 1.5px var(--line-2), var(--ring); }
.actions { align-self: stretch; justify-content: flex-end; margin-top: 18px; }
.actions .btn { height: 32px; padding: 0 16px; }
</style>
