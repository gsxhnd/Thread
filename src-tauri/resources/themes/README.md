# Puck color themes

Place custom color themes in this folder as `{id}.css` files.

Theme ids use lowercase letters, digits, and hyphens. The settings UI derives display names from the id:

- English: hyphens become spaces, first letter of each word is capitalized (`aa-1` → `Aa 1`)
- Chinese: hyphens become spaces only (`aa-1` → `aa 1`)

Each CSS file should define light and dark color variables:

```css
:root[data-color-theme="my-theme"] {
  --background: oklch(1 0 0);
  --foreground: oklch(0.145 0 0);
  /* ... */
}

.dark[data-color-theme="my-theme"] {
  --background: oklch(0.145 0 0);
  --foreground: oklch(0.985 0 0);
  /* ... */
}
```

## UI chrome tokens (optional)

Themes can also control layout chrome. Switching the color theme in **Settings → Appearance** updates these values automatically unless the user has customized them.

```css
:root[data-color-theme="my-theme"] {
  --ui-font-family: "Inter Variable", sans-serif;
  --ui-radius: 0.625rem;
  --ui-window-radius: 10px;
  --shell-primary-mix: 96%;
  --shell-secondary-mix: 92%;
  --terminal-padding-x: 10px;
  --terminal-padding-y: 8px;
}
```

| Variable | Purpose |
| --- | --- |
| `--ui-font-family` | UI font (sidebars, buttons, dialogs) |
| `--ui-radius` | Control corner radius |
| `--ui-window-radius` | macOS window outer corner radius |
| `--shell-primary-mix` | Primary panel background mix (`background` %) |
| `--shell-secondary-mix` | Secondary panel background mix (`background` %) |
| `--terminal-padding-x` / `--terminal-padding-y` | Terminal inset padding |

The built-in `default` theme ships with the app and is not stored here.

Reopen Settings to pick up newly added theme files.
