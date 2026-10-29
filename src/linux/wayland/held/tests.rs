use super::*;
use crate::linux::platform::modifiers_from_xkb;

const SHIFT_L: u32 = 42 + 8;
const SHIFT_R: u32 = 54 + 8;
const CONTROL_L: u32 = 29 + 8;
const CAPS: u32 = 58 + 8;
const LWIN: u32 = 125 + 8;
const RWIN: u32 = 126 + 8;
const RALT: u32 = 100 + 8;
const LALT: u32 = 56 + 8;
const MENU: u32 = 127 + 8;
const RCONTROL: u32 = 97 + 8;
const A: u32 = 30 + 8;
const C: u32 = 46 + 8;
const Q: u32 = 16 + 8;

fn layouts(layouts: &str, options: &str) -> State {
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let options = Some(options.to_string()).filter(|options| !options.is_empty());
    let keymap = xkb::Keymap::new_from_names(&context, "", "", layouts, "", options, 0)
        .expect("the keymap compiles");
    State::new(&keymap)
}

fn us() -> State {
    layouts("us", "")
}

fn key(held: &mut HeldKeys, state: &mut State, keycode: u32, pressed: bool) {
    held.key(state, Keycode::from(keycode), pressed);
}

fn sent(held: &mut HeldKeys, state: &mut State, depressed: u32, group: u32) {
    held.sent(state, [depressed, 0, 16, group]);
}

fn typed(state: &State, keycode: u32) -> String {
    state.key_get_utf8(Keycode::from(keycode))
}

#[test]
fn shift_held_by_its_key_alone_types_a_capital() {
    let (mut held, mut state) = (HeldKeys::default(), us());
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_L, true);
    assert_eq!(typed(&state, A), "A");
    assert!(modifiers_from_xkb(&state).shift);
    key(&mut held, &mut state, SHIFT_L, false);
    assert_eq!(typed(&state, A), "a");
    assert!(!modifiers_from_xkb(&state).shift);
}

#[test]
fn shift_after_the_mask_said_control_joins_it() {
    // The reporter's log: modifiers(4), then Shift_L's key alone, then c.
    let (mut held, mut state) = (HeldKeys::default(), us());
    key(&mut held, &mut state, CONTROL_L, true);
    sent(&mut held, &mut state, 4, 0);
    key(&mut held, &mut state, SHIFT_L, true);
    let modifiers = modifiers_from_xkb(&state);
    assert!(modifiers.control && modifiers.shift, "{modifiers:?}");
    assert_eq!(typed(&state, C), "\u{3}");
}

#[test]
fn control_by_its_key_alone_joins_the_mask_s_shift() {
    let (mut held, mut state) = (HeldKeys::default(), us());
    sent(&mut held, &mut state, 1, 0);
    key(&mut held, &mut state, CONTROL_L, true);
    let modifiers = modifiers_from_xkb(&state);
    assert!(modifiers.control && modifiers.shift, "{modifiers:?}");
}

#[test]
fn the_mask_outweighs_a_release_never_heard() {
    // Shift_L's release swallowed; the next mask lacks Shift.
    let (mut held, mut state) = (HeldKeys::default(), us());
    key(&mut held, &mut state, SHIFT_L, true);
    sent(&mut held, &mut state, 4, 0);
    assert!(!modifiers_from_xkb(&state).shift);
    // Pressed again, it counts, and its release lets go.
    key(&mut held, &mut state, SHIFT_L, true);
    assert!(modifiers_from_xkb(&state).shift);
    key(&mut held, &mut state, SHIFT_L, false);
    let modifiers = modifiers_from_xkb(&state);
    assert!(!modifiers.shift && modifiers.control, "{modifiers:?}");
}

#[test]
fn one_shift_let_go_leaves_the_other_held() {
    let (mut held, mut state) = (HeldKeys::default(), us());
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_L, true);
    key(&mut held, &mut state, SHIFT_R, true);
    key(&mut held, &mut state, SHIFT_L, false);
    assert!(modifiers_from_xkb(&state).shift);
}

