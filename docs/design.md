# Design

The rules, values and reference files for every screen, component and piece of UI text.
Read this file before you write or review UI. How to read the reference files themselves:
`docs/reading-the-design-system.md`.

## Where the truth lives

| What | Where | Role |
|---|---|---|
| Design System (tokens, components with live previews, rules) | https://claude.ai/artifact/CpPMyvnhc34epKE8QXDjBi | The reference. Read it for any UI work. |
| Wireframe app | https://claude.ai/artifact/MgLQKCFQgTfwWRf8ZV7gFT | The source of every value in the design system (ADR 0017, D-131). |
| Desktop App demo | https://claude.ai/artifact/7Fhv5F5UpXwm87w1VVig3w | The same screens inside a macOS scene: menu bar, window, status popover. |
| UI Foundations | https://claude.ai/artifact/Nif3R9Bghoeb4rfJv8H8Xp | Contrast table and the accessibility rules UA-1 to UA-9. |
| This file | `docs/design.md` | A copy of the parts that bind code, so a contributor without the links can still build the UI. |

If this file and the design system disagree, the design system wins; fix this file in the
same change. The pages are owner-only links.

TODO(owner), before the repository is public: commit an export of the design system under
`docs/design-system/` and remove the links, as `docs/decisions.md` says for the wireframe.

## Decided by the owner on 2026-11-19

These are to be recorded in the Decision Log (the owner or the planning session does that).

1. **Amber for live is `#9a5a00`** in light mode (`live`), as in the wireframe and UI Foundations. The
   older desktop demo used `#c77a12`, which reads at 2.99:1 on `live-soft` and fails 4.5:1.
2. **Text on a filled accent is `acc-ink`**, never literal white or black. White on the dark `acc`
   reads at 1.85:1.
3. **Cards carry the soft `shadow`**, as the wireframe draws them. Floating surfaces use their own
   shadow tokens. UI Foundations says shadow is only for floating surfaces; the wireframe wins.

Still open: the app icon is a navy gradient and the UI accent is green. The desktop demo shows the
icon in the sidebar at 26 px. Do not recolour the icon until the owner decides.

## Rules

- **Tokens only.** A colour, radius, shadow or type size is a token from this file. A literal
  value in a component or style file is a defect. A value that has no token stops the task: ask
  the owner; do not invent one.
- **Light is the default theme.** Dark follows the system setting or the Settings sheet. Every
  colour token has both values; a screen must read in both.
- **Names.** CSS custom properties are the token names with `--` in front (`--acc`, `--card`,
  `--radius-10`, `--shadow`). Type sizes are `size-<n>`; fonts are the families `display`, `text`
  and `mono`: display for headings and the brand, text for body and controls, mono for timers,
  timestamps, tags and key caps.
- **State is never colour alone** (UA-4). A live dot, a failure banner and a muted lane each
  carry a word.
- **Focus** is a solid 2 px `acc` ring with a 2 px offset on every interactive element.
- **Motion** carries no meaning. The live dot and the spinner stop under Reduce motion (UA-9).
- **Four states.** A screen that loads data shows loading, empty, error and done: use
  `AsyncContent`.
- **Copy.** Name things by what the person recognises, never a model id. A control states what it
  does ("Start meeting", "Retry translation"). A failure stays until the cause is gone or the
  person dismisses it. Strings live in `apps/ui/src/shared/messages.en.ts` and `messages.vi.ts`.
- **No other product names** in any UI text or fixture (guardrail 3). Meeting apps are "meeting
  app". Libraries and fonts are named only in `docs/tech-stack.md` and `NOTICE`.

## Tokens

CSS names are `--<token>`. Values are copied from the design system's `tokens.json`.

### Colour: the app

