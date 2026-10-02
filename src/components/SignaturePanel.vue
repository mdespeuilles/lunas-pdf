<script setup lang="ts">
// Panneau « Signature numérique » (planche 05) : état, signataire, certificat.
import { CircleCheck, CircleHelp, CircleX, ShieldCheck, ShieldPlus, ShieldQuestion, ShieldX, X } from "lucide-vue-next";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { SignatureInfo } from "../bindings";
import { shortDate, shortFingerprint, sigDate } from "../composables/signatures";
import { type DocTab, useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";
import { commands, unwrap } from "../lib/api";
import Modal from "./Modal.vue";

const props = defineProps<{ tab: DocTab }>();
const { t, locale } = useI18n();
const tabs = useTabs();
const ui = useUi();
const list = computed(() => props.tab.signatures ?? []);
const index = computed(() => Math.min(props.tab.sigSelected, Math.max(0, list.value.length - 1)));
const sig = computed<SignatureInfo | undefined>(() => list.value[index.value]);
const certOpen = ref(false);
const trustOpen = ref(false);

/** Approuve l'autorité de la signature affichée, puis revérifie tous les documents. */
async function trust() {
  const s = sig.value;
  trustOpen.value = false;
  if (!s || !props.tab.info) return;
  try {
    const root = await unwrap(commands.trustSignatureRoot(props.tab.info.id, s.field));
    await tabs.reloadAllSignatures();
    ui.notify(t("sig.trustDone", { name: root.name }));
  } catch (e) {
    ui.notify(String(e));
  }
}

function statusText(s: SignatureInfo) {
  if (s.status === "valid") return { title: t("sig.status.valid"), text: s.coversWhole ? t("sig.status.validText") : t("sig.status.validLaterText") };
  if (s.status === "invalid") return { title: t("sig.status.invalid"), text: t(`sig.problem.${s.problem ?? "modified"}`) };
  return { title: t("sig.status.unknown"), text: t(`sig.problem.${s.problem ?? "untrusted"}`) };
}

function select(i: number) {
  props.tab.sigSelected = i;
  const s = list.value[i];
  if (s?.page !== null && s?.page !== undefined) {
    tabs.goto(props.tab, s.page, s.rect ? Math.max(0, s.rect.y - 60) : undefined);
    if (s.rect) props.tab.flash = { page: s.page, rect: s.rect, seq: Date.now() };
  }
}

function details(s: SignatureInfo): string {
  const c = s.certificate;
  const rows: [string, string | null | undefined][] = [
    [t("sig.field"), s.field],
    [t("sig.status.label"), statusText(s).title],
    [t("sig.signer"), [s.signer, s.organization].filter(Boolean).join(", ")],
    [t("sig.date"), sigDate(s.signedAt, s.utcOffset, locale.value)],
    [t("sig.timestamp"), s.timestamp !== null ? `${sigDate(s.timestamp, null, locale.value)} (${s.timestampTrusted ? t("sig.verified") : s.timestampVerified ? t("sig.timestampUntrusted") : t("sig.notVerified")})` : t("sig.none")],
    [t("sig.reason"), s.reason],
    [t("sig.location"), s.location],
    [t("sig.coverage"), s.coversWhole ? t("sig.wholeDocument") : t("sig.signedRevision")],
  ];
  if (c) {
    rows.push(
      [t("sig.subject"), c.subject],
      [t("sig.issuer"), c.issuer],
      [t("sig.validity"), `${shortDate(c.notBefore, locale.value)} → ${shortDate(c.notAfter, locale.value)}`],
      [t("sig.serial"), c.serial],
      ["SHA-256", c.sha256],
      [t("sig.chain"), c.chain.join(" → ")],
    );
  }
  return rows.filter(([, v]) => v).map(([k, v]) => `${k} : ${v}`).join("\n");
}

async function copy() {
  if (!sig.value) return;
  try {
    await navigator.clipboard.writeText(details(sig.value));
    ui.notify(t("sig.copied"));
  } catch {
    ui.notify(t("sig.copyFailed"));
  }
}
</script>

<template>
  <aside class="insp" :aria-label="t('sig.panel')" @keydown.esc.stop="tab.sigPanel = false">
    <div class="inh">
      <ShieldCheck v-if="sig?.status === 'valid'" class="ic s ok" aria-hidden="true" />
      <ShieldX v-else-if="sig?.status === 'invalid'" class="ic s bad" aria-hidden="true" />
      <ShieldQuestion v-else class="ic s warn" aria-hidden="true" />
      <span class="grow">{{ t("sig.panel") }}</span>
      <button class="cl" style="width: 26px; height: 26px" :aria-label="t('sig.closePanel')" @click="tab.sigPanel = false">
        <X class="ic s" aria-hidden="true" />
      </button>
    </div>
    <div v-if="sig" class="scroll">
      <div class="stc" :class="sig.status">
        <CircleCheck v-if="sig.status === 'valid'" class="ic" aria-hidden="true" />
        <CircleX v-else-if="sig.status === 'invalid'" class="ic" aria-hidden="true" />
        <CircleHelp v-else class="ic" aria-hidden="true" />
        <div><b>{{ statusText(sig).title }}</b><span>{{ statusText(sig).text }}</span></div>
      </div>
      <div v-if="list.length > 1" class="pick seg" role="tablist">
        <button v-for="(s, i) in list" :key="s.field" role="tab" :aria-selected="i === index" :class="{ on: i === index }" @click="select(i)">{{ i + 1 }}</button>
      </div>
      <button class="ish link" @click="select(index)">{{ t("sig.nOfM", { n: index + 1, m: list.length }) }}</button>
      <div class="dl">
        <span class="dt">{{ t("sig.signer") }}</span>
        <span class="dd">{{ sig.signer ?? t("sig.unknownSigner") }}<template v-if="sig.organization"><br /><span class="dim">{{ sig.organization }}</span></template></span>
        <template v-if="sig.signedAt !== null">
          <span class="dt">{{ t("sig.date") }}</span><span class="dd">{{ sigDate(sig.signedAt, sig.utcOffset, locale) }}</span>
        </template>
        <span class="dt">{{ t("sig.timestamp") }}</span>
        <span class="dd">
          <span v-if="sig.timestamp === null" class="dim">{{ t("sig.none") }}</span>
          <span v-else-if="sig.timestampTrusted" class="okt"><CircleCheck class="ic xs" aria-hidden="true" />{{ t("sig.verified") }}</span>
          <span v-else-if="sig.timestampVerified" class="wt"><CircleHelp class="ic xs" aria-hidden="true" />{{ t("sig.timestampUntrusted") }}</span>
          <span v-else class="wt"><CircleHelp class="ic xs" aria-hidden="true" />{{ t("sig.notVerified") }}</span>
        </span>
        <template v-if="sig.reason"><span class="dt">{{ t("sig.reason") }}</span><span class="dd">{{ sig.reason }}</span></template>
        <template v-if="sig.location"><span class="dt">{{ t("sig.location") }}</span><span class="dd">{{ sig.location }}</span></template>
        <span class="dt">{{ t("sig.coverage") }}</span>
        <span class="dd">{{ sig.coversWhole ? t("sig.wholeDocument") : t("sig.signedRevision") }}</span>
      </div>
      <template v-if="sig.certificate">
        <div class="hr" />
        <div class="ish">{{ t("sig.certificate") }}</div>
        <div class="dl">
          <span class="dt">{{ t("sig.issuedTo") }}</span><span class="dd">{{ sig.certificate.commonName ?? sig.certificate.subject }}</span>
          <span class="dt">{{ t("sig.issuedBy") }}</span><span class="dd">{{ sig.certificate.issuerName ?? sig.certificate.issuer }}</span>
          <span class="dt">{{ t("sig.validity") }}</span>
          <span class="dd">{{ shortDate(sig.certificate.notBefore, locale) }} → {{ shortDate(sig.certificate.notAfter, locale) }}</span>
          <span class="dt">{{ t("sig.chain") }}</span>
          <span class="dd">
            <span v-if="sig.trusted" class="okt"><CircleCheck class="ic xs" aria-hidden="true" />{{ t("sig.trusted") }}</span>
            <span v-else class="wt"><CircleHelp class="ic xs" aria-hidden="true" />{{ t("sig.untrusted") }}</span>
          </span>
          <span class="dt">SHA-256</span><span class="dd mono" :title="sig.certificate.sha256">{{ shortFingerprint(sig.certificate.sha256) }}</span>
        </div>
      </template>
      <div v-if="sig.intact && !sig.trusted && sig.certificate" class="trust">
        <button class="btn out" @click="trustOpen = true"><ShieldPlus class="ic s" aria-hidden="true" />{{ t("sig.trustRoot", { name: sig.certificate.rootName }) }}</button>
      </div>
      <div class="acts">
        <button v-if="sig.certificate" class="btn out" @click="certOpen = true">{{ t("sig.showCertificate") }}</button>
        <button class="btn gh" @click="copy">{{ t("sig.copyDetails") }}</button>
      </div>
    </div>
    <Modal v-if="trustOpen && sig?.certificate" :title="t('sig.trustTitle')" :close-label="t('sig.close')" :width="480" @close="trustOpen = false">
      <p class="tp">{{ t("sig.trustBody", { name: sig.certificate.rootName }) }}</p>
      <div class="dl cert">
        <span class="dt">{{ t("sig.authority") }}</span><span class="dd">{{ sig.certificate.rootName }}</span>
        <span class="dt">SHA-256</span><span class="dd mono">{{ sig.certificate.rootSha256 }}</span>
      </div>
      <div class="mft">
        <button class="btn gh" @click="trustOpen = false">{{ t("save.cancel") }}</button>
        <button class="btn pri" @click="trust">{{ t("sig.trustConfirm") }}</button>
      </div>
    </Modal>
    <Modal v-if="certOpen && sig?.certificate" :title="t('sig.certificate')" :close-label="t('sig.close')" :width="520" @close="certOpen = false">
      <div class="dl cert">
        <span class="dt">{{ t("sig.subject") }}</span><span class="dd">{{ sig.certificate.subject }}</span>
        <span class="dt">{{ t("sig.issuer") }}</span><span class="dd">{{ sig.certificate.issuer }}</span>
        <span class="dt">{{ t("sig.validity") }}</span>
        <span class="dd">{{ shortDate(sig.certificate.notBefore, locale) }} → {{ shortDate(sig.certificate.notAfter, locale) }}</span>
        <span class="dt">{{ t("sig.serial") }}</span><span class="dd mono">{{ sig.certificate.serial }}</span>
        <span class="dt">SHA-256</span><span class="dd mono">{{ sig.certificate.sha256 }}</span>
        <span class="dt">{{ t("sig.chain") }}</span><span class="dd">{{ sig.certificate.chain.join(" → ") }}</span>
      </div>
    </Modal>
  </aside>
</template>

<style scoped>
.insp { width: 340px; flex: none; background: var(--sidebar); border-left: 1px solid var(--line); display: flex; flex-direction: column; overflow: hidden; }
.inh { display: flex; align-items: center; gap: 8px; padding: 12px 12px 8px 16px; font-size: 13.5px; font-weight: 600; }
.inh .ok { color: var(--ok); } .inh .bad { color: var(--bad); } .inh .warn { color: var(--warn); }
.grow { flex: 1; }
.scroll { overflow: auto; flex: 1; padding-bottom: 8px; }
.stc { margin: 4px 16px; padding: 12px; border-radius: 10px; display: flex; gap: 10px; align-items: flex-start; }
.stc.valid { background: var(--ok-bg); } .stc.valid .ic { color: var(--ok); }
.stc.invalid { background: var(--bad-bg); } .stc.invalid .ic { color: var(--bad); }
.stc.unknown { background: var(--warn-bg); } .stc.unknown .ic { color: var(--warn); }
.stc b { display: block; font-size: 13px; font-weight: 600; }
.stc span { font-size: 12px; color: var(--text-2); }
.pick { margin: 10px 16px 0; align-self: flex-start; display: inline-flex; }
.ish { display: block; padding: 16px 16px 8px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--text-3); border: 0; background: none; text-align: left; font-family: inherit; }
.ish.link { cursor: pointer; }
.ish.link:hover { color: var(--text-2); }
.dl { display: grid; grid-template-columns: 118px minmax(0, 1fr); gap: 8px 12px; padding: 0 16px; font-size: 12.5px; }
.dl.cert { padding: 0; grid-template-columns: 110px minmax(0, 1fr); }
.dt { color: var(--text-3); }
.dd { color: var(--text); overflow-wrap: anywhere; }
.dim { color: var(--text-2); }
.okt, .wt { display: inline-flex; align-items: center; gap: 5px; font-weight: 500; }
.okt { color: var(--ok); } .wt { color: var(--warn); }
.hr { height: 1px; background: var(--line); margin: 16px 16px 0; }
.mono { font-family: ui-monospace, "JetBrains Mono", "DejaVu Sans Mono", Menlo, monospace; font-size: 11.5px; letter-spacing: .02em; }
.acts { display: flex; gap: 8px; padding: 18px 16px; flex-wrap: wrap; }
.trust { padding: 18px 16px 0; }
.trust .btn { width: 100%; height: auto; min-height: 30px; padding: 6px 12px; white-space: normal; text-align: left; justify-content: flex-start; }
.tp { margin: 0 0 14px; color: var(--text-2); line-height: 1.5; }
.mft { display: flex; justify-content: flex-end; gap: 8px; margin-top: 20px; }
</style>