#[test]
fn a_compositor_that_sends_the_mask_is_followed_as_before() {
    let (mut held, mut state) = (HeldKeys::default(), us());
    key(&mut held, &mut state, SHIFT_L, true);
    sent(&mut held, &mut state, 1, 0);
    assert!(modifiers_from_xkb(&state).shift);
    key(&mut held, &mut state, SHIFT_L, false);
    sent(&mut held, &mut state, 0, 0);
    assert!(!modifiers_from_xkb(&state).shift);
    key(&mut held, &mut state, A, true);
    assert!(!modifiers_from_xkb(&state).shift);
}

#[test]
fn a_mask_drops_the_keys_it_leaves_out() {
    // Left Shift's release swallowed, a mask without Shift, Right Shift.
    let (mut held, mut state) = (HeldKeys::default(), us());
    key(&mut held, &mut state, SHIFT_L, true);
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_R, true);
    key(&mut held, &mut state, SHIFT_R, false);
    assert_eq!(typed(&state, A), "a");
}

#[test]
fn keys_held_on_entering_stay_held() {
    // Both Shifts held on entering; one goes up, mask unchanged.
    let (mut held, mut state) = (HeldKeys::default(), us());
    held.unseen(vec![Keycode::from(SHIFT_L), Keycode::from(SHIFT_R)]);
    sent(&mut held, &mut state, 1, 0);
    key(&mut held, &mut state, SHIFT_L, false);
    assert_eq!(typed(&state, A), "A");
}

#[test]
fn a_new_keymap_keeps_the_keys_held() {
    let (mut held, mut state) = (HeldKeys::default(), us());
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_L, true);
    key(&mut held, &mut state, SHIFT_R, true);
    state = us();
    held.rekeyed();
    sent(&mut held, &mut state, 1, 0);
    key(&mut held, &mut state, SHIFT_L, false);
    assert_eq!(typed(&state, A), "A");
}

#[test]
fn a_lock_key_locks() {
    let (mut held, mut state) = (HeldKeys::default(), us());
    held.sent(&mut state, [0, 0, 0, 0]);
    key(&mut held, &mut state, CAPS, true);
    key(&mut held, &mut state, CAPS, false);
    assert_eq!(typed(&state, A), "A");
}

#[test]
fn a_key_holds_what_the_keymap_maps_it_to() {
    // altwin:meta_win puts Meta on the Windows keys, as Mod4.
    let (mut held, mut state) = (HeldKeys::default(), layouts("us", "altwin:meta_win"));
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, LWIN, true);
    let first = modifiers_from_xkb(&state);
    key(&mut held, &mut state, RWIN, true);
    let modifiers = modifiers_from_xkb(&state);
    assert!(!modifiers.alt && modifiers.platform, "{modifiers:?}");
    assert_eq!(modifiers, first);
}

#[test]
fn a_key_that_types_at_its_level_holds_nothing() {
    // Shift with Right Alt is Multi_key under ralt_switch_multikey.
    let (mut held, mut state) = (
        HeldKeys::default(),
        layouts("us", "lv3:ralt_switch_multikey"),
    );
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_L, true);
    key(&mut held, &mut state, RALT, true);
    let modifiers = modifiers_from_xkb(&state);
    assert!(modifiers.shift && !modifiers.alt, "{modifiers:?}");
    assert_eq!(typed(&state, A), "A");
}

#[test]
fn a_release_clears_what_its_press_set() {
    // AltGr down, Shift down, AltGr up as Multi_key, Shift up.
    let (mut held, mut state) = (
        HeldKeys::default(),
        layouts("de", "lv3:ralt_switch_multikey"),
    );
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, RALT, true);
    assert_eq!(typed(&state, Q), "@");
    key(&mut held, &mut state, SHIFT_L, true);
    key(&mut held, &mut state, RALT, false);
    key(&mut held, &mut state, SHIFT_L, false);
    assert_eq!(typed(&state, Q), "q");
}

#[test]
fn a_group_switch_holds_no_modifier() {
    // Ctrl with Left Win picks the first group.
    let (mut held, mut state) = (
        HeldKeys::default(),
        layouts("us,de", "grp:lctrl_lwin_rctrl_menu"),
    );
    key(&mut held, &mut state, CONTROL_L, true);
    sent(&mut held, &mut state, 4, 0);
    key(&mut held, &mut state, LWIN, true);
    let modifiers = modifiers_from_xkb(&state);
    assert!(modifiers.control && !modifiers.platform, "{modifiers:?}");
}

