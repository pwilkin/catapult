/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        surface: {
          0: "#0a0a0b",
          1: "#111113",
          2: "#18181b",
          3: "#222226",
          4: "#2a2a30",
        },
        border: {
          DEFAULT: "#2e2e36",
          strong: "#3e3e4a",
        },
        primary: {
          DEFAULT: "#7c3aed",
          hover: "#6d28d9",
          light: "#8b5cf6",
        },
        accent: {
          green: "#22c55e",
          red: "#ef4444",
          "red-dark": "#b91c1c",
          orange: "#f97316",
          yellow: "#f59e0b",
          blue: "#3b82f6",
          cyan: "#06b6d4",
        },
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "Fira Code", "monospace"],
      },
    },
  },
  plugins: [],
};
