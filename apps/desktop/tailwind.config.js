/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Shop-floor palette: high-contrast, glanceable on a kiosk.
        good: "#16a34a",
        scrap: "#dc2626",
        down: "#ea580c",
        idle: "#64748b",
      },
    },
  },
  plugins: [],
};