| Token | Light | Dark | Use |
|---|---|---|---|
| `bg` | `#f7f8fa` | `#101412` | Page background. |
| `side` | `#f0f2f5` | `#0c100e` | Sidebar background. |
| `card` | `#ffffff` | `#171d1a` | Card, panel, banner and dialog surface. |
| `sunk` | `#eceff3` | `#222a26` | Inset surface: inputs and progress tracks. |
| `line` | `#e2e6ea` | `#2a3330` | Hairline border between surfaces. |
| `line2` | `#cbd1d8` | `#3a4541` | Stronger border: buttons, inputs, dividers. |
| `text` | `#14181c` | `#e8eeea` | Body text on `bg`, `card` and `side` (14.55:1 or better in both themes). |
| `mute` | `#5b646d` | `#92a09a` | Secondary text on `bg`, `card` and `sunk` (5.22:1 or better in both themes). |
| `acc` | `#1f5f4a` | `#6fd1a8` | Accent: primary action, active tab, links and the focus ring. |
| `acc-ink` | `#ffffff` | `#06241a` | Text on a filled `acc` (7.52:1 light, 8.91:1 dark). |
| `acc-soft` | `#e4f2ec` | `#153327` | Accent tint behind a selected or active item; `acc` text on it reads at 6.52:1 light, 7.40:1 dark. |
| `live` | `#9a5a00` | `#f0a93b` | Live and recording state. |
| `live-soft` | `#fdf0d6` | `#3a2c10` | Tint behind a live state; carries `live` text. |
| `bad` | `#b3261e` | `#f0958d` | Error and Stop. |
| `bad-soft` | `#f9e3df` | `#3b1f1d` | Tint behind an error; carries `bad` text. |
| `warn` | `#8a5a00` | `#f0c56a` | Warning text. |
| `warn-soft` | `#f7ecc9` | `#372c12` | Tint behind a warning; carries `warn` text. |
| `ok` | `#1f7a43` | `#6fd1a8` | Ready and success. |

### Colour: overlays and the subtitle bar

| Token | Light | Dark | Use |
|---|---|---|---|
| `scrim` | `rgba(8,12,10,0.45)` | `rgba(8,12,10,0.45)` | Backdrop behind a dialog or the settings sheet. |
| `bar` | `rgba(14,18,16,0.88)` | `rgba(14,18,16,0.88)` | Subtitle bar background: a dark bar over any screen, in both themes. |
| `bar-ink` | `#f3f6f4` | `#f3f6f4` | Text on the subtitle bar. |

### Colour: the desktop shell (menu bar, menus, window lights)

| Token | Light | Dark | Use |
|---|---|---|---|
| `menubar` | `rgba(18,22,26,0.52)` | `rgba(18,22,26,0.52)` | macOS menu bar background over the wallpaper (desktop demo). |
| `menu` | `rgba(38,42,46,0.94)` | `rgba(38,42,46,0.94)` | Background of a menu dropdown and the status popover. |
| `menu-ink` | `#f3f6f4` | `#f3f6f4` | Text on the menu bar, a dropdown and the popover. |
| `menu-mute` | `#aab8b1` | `#aab8b1` | Secondary text in the popover. |
| `menu-line` | `rgba(255,255,255,0.14)` | `rgba(255,255,255,0.14)` | Border and divider of a dropdown and the popover. |
| `menu-hover` | `rgba(255,255,255,0.2)` | `rgba(255,255,255,0.2)` | Hover and open state of a menu bar item or a dropdown row. |
| `menu-live` | `#ffc46b` | `#ffc46b` | The status item in the menu bar while a meeting runs. |
| `traffic-close` | `#ff5f57` | `#ff5f57` | Window close light. |
| `traffic-min` | `#febc2e` | `#febc2e` | Window minimize light. |
| `traffic-zoom` | `#28c840` | `#28c840` | Window zoom light. |

### Shadow

| Token | Value | Use |
|---|---|---|
| `shadow` | light: `0 1px 2px rgba(16,24,32,0.05), 0 8px 24px rgba(16,24,32,0.07)`; dark: `0 1px 2px rgba(0,0,0,0.4), 0 10px 28px rgba(0,0,0,0.35)` | Cards, the language pair and the segmented control's active segment (wireframe `--shadow`). |
| `shadow-menu` | `0 20px 50px rgba(0,0,0,0.25)` | Dropdown menus. |
| `shadow-float` | `0 24px 60px rgba(0,0,0,0.28)` | The compact live window. |
| `shadow-overlay` | `0 30px 80px rgba(0,0,0,0.4)` | Dialogs, the settings sheet and the command bar. |
| `shadow-toast` | `0 12px 30px rgba(0,0,0,0.3)` | Toasts. |
| `shadow-window` | `0 0 0 1px rgba(0,0,0,0.5), 0 30px 90px rgba(0,0,0,0.55)` | The app window on the desktop. |
| `shadow-popover` | `0 20px 56px rgba(0,0,0,0.5)` | Menu bar dropdowns and the status popover. |

### Corner radius

