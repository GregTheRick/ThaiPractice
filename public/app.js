const MODES = [
  { id: 'spell', name: 'Spelling', badge: 'S', desc: 'See English, type the Thai word' },
  { id: 'read', name: 'Reading', badge: 'R', desc: 'Read Thai, recall the meaning' },
  { id: 'translate', name: 'Translation', badge: 'T', desc: 'Read Thai, type the English meaning' },
  { id: 'phonetic', name: 'Pronunciation', badge: 'P', desc: 'Read Thai, type the phonetic' },
  { id: 'listen', name: 'Listening', badge: 'L', desc: 'Hear Thai, type what you heard' },
];

const norm = s => s.normalize('NFC').trim();

function speak(text) {
  const u = new SpeechSynthesisUtterance(text);
  u.lang = 'th-TH';
  const voice = speechSynthesis.getVoices().find(v => v.lang.startsWith('th'));
  if (voice) u.voice = voice;
  speechSynthesis.cancel();
  speechSynthesis.speak(u);
}
speechSynthesis?.getVoices(); // warm the async voice list

const app = Vue.createApp({
  data: () => ({
    view: 'login',
    password: '',
    error: '',
    words: [],
    form: { id: null, thai: '', phonetic: '', meaning: '' },
    kbOn: localStorage.getItem('kbOn') === '1',
    kbPhon: localStorage.getItem('kbPhon') === '1',
    phoneticRows: PHONETIC,
    modes: MODES,
    quiz: { mode: null, words: [], i: 0, answer: '', revealed: false, result: null, right: 0, wrong: 0 },
  }),
  computed: {
    cur() { return this.quiz.words[this.quiz.i]; },
    thaiTyped() { return this.quiz.mode === 'spell' || this.quiz.mode === 'listen'; },
  },
  async mounted() {
    try {
      await this.load();
      this.view = 'words';
    } catch { /* stays on login */ }
  },
  methods: {
    speak,
    modeName(id) { return MODES.find(m => m.id === id).name; },
    async api(path, opts = {}) {
      const res = await fetch(path, { headers: { 'Content-Type': 'application/json' }, ...opts });
      if (res.status === 401 && path !== '/api/login') this.view = 'login';
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || res.status);
      return data;
    },
    async login() {
      this.error = '';
      try {
        await this.api('/api/login', { method: 'POST', body: JSON.stringify({ password: this.password }) });
        this.password = '';
        await this.load();
        this.view = 'words';
      } catch (e) {
        this.error = e.message;
      }
    },
    async load() { this.words = await this.api('/api/words'); },
    toggle(pref) {
      this[pref] = !this[pref];
      localStorage.setItem(pref, this[pref] ? '1' : '0');
    },
    async saveWord() {
      this.error = '';
      const { id, ...body } = this.form;
      try {
        await this.api(id ? `/api/words/${id}` : '/api/words',
          { method: id ? 'PUT' : 'POST', body: JSON.stringify(body) });
        this.resetForm();
        await this.load();
      } catch (e) {
        this.error = e.message;
      }
    },
    editWord(w) {
      this.form = { id: w.id, thai: w.thai, phonetic: w.phonetic, meaning: w.meaning };
      document.getElementById('thai-input').focus();
    },
    resetForm() { this.form = { id: null, thai: '', phonetic: '', meaning: '' }; },
    async delWord(w) {
      if (!confirm(`Delete ${w.thai} (${w.meaning})?`)) return;
      await this.api(`/api/words/${w.id}`, { method: 'DELETE' });
      await this.load();
    },
    async startQuiz(mode) {
      this.quiz = { mode, words: await this.api(`/api/quiz?mode=${mode}`),
                    i: 0, answer: '', revealed: false, result: null, right: 0, wrong: 0 };
      if (mode === 'listen' && this.cur) speak(this.cur.thai);
    },
    review(correct) {
      this.quiz[correct ? 'right' : 'wrong']++;
      // fire and forget; a lost review only means the word shows up again
      this.api('/api/review', {
        method: 'POST',
        body: JSON.stringify({ word_id: this.cur.id, mode: this.quiz.mode, correct }),
      }).catch(() => {});
    },
    submitAnswer() {
      const expected = this.thaiTyped ? this.cur.thai
        : this.quiz.mode === 'phonetic' ? this.cur.phonetic : this.cur.meaning;
      const [a, b] = [norm(this.quiz.answer), norm(expected)];
      this.quiz.result = this.thaiTyped ? a === b : a.toLowerCase() === b.toLowerCase();
      this.review(this.quiz.result);
    },
    grade(correct) {
      this.review(correct);
      this.next();
    },
    next() {
      Object.assign(this.quiz, { i: this.quiz.i + 1, answer: '', revealed: false, result: null });
      if (this.quiz.mode === 'listen' && this.cur) speak(this.cur.thai);
    },
  },
});
app.component('thai-keyboard', ThaiKeyboard);
app.mount('#app');
