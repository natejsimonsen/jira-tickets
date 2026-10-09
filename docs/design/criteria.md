# Jira Tickets: design criteria (acceptance bar for UI/UX ratings)

Chosen philosophy: **minimal and clean**, in the spirit of github-rs / GitHub Primer. Light and dark themes
carry equal weight; a flow scores 10 only when both do.

## What "minimal and clean" means here

1. **One accent.** `accent` is the only saturated UI colour. Every other colour carries meaning: status tint,
   epic hue, health (on/at-risk/off), danger. Decorative colour is a defect.
2. **Quiet neutrals.** Backgrounds step `bg` → `surface` → `surface_2`; borders are 1 px `border`; text is
   `text` with `muted` for secondary. No gradients, no drop shadows except the toast and modal backdrop.
3. **One spacing scale.** 4 / 8 / 12 / 16 / 24 px. Paddings inside like components are identical
   (every pill, every card, every panel margin). Rows align to a shared left edge; pills align to a baseline.
4. **One type scale.** Body 14, small 12, mono 12.5 for keys/ids, bold 13.5 for row titles, 22 for the
   detail title, 15 for the app title. No other sizes without a reason the rater can see.
5. **One corner radius family.** 6 px for buttons, cards, dialogs; 10 px for pills; 4 px for picker rows.
6. **Whitespace does the grouping.** Prefer space over borders; when a border is used it is `border`, 1 px.
7. **Keyboard-first with visible affordances.** Every key has a visible hint somewhere (hint bar, tooltip,
   help). Focus and selection states are unmistakable but calm (`selected_row` fill, not a bright outline).
8. **Text is the UI.** Icons only where github-rs would use one (sync, chevrons). Labels are short,
   sentence case, no trailing colons, no exclamation marks.
9. **Nothing jumps.** Loading states hold their slot (spinner replaces control at same size); lists keep
   previous rows while updating; toasts do not shift layout.
10. **Density is moderate.** Rows ~44 px two-line; lists breathe but a 1440×900 window shows ≥ 14 rows.

## How raters score (1–10)

- 10: nothing to change; every element follows the rules above in both themes.
- 8–9: polish only (a padding off by 4 px, a label wording, a colour a shade off).
- 6–7: one visible inconsistency a user would notice (misaligned columns, inconsistent pill padding,
  unclear active state).
- ≤ 5: a usability problem (unclear what to do next, information hidden, contrast below 4.5:1, layout jump).

Raters judge only the chunk they are given. Each finding states: what, where (screenshot + region),
why it breaks a rule above, and the smallest fix.

## Standing rulings (do not re-flag)

These were decided in earlier rounds; raters treat them as correct and do not score against them.

- **Spinners.** The header sync spinner is for ⌘R (and the sync after a write) only; a list, filter or search
  load spins inline in the count row. A spinner takes the text colour of the slot it replaces.
- **Stale count.** While a list is updating, the count is `muted` (the rows on screen are stale); it returns to
  `text` when the result lands.
- **Backdrop.** The dark modal backdrop alpha is 110 and final; light is 48.
- **Placeholders.** Every placeholder uses the `placeholder` token, a step dimmer than `muted`, in both themes, so
  an empty field never reads as filled in.
- **"Only opened"** mirrors the Jira board's quick filter of that name; the wording stays.
- **Dialog edge.** One dialog edge token is shared by every modal, the go-to box included; it may differ from
  `border`.
- **Read-only composer.** In read-only mode the detail pane shows only the muted line "Read-only, comments can't
  be posted" under Comments; composer captures use `JIRA_TICKETS_SNAPSHOT_COMPOSER=1` (writes still blocked).
- **Launch revalidation is a sync.** The refresh at launch over cached rows spins the header (like ⌘R) and the
  count stays full `text` colour: cached rows are real data, not stale filter results.
- **⌘1–4 and ⌘R** are discoverable through the tab and sync tooltips and the help overlay, not the hint bar.
- **Header right cluster.** "?" and sync are outermost at the 16 px right edge; the name and "Read-only" sit to
  their left and grow leftward, so the buttons never move whatever the name.