| Token | Value | Where the wireframe uses it |
|---|---|---|
| `radius-3` | `3px` | Meter and its fill. |
| `radius-5` | `5px` | Key caps. Menu bar items. |
| `radius-6` | `6px` | Mono tags, language badges. Menu rows. |
| `radius-8` | `8px` | Small buttons, the brand mark, segment items, dropdown items. |
| `radius-9` | `9px` | Command-bar rows. |
| `radius-10` | `10px` | Buttons, fields, search box, navigation items. Menu dropdowns and the status popover. |
| `radius-11` | `11px` | The segmented control. |
| `radius-12` | `12px` | Banners, toasts, the local-only card, the toggle track. The app window. |
| `radius-14` | `14px` | The language pair, list rows. |
| `radius-16` | `16px` | Cards, empty states, the compact window, the command bar. |
| `radius-18` | `18px` | Dialogs and the settings sheet. |
| `radius-pill` | `999px` | chips, the speaking dot and round buttons. |

Use no other radius. A fully round shape is `radius-pill`.

### Type sizes

| Token | Size | Where the wireframe uses it |
|---|---|---|
| `size-10` | `10px` | Language label in the language pair (letter-spaced capitals). |
| `size-11` | `11px` | Mono tags and key caps, table headers (capitals), turn badges. |
| `size-11-5` | `11.5px` | Speaker line of the subtitle bar. |
| `size-12` | `12px` | Mono timestamps, turn labels, secondary lines in a card. |
| `size-12-5` | `12.5px` | Chips, small buttons, row descriptions, system notes. |
| `size-13` | `13px` | Card subtitles, field labels, the English line of the subtitle bar. |
| `size-13-5` | `13.5px` | The spoken line in a transcript turn. |
| `size-14` | `14px` | Body text, buttons, navigation. |
| `size-15` | `15px` | Translated line in a transcript turn and in captions. |
| `size-16` | `16px` | Card titles (h2), the command bar input, empty-state title. |
| `size-18` | `18px` | Brand name in the sidebar, settings-sheet title. |
| `size-19` | `19px` | Dialog title, the translated line of the subtitle bar. |
| `size-24` | `24px` | Settings page title, the large subtitle bar. |
| `size-26` | `26px` | Page title (h1). |

There is no spacing scale: the wireframe has none. Do not add spacing tokens before the owner decides.

## Layout measures in the wireframe

| Part | Value |
|---|---|
| Window | sidebar 232 px, then the main area; minimum height 560 px |
| Top bar | 60 px high, 18 px side padding, 10 px gap |
| Content view | padding 18 px 24 px 40 px |
| Grid gap between cards | 14 px |
| Card padding | 16 px |
| Banner padding | 10 px 12 px |
| Dialog | 440 px wide at most, 20 px padding |
| Settings sheet | 860 by 560 px at most, navigation column 210 px |
| Command bar | 560 px wide at most, 90 px from the top |
| Compact live window | 360 px wide, 22 px from the right and bottom |
| Subtitle bar | 640 px wide at most, 96 px from the bottom |
| Menu bar | 28 px high |
| Window title bar | 40 px high, traffic lights 12 px, 8 px apart |
| Docked compact window (desktop demo) | 392 px wide, inside the window at the right |
| Menu dropdown / status popover | 250 px / 310 px wide at least |

These are measures, not a scale.

## Components

The design system has a folder per component with a README (behaviour, props, rules) and a live
preview. Read the folder before you change or add the component.

| In the design system | In `apps/ui/src/shared/ui/` |
|---|---|
| `Button` (variants primary, secondary, ghost, danger; size sm), `Banner`, `ActionErrorBanner`, `Toast`, `Tabs`, `Dialog`, `EmptyState`, `ErrorState`, `LoadingState`, `NotAvailable`, `AsyncContent`, `Page`, `Section`, `Chip`, `Segmented`, `Meter`, `KeyCap`, the line icons (`Icon`) | exists (M1-51) |
| `Toggle`, `Field` | in the wireframe, not built yet (X-50) |
| `Window`, `MenuBar`, `Menu`, `StatusPopover` | desktop shell; the app supplies the menus and the status item |

A control with no behaviour yet is a real `disabled` control described as "Not available yet"
(`Button` prop `unavailable`, or `useUnavailable` for other elements), shows no value, and a
`Meter` in that state is an empty track.

## What the code does not match yet

`apps/ui/src/styles.css` and `apps/ui/src/styles/tokens.css` use the tokens of this file (M1-51);
`tokens.test.ts` keeps the values equal to the tables above and every text pair at 4.5:1, and
`noLiterals.test.ts` keeps literal colours, radii, shadows and type sizes out of every other file.
What is still open:

| Area | In the code today | Should be |
|---|---|---|
| Layout of Live, Setup, Meetings and detail, Workspace, settings sheet and dialogs | the old single-column layout, restyled with the tokens | the wireframe layout (tickets M1-51a to M1-51e) |
| `Toggle`, `Field` | not built | the wireframe components (X-50) |
