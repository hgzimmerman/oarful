/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "crates/server/src/**/*.rs",
    "crates/server/src/templates/js/*.js",
  ],
  theme: {
    extend: {
      boxShadow: {
        soft: 'var(--shadow-soft)',
        card: 'var(--shadow-card)',
      },
      colors: {
        paper:   { DEFAULT: 'var(--paper)',   2: 'var(--paper-2)', 3: 'var(--paper-3)' },
        ink:     { DEFAULT: 'var(--ink)',     2: 'var(--ink-2)',   3: 'var(--ink-3)' },
        muted:   'var(--muted)',
        rule:    { DEFAULT: 'var(--rule)',    2: 'var(--rule-2)' },
        accent:  { DEFAULT: 'var(--accent)',  2: 'var(--accent-2)' },
        port:    'var(--port)',
        stbd:    'var(--stbd)',
        cox:     'var(--cox)',
        either:  'var(--either)',
        good:    'var(--good)',
        warn:    'var(--warn)',
        bad:     'var(--bad)',
      },
    },
  },
  plugins: [],
};
