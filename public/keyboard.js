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

// IPA keyboard for Thai transliteration (Chulalongkorn CTFL system).
// Vowels + consonants cover the full inventory: long vowels are doubled (aa),
// digraphs are typed letter by letter (kh = k + h). The last row holds the
// combining tone marks — low, falling, high, rising; level is unmarked —
// which attach to the letter before the cursor (type "pa", tap tone, get "pà").
const PHONETIC = [
  [['a'], ['e'], ['i'], ['o'], ['u'], ['ɛ'], ['ɔ'], ['ə'], ['ɯ']],
  [['b'], ['c'], ['d'], ['f'], ['h'], ['k'], ['l'], ['m'], ['n'], ['p']],
  [['r'], ['s'], ['t'], ['w'], ['y'], ['ʔ'], ['ŋ']],
  [['̀'], ['̂'], ['́'], ['̌']], // ◌̀ ◌̂ ◌́ ◌̌
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
  props: { target: String, layout: { default: () => KEDMANEE } },
  data: () => ({ shift: false }),
  computed: {
    rows() { return this.layout; },
    hasShift() { return this.layout.some(row => row.some(k => k[1])); },
  },
  methods: {
    key(k) { return this.shift && k[1] ? k[1] : k[0]; },
    // tone-mark keys are shown applied to "a" (à â á ǎ), like the course slides
    label(k) { const ch = this.key(k); return /[̀-ͯ]/.test(ch) ? 'a' + ch : ch; },
    press(ch) {
      kbInsert(this.target, ch);
      this.shift = false;
    },
  },
  template: `
  <div class="kb">
    <div class="kb-row" v-for="row in rows">
      <button type="button" v-for="k in row" :key="k[0]"
              @pointerdown.prevent="press(key(k))">{{ label(k) }}</button>
    </div>
    <div class="kb-row">
      <button type="button" v-if="hasShift" class="kb-wide" :class="{ active: shift }" aria-label="Shift"
              @pointerdown.prevent="shift = !shift">⇧</button>
      <button type="button" class="kb-space" aria-label="Space"
              @pointerdown.prevent="press(' ')"> </button>
      <button type="button" class="kb-wide" aria-label="Backspace"
              @pointerdown.prevent="press('\\b')">⌫</button>
    </div>
  </div>`,
};
