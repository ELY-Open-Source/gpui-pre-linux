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
    held.left(&mut state);
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

mod fuzz;
