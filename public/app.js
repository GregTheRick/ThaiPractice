// Every mode is a prompt/answer pair; the quiz card, answer checking and
// keyboard choice all key off these fields. prompt: meanings|thai|phonetic|audio
// (audio speaks the Thai script via TTS); answer: thai|phonetic|meanings|self.
// The Phonetics group serves students who know no Thai script.
const MODES = [
  { id: 'spell', name: 'Spelling', badge: 'S', group: 'Thai script', prompt: 'meanings', answer: 'thai', desc: 'See English, type the Thai word' },
  { id: 'read', name: 'Reading', badge: 'R', group: 'Thai script', prompt: 'thai', answer: 'self', desc: 'Read Thai, recall the meaning' },
  { id: 'translate', name: 'Translation', badge: 'T', group: 'Thai script', prompt: 'thai', answer: 'meanings', desc: 'Read Thai, name every English meaning' },
  { id: 'phonetic', name: 'Pronunciation', badge: 'P', group: 'Thai script', prompt: 'thai', answer: 'phonetic', desc: 'Read Thai, type the phonetic' },
  { id: 'listen', name: 'Listening', badge: 'L', group: 'Thai script', prompt: 'audio', answer: 'thai', desc: 'Hear Thai, type what you heard' },
  { id: 'pspell', name: 'Phonetic spelling', badge: 's', group: 'Phonetics', prompt: 'meanings', answer: 'phonetic', desc: 'See English, type the phonetic' },
  { id: 'ptranslate', name: 'Phonetic translation', badge: 't', group: 'Phonetics', prompt: 'phonetic', answer: 'meanings', desc: 'Read the phonetic, name every English meaning' },
  { id: 'plisten', name: 'Phonetic listening', badge: 'l', group: 'Phonetics', prompt: 'audio', answer: 'phonetic', desc: 'Hear Thai, type the phonetic' },
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
    username: '',
    password: '',
    registering: false,
    me: '',
    error: '',
    words: [],
    form: { id: null, thai: '', phonetic: '', meanings: [] },
    newMeaning: '',
    kbOn: localStorage.getItem('kbOn') === '1',
    kbPhon: localStorage.getItem('kbPhon') === '1',
    phoneticRows: PHONETIC,
    modes: MODES,
    quiz: { mode: null, words: [], i: 0, answer: '', found: [], revealed: false, result: null, right: 0, wrong: 0 },
  }),
  computed: {
    cur() { return this.quiz.words[this.quiz.i]; },
    curMode() { return MODES.find(m => m.id === this.quiz.mode); },
  },
  async mounted() {
    try {
      this.me = (await this.api('/api/me')).username;
      await this.load();
      this.view = 'words';
    } catch { /* stays on login */ }
  },
  methods: {
    speak,
    modeName(id) { return MODES.find(m => m.id === id).name; },
    // mirrors the server's per-mode gating, for the words-view box badges
    eligible(w, m) {
      const needsThai = m.prompt === 'thai' || m.prompt === 'audio' || m.answer === 'thai';
      const needsPhonetic = m.prompt === 'phonetic' || m.answer === 'phonetic';
      return (!needsThai || w.thai) && (!needsPhonetic || w.phonetic);
    },
    async api(path, opts = {}) {
      const res = await fetch(path, { headers: { 'Content-Type': 'application/json' }, ...opts });
      if (res.status === 401 && path !== '/api/login') this.view = 'login';
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || res.status);
      return data;
    },
    async auth() {
      this.error = '';
      try {
        const r = await this.api(this.registering ? '/api/register' : '/api/login',
          { method: 'POST', body: JSON.stringify({ username: this.username, password: this.password }) });
        this.me = r.username;
        this.password = '';
        await this.load();
        this.view = 'words';
      } catch (e) {
        this.error = e.message;
      }
    },
    async logout() {
      await this.api('/api/logout', { method: 'POST' }).catch(() => {});
      this.me = '';
      this.words = [];
      this.view = 'login';
    },
    async load() { this.words = await this.api('/api/words'); },
    toggle(pref) {
      this[pref] = !this[pref];
      localStorage.setItem(pref, this[pref] ? '1' : '0');
    },
    addMeaning() {
      const m = norm(this.newMeaning);
      if (m && !this.form.meanings.some(e => e.toLowerCase() === m.toLowerCase())) {
        this.form.meanings.push(m);
      }
      this.newMeaning = '';
    },
    async saveWord() {
      this.error = '';
      this.addMeaning(); // adopt a meaning still sitting in the input
      if (!this.form.meanings.length) {
        this.error = 'add at least one meaning';
        return;
      }
      if (!this.form.thai.trim() && !this.form.phonetic.trim()) {
        this.error = 'enter the Thai script or a phonetic (or both)';
        return;
      }
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
      this.form = { id: w.id, thai: w.thai, phonetic: w.phonetic, meanings: [...w.meanings] };
      document.getElementById('thai-input').focus();
    },
    resetForm() { this.form = { id: null, thai: '', phonetic: '', meanings: [] }; this.newMeaning = ''; },
    async delWord(w) {
      if (!confirm(`Delete ${w.thai} (${w.meaning})?`)) return;
      await this.api(`/api/words/${w.id}`, { method: 'DELETE' });
      await this.load();
    },
    async startQuiz(mode) {
      this.quiz = { mode, words: await this.api(`/api/quiz?mode=${mode}`),
                    i: 0, answer: '', found: [], revealed: false, result: null, right: 0, wrong: 0 };
      if (this.curMode.prompt === 'audio' && this.cur) speak(this.cur.thai);
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
      const a = norm(this.quiz.answer);
      if (this.curMode.answer === 'meanings') {
        // name-them-all: each guess must exactly match a whole meaning
        // ("go" never counts for "go to"); a wrong guess ends the round.
        // ponytail: scheduling stays per word — a missed meaning resets the
        // whole word's box; per-meaning boxes are the upgrade path.
        if (!a) return;
        this.quiz.answer = '';
        const hit = this.cur.meanings.find(m => norm(m).toLowerCase() === a.toLowerCase());
        if (!hit) {
          this.quiz.result = false;
          this.review(false);
        } else if (!this.quiz.found.includes(hit)) {
          this.quiz.found.push(hit);
          if (this.quiz.found.length === this.cur.meanings.length) {
            this.quiz.result = true;
            this.review(true);
          }
        } // repeating an already-found meaning is a harmless no-op
        return;
      }
      this.quiz.result = this.curMode.answer === 'thai' ? a === norm(this.cur.thai)
        : a.toLowerCase() === norm(this.cur.phonetic).toLowerCase();
      this.review(this.quiz.result);
    },
    giveUp() {
      this.quiz.result = false;
      this.review(false);
    },
    grade(correct) {
      this.review(correct);
      this.next();
    },
    next() {
      Object.assign(this.quiz, { i: this.quiz.i + 1, answer: '', found: [], revealed: false, result: null });
      if (this.curMode.prompt === 'audio' && this.cur) speak(this.cur.thai);
    },
  },
});
app.component('thai-keyboard', ThaiKeyboard);
app.mount('#app');
