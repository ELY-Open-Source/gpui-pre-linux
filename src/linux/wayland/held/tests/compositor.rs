use super::*;

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
        held.key(state, Keycode::from(keycode), pressed);
        self.send(held, state);
        // The next key comes: no mask came, the shadow's.
        held.settle(state);
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
    // A held into the window; AltGr and a latch follow.
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

#[test]
fn a_focus_lost_resets_the_keys_state() {
    // Shift pressed after Ctrl, let go away from the window.
    let options = "grp:ctrl_shift_toggle";
    let (mut held, mut state) = (HeldKeys::default(), layouts("us,de", options));
    let mut compositor = Compositor::new(layouts("us,de", options));
    for (keycode, pressed) in [(CONTROL_L, true), (SHIFT_L, true), (CONTROL_L, false)] {
        compositor.play(&mut held, &mut state, keycode, pressed);
    }
    held.left();
    compositor
        .state
        .update_key(Keycode::from(SHIFT_L), direction(false));
    focus(&mut compositor, &mut held, &mut state, &[]);
    for (keycode, pressed) in [(SHIFT_R, true), (SHIFT_L, true), (SHIFT_R, false)] {
        compositor.play(&mut held, &mut state, keycode, pressed);
        assert_eq!(compositor.differs(&state), None);
    }
}

#[test]
fn an_unseen_key_s_release_keeps_a_latch_s_history() {
    // Right Shift and A held in; AltGr, a latch follow.
    let options = "lv3:lalt_switch,lv3:caps_switch_latch";
    let (mut held, mut state) = (HeldKeys::default(), layouts("de", options));
    let mut compositor = Compositor::new(layouts("de", options));
    focus(&mut compositor, &mut held, &mut state, &[SHIFT_R, A]);
    for (keycode, pressed) in [
        (LALT, true),
        (CAPS, true),
        (RCONTROL, true),
        (RCONTROL, false),
        (SHIFT_R, false),
        (CAPS, false),
        (CAPS, true),
        (SHIFT_R, true),
        (CAPS, false),
    ] {
        compositor.play(&mut held, &mut state, keycode, pressed);
        assert_eq!(compositor.differs(&state), None);
    }
}

#[test]
fn a_key_the_compositor_takes_moves_only_its_masks() {
    // AltGr and Caps held; a media key the compositor binds.
    let options = "lv3:caps_switch_latch";
    let (mut held, mut state) = (HeldKeys::default(), layouts("de", options));
    let mut compositor = Compositor::new(layouts("de", options));
    let play = 172 + 8;
    for (keycode, pressed, seen) in [
        (RALT, true, true),
        (CAPS, true, true),
        (play, true, false),
        (play, false, false),
        (CAPS, false, true),
        (CAPS, true, true),
        (LALT, true, true),
        (RALT, false, true),
    ] {
        if seen {
            compositor.play(&mut held, &mut state, keycode, pressed);
        } else {
            compositor
                .state
                .update_key(Keycode::from(keycode), direction(pressed));
            compositor.send(&mut held, &mut state);
        }
        assert_eq!(compositor.differs(&state), None);
    }
}

/// us,de, with Menu latching the next group.
fn group_latch() -> State {
    let names = layouts("us,de", "").get_keymap();
    let text = names.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
    let start = text.find("key <COMP>").expect("a Menu key");
    let end = start + text[start..].find("};").expect("its end") + 2;
    let menu = "key <COMP> { [ ISO_Next_Group ], actions[Group1] = [ LatchGroup(group=+1) ] };";
    let text = format!("{}{menu}{}", &text[..start], &text[end..]);
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_string(&context, text, xkb::KEYMAP_FORMAT_TEXT_V1, 0)
        .expect("the latching keymap compiles");
    State::new(&keymap)
}

#[test]
fn a_mask_no_seen_key_explains_drops_a_stale_latch() {
    // Menu latches German; a taken key breaks it first.
    let (mut held, mut state) = (HeldKeys::default(), group_latch());
    let mut compositor = Compositor::new(group_latch());
    let (play, y) = (172 + 8, 21 + 8);
    for (keycode, pressed, seen) in [
        (MENU, true, true),
        (play, true, false),
        (play, false, false),
        (MENU, false, true),
        (Q, true, true),
        (Q, false, true),
        (y, true, true),
    ] {
        if seen {
            compositor.play(&mut held, &mut state, keycode, pressed);
        } else {
            compositor
                .state
                .update_key(Keycode::from(keycode), direction(pressed));
            compositor.send(&mut held, &mut state);
        }
        assert_eq!(
            compositor.differs(&state),
            None,
            "after {keycode} {pressed}"
        );
    }
}

#[test]
fn a_mask_after_a_seen_key_corrects_the_shadow_s_latch() {
    // Menu held, a taken key, Menu up, then q, y.
    let (mut held, mut state) = (HeldKeys::default(), group_latch());
    let mut compositor = Compositor::new(group_latch());
    let (play, y): (u32, u32) = (172 + 8, 21 + 8);
    compositor.play(&mut held, &mut state, MENU, true);
    compositor
        .state
        .update_key(Keycode::from(play), direction(true));
    compositor
        .state
        .update_key(Keycode::from(play), direction(false));
    compositor.send(&mut held, &mut state);
    for (keycode, pressed) in [(MENU, false), (Q, true), (Q, false), (y, true)] {
        compositor.play(&mut held, &mut state, keycode, pressed);
        assert_eq!(
            compositor.differs(&state),
            None,
            "after {keycode} {pressed}"
        );
    }
}

#[test]
fn a_latch_across_a_focus_change_moves_no_group_unasked() {
    // Menu tapped, focus out and in, Menu, a, then y.
    let (mut held, mut state) = (HeldKeys::default(), group_latch());
    let mut compositor = Compositor::new(group_latch());
    let y: u32 = 21 + 8;
    compositor.play(&mut held, &mut state, MENU, true);
    compositor.play(&mut held, &mut state, MENU, false);
    held.left();
    focus(&mut compositor, &mut held, &mut state, &[]);
    for (keycode, pressed) in [
        (MENU, true),
        (MENU, false),
        (A, true),
        (A, false),
        (y, true),
    ] {
        compositor.play(&mut held, &mut state, keycode, pressed);
        assert_eq!(
            compositor.differs(&state),
            None,
            "after {keycode} {pressed}"
        );
    }
}

mod fuzz;
