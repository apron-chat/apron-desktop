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
        t.colors.danger = rgb(if dark { 0xff9696 } else { 0xb42318 }).into();
        t.colors.border = rgb(if dark { 0x30394a } else { 0xdce2ed }).into();
        t.colors.input = t.colors.border;
        t.colors.accent = rgb(if dark { 0x293752 } else { 0xe2eafb }).into();
        t.colors.accent_foreground = t.colors.foreground;
        t.colors.primary = accent;
        t.colors.ring = accent;
        t.colors.primary_hover = rgb(if dark { 0xa7bdff } else { 0x2d50b0 }).into();
        t.colors.primary_active = rgb(if dark { 0x7e9cf0 } else { 0x244391 }).into();
        t.colors.primary_foreground = rgb(if dark { 0x101a35 } else { 0xffffff }).into();
        t.colors.button_primary = accent;
        t.colors.button_primary_hover = t.colors.primary_hover;
        t.colors.button_primary_active = t.colors.primary_active;
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

pub const CONTROL: f32 = 32.;
pub const ROW: f32 = 36.;
pub const SIDEBAR: f32 = 256.;
pub const COMPACT: f32 = 900.;
pub fn button(id: impl Into<ElementId>) -> component::button::Button {
    use component::Sizable;
    component::button::Button::new(id).small().h(px(CONTROL))
}
pub fn surface(cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded_lg()
        .bg(cx.theme().background)
        .border_1()
        .border_color(cx.theme().border)
}
pub fn gallery(window: &mut Window, cx: &mut App) {
    use component::{
        Disableable, Selectable, WindowExt,
        button::ButtonVariants,
        input::{Input, InputState},
    };
    if window.has_active_dialog(cx) {
        return;
    }
    let field = cx.new(|cx| InputState::new(window, cx).placeholder("Room title"));
    window.open_dialog(cx, move |dialog,_,cx| dialog.title("Apron component gallery").child(surface(cx)
        .child("Controls · focus with Tab, activate with Enter")
        .child(div().flex().gap_2().flex_wrap()
            .child(button("g-primary").primary().label("Primary"))
            .child(button("g-normal").label("Secondary"))
            .child(button("g-selected").ghost().selected(true).label("Selected"))
            .child(button("g-disabled").disabled(true).label("Unavailable")))
        .child("Room title")
        .child(Input::new(&field).aria_label("Room title").w_full())
        .child(div().p_2().rounded_md().bg(cx.theme().accent).child("# Selected room · 3 unread · @1"))
        .child(div().text_color(cx.theme().muted_foreground).child("Loading… · No matching rooms"))
        .child(div().text_color(cx.theme().danger).child("Change was refused. Your input is retained."))
        .child(div().text_sm().child("Long text wraps inside a surface and leaves actions reachable. Controls share toolkit keyboard, focus and disabled behavior."))
        ).footer(button("g-close").label("Close gallery").on_click(|_,w,cx|w.close_dialog(cx))));
}

pub fn dialog_body_height(window: &Window) -> Pixels {
    (window.viewport_size().height - px(210.))
        .max(px(160.))
        .min(px(560.))
}
