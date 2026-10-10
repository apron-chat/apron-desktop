//! Apron's restrained, opaque Linux palette. No blur passes or animation timers.
use gpui_kit::{
    component::{ActiveTheme, Theme},
    *,
};
pub fn apply(cx: &mut App) {
    let dark = cx.theme().is_dark();
    let accent: Hsla = rgb(if dark { 0x91adff } else { 0x365dc6 }).into();
    if cx.theme().primary == accent {
        return;
    }
    Theme::update(cx, |t| {
        t.radius = px(8.);
        t.radius_lg = px(14.);
        t.colors.background = rgb(if dark { 0x181c24 } else { 0xfcfcfe }).into();
        t.colors.foreground = rgb(if dark { 0xe4e8f0 } else { 0x242d40 }).into();
        t.colors.sidebar = rgb(if dark { 0x12161e } else { 0xf0f3f9 }).into();
        t.colors.muted = rgb(if dark { 0x222937 } else { 0xeef1f7 }).into();
        t.colors.muted_foreground = rgb(if dark { 0x9aa8bd } else { 0x606e85 }).into();
        t.colors.border = rgb(if dark { 0x30394a } else { 0xdce2ed }).into();
        t.colors.input = t.colors.border;
        t.colors.accent = rgb(if dark { 0x293752 } else { 0xe2eafb }).into();
        t.colors.accent_foreground = t.colors.foreground;
        t.colors.primary = accent;
        t.colors.ring = accent;
        t.colors.primary_hover = accent;
        t.colors.primary_active = accent;
        t.colors.primary_foreground = rgb(if dark { 0x101a35 } else { 0xffffff }).into();
        t.colors.button_primary = accent;
        t.colors.button_primary_hover = accent;
        t.colors.button_primary_active = accent;
        t.colors.button_primary_foreground = t.colors.primary_foreground;
    });
}
pub fn avatar(name: &str, cx: &App) -> Hsla {
    let colors = if cx.theme().is_dark() {
        [0x91adff, 0x8fd0ba, 0xddb18c, 0xc3a6ef]
    } else {
        [0x365dc6, 0x24745c, 0x96571f, 0x7c4eaf]
    };
    let hash = name
        .bytes()
        .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize));
    rgb(colors[hash % colors.len()]).into()
}
