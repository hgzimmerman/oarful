/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "crates/server/src/**/*.rs",
    "crates/server/src/templates/js/*.js",
  ],
  theme: {
    extend: {},
  },
  plugins: [],
};
