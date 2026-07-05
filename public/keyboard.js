// On-screen Thai keyboard, standard Kedmanee layout: [normal, shifted] per key.
// Plain data so a future "learn the keyboard" mode can reuse it.
const KEDMANEE = [
  [['_', '%'], ['ๅ', '+'], ['/', '๑'], ['-', '๒'], ['ภ', '๓'], ['ถ', '๔'], ['ุ', 'ู'], ['ึ', '฿'],
   ['ค', '๕'], ['ต', '๖'], ['จ', '๗'], ['ข', '๘'], ['ช', '๙']],
  [['ๆ', '๐'], ['ไ', '"'], ['ำ', 'ฎ'], ['พ', 'ฑ'], ['ะ', 'ธ'], ['ั', 'ํ'], ['ี', '๊'], ['ร', 'ณ'],
   ['น', 'ฯ'], ['ย', 'ญ'], ['บ', 'ฐ'], ['ล', ','], ['ฃ', 'ฅ']],
  [['ฟ', 'ฤ'], ['ห', 'ฆ'], ['ก', 'ฏ'], ['ด', 'โ'], ['เ', 'ฌ'], ['้', '็'], ['่', '๋'], ['า', 'ษ'],
   ['ส', 'ศ'], ['ว', 'ซ'], ['ง', '.']],
  [['ผ', '('], ['ป', ')'], ['แ', 'ฉ'], ['อ', 'ฮ'], ['ิ', 'ฺ'], ['ื', '์'], ['ท', '?'], ['ม', 'ฒ'],
   ['ใ', 'ฬ'], ['ฝ', 'ฦ']],
];

// Inserts at the input's cursor (or deletes one char for '\b') and fires an
// input event so Vue's v-model stays in sync.
function kbInsert(targetId, ch) {
  const el = document.getElementById(targetId);
  if (!el) return;
  el.focus();
  const s = el.selectionStart ?? el.value.length;
  const e = el.selectionEnd ?? el.value.length;
  if (ch === '\b') el.setRangeText('', s === e ? Math.max(0, s - 1) : s, e, 'end');
  else el.setRangeText(ch, s, e, 'end');
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

// pointerdown.prevent everywhere so tapping keys never steals focus from the input.
const ThaiKeyboard = {
  props: { target: String },
  data: () => ({ shift: false }),
  computed: { rows: () => KEDMANEE },
  methods: {
    press(ch) {
      kbInsert(this.target, ch);
      this.shift = false;
    },
  },
  template: `
  <div class="kb">
    <div class="kb-row" v-for="row in rows">
      <button type="button" v-for="k in row" :key="k[0]"
              @pointerdown.prevent="press(shift ? k[1] : k[0])">{{ shift ? k[1] : k[0] }}</button>
    </div>
    <div class="kb-row">
      <button type="button" class="kb-wide" :class="{ active: shift }" aria-label="Shift"
              @pointerdown.prevent="shift = !shift">⇧</button>
      <button type="button" class="kb-space" aria-label="Space"
              @pointerdown.prevent="press(' ')"> </button>
      <button type="button" class="kb-wide" aria-label="Backspace"
              @pointerdown.prevent="press('\\b')">⌫</button>
    </div>
  </div>`,
};
