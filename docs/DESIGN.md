# DiskPilot design system

DiskPilot is a system utility that people trust with their files. The
interface should read like a well-made native Mac app: calm, dense where data
is dense, and explicit about risk. Brand personality lives in the mascot and
the app icon; the working interface stays quiet.

## Principles

1. **Answer first.** Every screen opens with the answer to its question
   (how much is used, what can be recovered, what to look at first), then
   the detail.
2. **Risk is always visible.** Anything that can be removed shows its tier
   (Likely safe, Review first, Keep) and the reason. Nothing is called safe
   without evidence.
3. **One accent, used for one job.** Indigo marks the primary action and the
   current selection. Status colors mean status and nothing else. Red is for
   destructive actions only.
4. **Lines before boxes.** Group with spacing and hairline dividers. A border
   and background mark something as a separate object (a sheet, a menu, the
   storage strip); most content needs neither.
5. **Motion answers actions.** Hover and press feedback, expanding rows,
   sheets and progress. The one unprompted moment is the storage strip
   filling after a scan.

## Color

System grays with a slight cool bias, matched to macOS so the app sits
naturally next to Finder and System Settings.

| Token                | Light     | Dark      | Use                                      |
| -------------------- | --------- | --------- | ---------------------------------------- |
| `background`         | `#FFFFFF` | `#1C1C1E` | Content area                             |
| `sidebar`            | `#F5F5F7` | `#161618` | Sidebar                                  |
| `panel`              | `#FFFFFF` | `#1C1C1E` | Surfaces that sit on the background      |
| `panel-raised`       | `#F5F5F7` | `#252528` | Sheets, menus, inset wells               |
| `hover`              | `#EDEDF0` | `#2A2A2D` | Row and control hover                    |
| `selected`           | `#E4E4EA` | `#323236` | Selected row, active navigation          |
| `border`             | `#E5E5EA` | `#2E2E31` | Hairline dividers                        |
| `border-strong`      | `#D1D1D6` | `#3A3A3D` | Control outlines                         |
| `foreground`         | `#1D1D1F` | `#F5F5F7` | Primary text                             |
| `muted-foreground`   | `#6E6E73` | `#A1A1A6` | Secondary text                           |
| `subtle-foreground`  | `#737378` | `#8E8E93` | Tertiary text, metadata (4.5:1 or more)  |
| `accent`             | `#5856D6` | `#7D7AFF` | Primary action, selection, focus         |
| `tier-safe`          | `#248A3D` | `#30D158` | Likely safe                              |
| `tier-review`        | `#B25000` | `#FF9F0A` | Review first                             |
| `tier-keep`          | `#6E6E73` | `#98989D` | Keep                                     |
| `critical`           | `#D70015` | `#FF453A` | Destructive actions and errors           |

The accent is the indigo of the mascot's goggles strap, which is also the
macOS system indigo. Category colors for charts keep the validated palette in
`src/styles/globals.css`; identity is always carried by a label too.

## Typography

The system font (SF Pro on macOS). SF Mono only for paths, commands and
identifiers. Sizes and counts use tabular figures in the text face.

| Role         | Size / line height | Weight |
| ------------ | ------------------ | ------ |
| Display      | 28 / 32            | 600    |
| Page title   | 15 / 20            | 600    |
| Section      | 13 / 18            | 600    |
| Body         | 13 / 18            | 400    |
| Secondary    | 12 / 16            | 400    |
| Caption      | 11 / 14            | 400    |

No all-caps labels. Section names are sentence case.

## Space, radius, elevation

- Spacing on a 4 px grid: 4, 8, 12, 16, 24, 32. Page gutters are 24 px.
- Radii: 4 for tags and small controls, 6 for buttons, inputs and rows,
  10 for panels, sheets and dialogs.
- No shadows on content. Menus, tooltips and dialogs get one soft shadow so
  they read as floating.

## Components

- **Buttons**: primary (accent fill), secondary (outline), ghost (text),
  destructive (red fill, only inside a confirmation). Heights 24 / 28 / 32.
- **Tier tag**: icon + label in the tier color on a faint tint. The only
  colored tag in the app.
- **Category label**: a colored dot and plain text. Never a filled pill.
- **Size cell**: tabular figure, right aligned, with an optional 2 px meter.
- **Rows**: 32 px for tables, 44 px for two-line rows. Hover tint, selected
  fill, focus ring on keyboard focus.
- **States**: skeletons that mirror the final layout while loading; empty
  states that say what will appear and offer the one action that fills
  them; errors that say what happened and how to fix it.

## Motion

- 120 ms ease-out for hover and press, 180 ms for expand and collapse,
  220 ms for sheets and dialogs.
- Everything respects `prefers-reduced-motion`.

## Icons

Lucide at 16 px in navigation and 14 px inline, 1.75 stroke. Installed apps
show their own icon from macOS. No emoji.
