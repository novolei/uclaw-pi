/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: [
    './src/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        border: 'hsl(var(--border) / <alpha-value>)',
        input: 'hsl(var(--input))',
        ring: 'hsl(var(--ring))',
        background: 'hsl(var(--background) / <alpha-value>)',
        foreground: 'hsl(var(--foreground) / <alpha-value>)',
        primary: {
          DEFAULT: 'hsl(var(--primary))',
          foreground: 'hsl(var(--primary-foreground))',
        },
        secondary: {
          DEFAULT: 'hsl(var(--secondary))',
          foreground: 'hsl(var(--secondary-foreground))',
        },
        muted: {
          DEFAULT: 'hsl(var(--muted))',
          foreground: 'hsl(var(--muted-foreground))',
        },
        accent: {
          DEFAULT: 'hsl(var(--accent))',
          foreground: 'hsl(var(--accent-foreground))',
        },
        destructive: {
          DEFAULT: 'hsl(var(--destructive))',
          foreground: 'hsl(var(--destructive-foreground))',
        },
        card: {
          DEFAULT: 'hsl(var(--card))',
          foreground: 'hsl(var(--card-foreground))',
        },
        popover: {
          DEFAULT: 'hsl(var(--popover))',
          foreground: 'hsl(var(--popover-foreground))',
        },
        dialog: {
          DEFAULT: 'hsl(var(--dialog))',
          foreground: 'hsl(var(--dialog-foreground))',
        },
        tooltip: {
          DEFAULT: 'hsl(var(--tooltip) / <alpha-value>)',
          foreground: 'hsl(var(--tooltip-foreground) / <alpha-value>)',
          muted: 'hsl(var(--tooltip-muted) / <alpha-value>)',
        },
        'content-area': 'hsl(var(--content-area) / <alpha-value>)',
        // Semantic tokens already defined per-theme in globals.css — exposed
        // here so components (approval modal, toasts, status badges) can
        // reach them without hardcoding bg-yellow-500 / text-red-500 etc.,
        // which break under warm-paper / qingye / forest-* themes.
        success: {
          DEFAULT: 'hsl(var(--success))',
          bg: 'hsl(var(--success-bg))',
        },
        warning: {
          DEFAULT: 'hsl(var(--warning))',
          bg: 'hsl(var(--warning-bg))',
        },
        danger: {
          DEFAULT: 'hsl(var(--danger))',
          bg: 'hsl(var(--danger-bg))',
        },
        // Memory UI kind tokens (docs/design/memory-ui.md). `bg-memory-fact` = the
        // kind's ink (dots, bars); `bg-memory-fact-bg` = tinted badge surface;
        // `text-memory-fact` = badge label (≥4.5:1 on its -bg in every theme).
        'memory-fact': {
          DEFAULT: 'hsl(var(--memory-kind-fact) / <alpha-value>)',
          bg: 'hsl(var(--memory-kind-fact-bg) / <alpha-value>)',
        },
        'memory-preference': {
          DEFAULT: 'hsl(var(--memory-kind-preference) / <alpha-value>)',
          bg: 'hsl(var(--memory-kind-preference-bg) / <alpha-value>)',
        },
        'memory-episode': {
          DEFAULT: 'hsl(var(--memory-kind-episode) / <alpha-value>)',
          bg: 'hsl(var(--memory-kind-episode-bg) / <alpha-value>)',
        },
        'memory-lesson': {
          DEFAULT: 'hsl(var(--memory-kind-lesson) / <alpha-value>)',
          bg: 'hsl(var(--memory-kind-lesson-bg) / <alpha-value>)',
        },
        'memory-entity': {
          DEFAULT: 'hsl(var(--memory-kind-entity) / <alpha-value>)',
          bg: 'hsl(var(--memory-kind-entity-bg) / <alpha-value>)',
        },
        'memory-procedure': {
          DEFAULT: 'hsl(var(--memory-kind-procedure) / <alpha-value>)',
          bg: 'hsl(var(--memory-kind-procedure-bg) / <alpha-value>)',
        },
        progress: {
          track: 'hsl(var(--progress-track) / <alpha-value>)',
          fill: 'hsl(var(--progress-fill) / <alpha-value>)',
        },
        // Fixed cross-theme scales — used by Dock/ConnectionIndicator; do not
        // change with theme switches unlike the CSS-variable tokens above.
        sage: {
          '50':  '#f0fdf4',
          '100': '#dcfce7',
          '200': '#bbf7d0',
          '300': '#86efac',
          '400': '#4ade80',
          '500': '#34c759',
          '600': '#16a34a',
          '700': '#15803d',
          '800': '#166534',
          '900': '#14532d',
        },
        coral: {
          '50':  '#fff1f2',
          '100': '#ffe4e6',
          '200': '#fecdd3',
          '300': '#fda4af',
          '400': '#fb7185',
          '500': '#f43f5e',
          '600': '#e11d48',
          '700': '#be123c',
          '800': '#9f1239',
          '900': '#881337',
        },
        amber: {
          '50':  '#fffbeb',
          '100': '#fef3c7',
          '200': '#fde68a',
          '300': '#fcd34d',
          '400': '#fbbf24',
          '500': '#f59e0b',
          '600': '#d97706',
          '700': '#b45309',
          '800': '#92400e',
          '900': '#78350f',
        },
      },
      fontFamily: {
        // CSS 变量驱动 — 切换主题时自动跟随
        sans: ['var(--font-sans)', 'ui-sans-serif', 'system-ui', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'Roboto', '"Helvetica Neue"', 'Arial', 'sans-serif'],
        serif: ['var(--font-serif)', 'ui-serif', 'Georgia', 'serif'],
        mono: ['var(--font-mono)', 'ui-monospace', 'SFMono-Regular', 'Menlo', 'Monaco', 'Consolas', 'monospace'],
      },
      boxShadow: {
        'elevation-1': 'var(--elevation-1)',
        'elevation-2': 'var(--elevation-2)',
      },
      borderRadius: {
        lg: 'var(--radius)',
        md: 'calc(var(--radius) - 2px)',
        sm: 'calc(var(--radius) - 4px)',
      },
      keyframes: {
        'slide-in-from-top': {
          from: { transform: 'translateY(-100%)' },
          to: { transform: 'translateY(0)' },
        },
        'slide-in-from-bottom': {
          from: { transform: 'translateY(100%)' },
          to: { transform: 'translateY(0)' },
        },
        'slide-out-to-right': {
          from: { transform: 'translateX(0)' },
          to: { transform: 'translateX(100%)' },
        },
        // Memory UI: indeterminate progress sweep + skeleton shimmer.
        'memory-indeterminate': {
          from: { transform: 'translateX(-100%)' },
          to: { transform: 'translateX(250%)' },
        },
        'memory-shimmer': {
          from: { backgroundPosition: '200% 0' },
          to: { backgroundPosition: '-200% 0' },
        },

      },
      animation: {
        'in': 'slide-in-from-top 0.3s ease-out',
        'out': 'slide-out-to-right 0.2s ease-in',
        'memory-indeterminate': 'memory-indeterminate 1.4s cubic-bezier(0.65, 0, 0.35, 1) infinite',
        'memory-shimmer': 'memory-shimmer 1.6s linear infinite',

      },
    },
  },
  plugins: [
    require('@tailwindcss/typography'),
    require('tailwindcss-animate'),
  ],
}