#[test]
fn keys_held_on_entering_are_pressed_in_the_mask_s_group() {
    // German group: Left Win and Right Alt are AltGr.
    let (mut held, mut state) = (HeldKeys::default(), layouts("us,de", "lv3:lwin_switch"));
    held.unseen(vec![Keycode::from(LWIN), Keycode::from(RALT)]);
    sent(&mut held, &mut state, 0x80, 1);
    key(&mut held, &mut state, LWIN, false);
    assert_eq!(typed(&state, Q), "@");
}

#[test]
fn a_key_keeps_the_group_it_was_pressed_in() {
    // Both Alts held; Menu toggles to German; Left Alt up.
    let (mut held, mut state) = (HeldKeys::default(), layouts("us,de", "grp:menu_toggle"));
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, LALT, true);
    key(&mut held, &mut state, RALT, true);
    key(&mut held, &mut state, MENU, true);
    key(&mut held, &mut state, MENU, false);
    assert_eq!(state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE), 1);
    sent(&mut held, &mut state, 8, 1);
    key(&mut held, &mut state, LALT, false);
    assert!(modifiers_from_xkb(&state).alt);
}

#[test]
fn a_latch_is_read_before_the_key_that_takes_it() {
    // AltGr held, Caps pressed: level three latches for one key.
    let (mut held, mut state) = (HeldKeys::default(), layouts("de", "lv3:caps_switch_latch"));
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, RALT, true);
    key(&mut held, &mut state, CAPS, true);
    key(&mut held, &mut state, CAPS, false);
    key(&mut held, &mut state, RALT, false);
    assert_eq!(typed(&state, Q), "@");
    key(&mut held, &mut state, Q, true);
    assert_eq!(typed(&state, Q), "q");
}

#[test]
fn a_key_held_unseen_outlasts_another_s_tap() {
    // Left Shift held into the window; Right Shift tapped.
    let (mut held, mut state) = (HeldKeys::default(), us());
    held.unseen(vec![Keycode::from(SHIFT_L)]);
    sent(&mut held, &mut state, 1, 0);
    key(&mut held, &mut state, SHIFT_R, true);
    key(&mut held, &mut state, SHIFT_R, false);
    assert_eq!(typed(&state, A), "A");
}

#[test]
fn a_mask_releases_a_key_sharing_a_modifier() {
    // Right Shift's release lost after Left Shift's came.
    let (mut held, mut state) = (HeldKeys::default(), us());
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_L, true);
    key(&mut held, &mut state, SHIFT_R, true);
    key(&mut held, &mut state, SHIFT_L, false);
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_L, true);
    key(&mut held, &mut state, SHIFT_L, false);
    assert_eq!(typed(&state, A), "a");
}

#[test]
fn a_focus_lost_leaves_no_key_down() {
    // Left Shift goes up outside the window.
    let (mut held, mut state) = (HeldKeys::default(), us());
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_L, true);
    held.clear();
    held.unseen(Vec::new());
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, SHIFT_R, true);
    key(&mut held, &mut state, SHIFT_R, false);
    assert_eq!(typed(&state, A), "a");
}

#[test]
fn a_lock_replays_under_the_locks_it_went_down_under() {
    // Caps, then both Shifts, which lock Caps; two released.
    let (mut held, mut state) = (HeldKeys::default(), layouts("us", "shift:both_capslock"));
    let mut compositor = Compositor::new(layouts("us", "shift:both_capslock"));
    for (keycode, pressed) in [
        (CAPS, true),
        (SHIFT_L, true),
        (SHIFT_R, true),
        (SHIFT_L, false),
        (CAPS, false),
    ] {
        compositor.play(&mut held, &mut state, keycode, pressed);
    }
    assert_eq!(typed(&state, A), typed(&compositor.state, A));
}

#[test]
fn keys_held_on_entering_keep_the_first_mask_s_group() {
    // Both Alts held into the window; Menu toggles to German.
    let (mut held, mut state) = (HeldKeys::default(), layouts("us,de", "grp:menu_toggle"));
    held.unseen(vec![Keycode::from(LALT), Keycode::from(RALT)]);
    sent(&mut held, &mut state, 8, 0);
    key(&mut held, &mut state, MENU, true);
    key(&mut held, &mut state, MENU, false);
    sent(&mut held, &mut state, 8, 1);
    key(&mut held, &mut state, LALT, false);
    assert!(modifiers_from_xkb(&state).alt);
}

