/* Isolated headless test harness for Weston 14, never a desktop plugin.
 * Function signatures verified against Weston 14.0.2 libweston-internal.h.
 * These exported internal APIs are version-specific. No hardware access.
 */
#include <stdlib.h>
#include <linux/input-event-codes.h>
#include <libweston/libweston.h>
extern void weston_seat_init(struct weston_seat *, struct weston_compositor *, const char *);
extern void weston_seat_release(struct weston_seat *);
extern int weston_seat_init_pointer(struct weston_seat *);
extern int weston_seat_init_keyboard(struct weston_seat *, struct xkb_keymap *);
extern void notify_key(struct weston_seat *, const struct timespec *, uint32_t,
                       enum wl_keyboard_key_state, enum weston_key_state_update);
struct test_seat {
    struct weston_seat seat;
    struct wl_listener destroy;
    struct wl_event_source *input_timer;
    unsigned attempts;
};
/* Opt-in, test-only: supply one genuine compositor input serial, allowing a
 * clipboard ownership roundtrip. Never enable during benchmarks. */
static int supply_input_serial(void *data) {
    struct test_seat *s = data;
    struct weston_keyboard *keyboard = weston_seat_get_keyboard(&s->seat);
    if (keyboard && keyboard->focus) {
        struct timespec time;
        clock_gettime(CLOCK_MONOTONIC, &time);
        notify_key(&s->seat, &time, KEY_LEFTSHIFT, WL_KEYBOARD_KEY_STATE_PRESSED, STATE_UPDATE_AUTOMATIC);
        notify_key(&s->seat, &time, KEY_LEFTSHIFT, WL_KEYBOARD_KEY_STATE_RELEASED, STATE_UPDATE_AUTOMATIC);
        weston_log("Apron isolated test seat supplied one input serial\n");
        return 0;
    }
    if (++s->attempts < 100) wl_event_source_timer_update(s->input_timer, 100);
    return 0;
}
static void cleanup(struct wl_listener *listener, void *unused) {
    struct test_seat *s = wl_container_of(listener, s, destroy);
    (void)unused;
    if (s->input_timer) wl_event_source_remove(s->input_timer);
    wl_list_remove(&s->destroy.link);
    weston_seat_release(&s->seat);
    free(s);
}
WL_EXPORT int wet_module_init(struct weston_compositor *compositor, int *argc, char *argv[]) {
    (void)argc; (void)argv;
    struct test_seat *s = calloc(1, sizeof *s);
    if (!s) return -1;
    weston_seat_init(&s->seat, compositor, "apron-isolated-test-seat");
    if (weston_seat_init_pointer(&s->seat) < 0 || weston_seat_init_keyboard(&s->seat, NULL) < 0) {
        weston_seat_release(&s->seat); free(s); return -1;
    }
    s->destroy.notify = cleanup;
    wl_signal_add(&compositor->destroy_signal, &s->destroy);
    if (getenv("APRON_TEST_SEAT_INPUT")) {
        s->input_timer = wl_event_loop_add_timer(wl_display_get_event_loop(compositor->wl_display), supply_input_serial, s);
        if (s->input_timer) wl_event_source_timer_update(s->input_timer, 100);
    }
    return 0;
}
