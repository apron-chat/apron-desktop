# Apron native design system

Use GPUI Kit for buttons, input/textarea state, dialogs, popup menus and focus semantics.
`src/design.rs` owns the opaque light/dark palette and app-level metrics; do not fork
widget behavior or introduce polling/blur/animation to reproduce another client.

## Tokens and composition

Spacing follows 4 px increments: 4 compact, 8 related, 12 controls, 16 panels, 24 sections.
Body text uses the toolkit rem scale; metadata is small/muted, never the only indication
of selection or errors. Controls are at least 32 px high; room rows 36 px. Corners are
8 px on controls and 14 px on surfaces. Sidebar is 256 px when space permits.
Primary actions use the theme accent; destructive actions need an explicit label.
Light and dark palettes share semantic roles, not hard-coded per-component colors.

## Interaction rules

- Forms label every field, retain input on errors, disable duplicate submissions and
  state uncertain completion explicitly. Keep primary actions in the dialog footer
  when the body scrolls. Escape cancels; restore useful keyboard focus.
- Menus use Kit keyboard navigation and labels; icon-only controls require tooltip and
  accessibility label. Selection, hover, focus, disabled and busy are distinct states.
- Room rows preserve parent context and aggregate hidden attention. Search never loses
  a selected conversation or silently broadens message history.
- Composer starts compact, grows to five lines, then scrolls. Ctrl+Enter sends; plain
  Enter inserts a line. Reply, attachment and pending/error states remain explicit.
- Narrow layouts prioritize the conversation and return focus after choosing a room.
- Untrusted server strings are cleaned before rendering; external content needs an
  explicit action. No continuous background UI polling.

## Gallery and review

The conversation menu opens the native component/state gallery. Reviewed captures:
[light](docs/milestone/wide/report-gallery.png), [dark](docs/milestone/wide/report-gallery-dark.png),
[minimum size](docs/milestone/minimum/report-gallery.png). Review light/dark controls,
forms, list selection/attention, loading/disabled/error/empty/long-text states. The gallery
is a visual regression surface, not a claim of screen-reader or IME validation.

## Daily workflow forms

Dialog body height follows the viewport, with fixed action footers. Preferences
uses a storage-details disclosure so ordinary appearance/privacy controls remain
scannable at 640×480. Caption and attachment titles are separate labeled fields;
Rename changes the shared display title, never a local filename. File drag hover
uses the semantic accent and only stages files. Emoji search presents a bounded
page with explicit selection and Toggle, plus textual names and keyboard focus.

## Media and moderation

Image viewers use available window width, a viewport-aware raster area and wrapping
fixed footer controls. Opening is separate from loading; avatar preview is separate
from showing it in chat. Moderation review replaces editable fields with an exact
room/user/reason summary, and keeps Back/Cancel/Confirm in the footer. Destructive
confirmation uses the shared danger style and server errors preserve review context.