#[test]
fn a_new_keymap_s_mask_names_the_held_keys_group() {
    // Right Alt held in German; keymap becomes us,de, group 1.
    let (mut held, mut state) = (HeldKeys::default(), layouts("de", "lv3:lwin_switch"));
    sent(&mut held, &mut state, 0, 0);
    key(&mut held, &mut state, RALT, true);
    state = layouts("us,de", "lv3:lwin_switch");
    held.rekeyed();
    sent(&mut held, &mut state, 0x80, 1);
    key(&mut held, &mut state, LWIN, true);
    key(&mut held, &mut state, LWIN, false);
    assert_eq!(typed(&state, Q), "@");
}

fn direction(pressed: bool) -> xkb::KeyDirection {
    match pressed {
        true => xkb::KeyDirection::Down,
        false => xkb::KeyDirection::Up,
    }
}

/// A compositor's own state, sending its mask when it changes.
struct Compositor {
    state: State,
    sent: Option<[u32; 4]>,
}

impl Compositor {
    fn new(state: State) -> Self {
        Self { state, sent: None }
    }

    /// One key through the compositor and client, mask and all.
    fn play(&mut self, held: &mut HeldKeys, state: &mut State, keycode: u32, pressed: bool) {
        self.state
            .update_key(Keycode::from(keycode), direction(pressed));
        key(held, state, keycode, pressed);
        self.send(held, state);
    }

    /// The compositor's mask, sent when it changed.
    fn send(&mut self, held: &mut HeldKeys, state: &mut State) {
        let mask = [
            self.state.serialize_mods(xkb::STATE_MODS_DEPRESSED),
            self.state.serialize_mods(xkb::STATE_MODS_LATCHED),
            self.state.serialize_mods(xkb::STATE_MODS_LOCKED),
            self.state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE),
        ];
        if self.sent != Some(mask) {
            held.sent(state, mask);
            self.sent = Some(mask);
        }
    }

    /// What the client's state differs in, if anything.
    fn differs(&self, state: &State) -> Option<String> {
        let read = |state: &State| {
            (
                state.serialize_mods(xkb::STATE_MODS_EFFECTIVE),
                state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE),
                typed(state, A),
                typed(state, Q),
            )
        };
        let (want, got) = (read(&self.state), read(state));
        (want != got).then(|| format!("compositor {want:?}, client {got:?}"))
    }
}

/// Each keymap the fuzzing plays, with options that trip readings.
const KEYMAPS: [(&str, &str); 14] = [
    ("us", ""),
    ("de", "lv3:lalt_switch,lv3:caps_switch_latch"),
    ("us", "shift:breaks_caps"),
    ("us", "caps:shiftlock"),
    ("us", "shift:both_capslock_cancel"),
    ("us", "ctrl:swapcaps"),
    ("us,de", "grp:alt_shift_toggle"),
    ("us", "shift:both_capslock"),
    ("us,de", "grp:menu_toggle"),
    ("de", "lv3:ralt_switch_multikey"),
    ("us", "altwin:meta_win"),
    ("us,de", "lv3:lwin_switch"),
    ("de", "lv3:caps_switch_latch"),
    ("us,de", "grp:lctrl_lwin_rctrl_menu"),
];

/// Plays `steps` random keys of `seed`; `entering` first goes unseen.
fn fuzz(entering: usize, steps: usize) {
    let keys = [
        SHIFT_L, SHIFT_R, CONTROL_L, RCONTROL, LALT, RALT, LWIN, RWIN, CAPS, MENU, A, Q,
    ];
    for (names, options) in KEYMAPS {
        for seed in 0..200u64 {
            let mut random = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let mut compositor = Compositor::new(layouts(names, options));
            let (mut held, mut state) = (HeldKeys::default(), layouts(names, options));
            let mut down: Vec<u32> = Vec::new();
            let mut played = Vec::new();
            for step in 0..entering + steps {
                if step == entering {
                    // Focus comes: the keys down, then the mask.
                    held.unseen(down.iter().map(|key| Keycode::from(*key)).collect());
                    compositor.sent = None;
                    compositor.send(&mut held, &mut state);
                }
                random = random
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let keycode = keys[(random >> 33) as usize % keys.len()];
                let pressed = !down.contains(&keycode);
                match pressed {
                    true => down.push(keycode),
                    false => down.retain(|key| *key != keycode),
                }
                played.push((keycode, pressed));
                if step < entering {
                    compositor
                        .state
                        .update_key(Keycode::from(keycode), direction(pressed));
                    continue;
                }
                compositor.play(&mut held, &mut state, keycode, pressed);
                if let Some(difference) = compositor.differs(&state) {
                    panic!("{names} {options}, seed {seed}, after {played:?}: {difference}");
                }
            }
        }
    }
}

#[test]
fn keys_held_into_the_window_are_matched_at_every_key() {
    fuzz(6, 30);
}

#[test]
fn a_compositor_sending_its_mask_is_matched_at_every_key() {
    fuzz(0, 40);
}

/// Keys down in the compositor alone, then focus and mask.
fn focus(compositor: &mut Compositor, held: &mut HeldKeys, state: &mut State, keys: &[u32]) {
    for keycode in keys {
        compositor
            .state
            .update_key(Keycode::from(*keycode), direction(true));
    }
    held.unseen(keys.iter().map(|key| Keycode::from(*key)).collect());
    compositor.sent = None;
    compositor.send(held, state);
}

#[test]
fn a_replay_keeps_the_level_its_press_found() {
    // shift:breaks_caps: Shift with Lock held sets Shift and Lock.
    let options = "shift:breaks_caps";
    let (mut held, mut state) = (HeldKeys::default(), layouts("us", options));
    let mut compositor = Compositor::new(layouts("us", options));
    for (keycode, pressed) in [
        (CAPS, true),
        (SHIFT_R, true),
        (SHIFT_L, true),
        (SHIFT_R, false),
        (CAPS, false),
        (SHIFT_R, true),
        (SHIFT_R, false),
    ] {
        compositor.play(&mut held, &mut state, keycode, pressed);
        assert_eq!(compositor.differs(&state), None);
    }
}

#[test]
fn an_unseen_lock_key_keeps_what_it_holds() {
    // caps:shiftlock: Caps held into the window; both Shifts tapped.
    let (mut held, mut state) = (HeldKeys::default(), layouts("us", "caps:shiftlock"));
    let mut compositor = Compositor::new(layouts("us", "caps:shiftlock"));
    focus(&mut compositor, &mut held, &mut state, &[CAPS]);
    for (keycode, pressed) in [
        (SHIFT_R, true),
        (SHIFT_R, false),
        (SHIFT_L, true),
        (SHIFT_L, false),
    ] {
        compositor.play(&mut held, &mut state, keycode, pressed);
        assert_eq!(compositor.differs(&state), None);
    }
}

#[test]
fn an_unseen_lock_is_the_mask_s() {
    // shift:both_capslock: both Shifts and Caps held into the window.
    let options = "shift:both_capslock";
    let (mut held, mut state) = (HeldKeys::default(), layouts("us", options));
    let mut compositor = Compositor::new(layouts("us", options));
    focus(
        &mut compositor,
        &mut held,
        &mut state,
        &[SHIFT_L, SHIFT_R, CAPS],
    );
    compositor.play(&mut held, &mut state, SHIFT_R, false);
    assert_eq!(compositor.differs(&state), None);
}

#[test]
fn a_plain_key_held_unseen_pins_no_later_modifier() {
    // A held into the window; AltGr keys and a latch follow.
    let options = "lv3:lalt_switch,lv3:caps_switch_latch";
    let (mut held, mut state) = (HeldKeys::default(), layouts("de", options));
    let mut compositor = Compositor::new(layouts("de", options));
    focus(&mut compositor, &mut held, &mut state, &[A]);
    for (keycode, pressed) in [
        (RALT, true),
        (CAPS, true),
        (SHIFT_R, true),
        (SHIFT_R, false),
        (CAPS, false),
        (CAPS, true),
        (A, false),
        (LALT, true),
        (LALT, false),
        (RALT, false),
    ] {
        compositor.play(&mut held, &mut state, keycode, pressed);
        assert_eq!(compositor.differs(&state), None);
    }
}
