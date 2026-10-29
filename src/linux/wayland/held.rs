//! Keys a forwarder sends without `wl_keyboard.modifiers`.

use xkbcommon::xkb::{self, Keycode, State};

/// The keys down, which xkb's own rules turn into modifiers.
#[derive(Default)]
pub(super) struct HeldKeys {
    down: Vec<Keycode>,
}

impl HeldKeys {
    /// The compositor's mask: the held keys pressed again under it.
    pub fn sent(&mut self, state: &mut State, mask: [u32; 4]) {
        let [depressed, latched, locked, group] = mask;
        *state = State::new(&state.get_keymap());
        state.update_mask(0, 0, locked, 0, 0, group);
        let mut kept = Vec::new();
        for keycode in self.down.drain(..) {
            let before = state.serialize_mods(xkb::STATE_MODS_DEPRESSED);
            state.update_key(keycode, xkb::KeyDirection::Down);
            let added = state.serialize_mods(xkb::STATE_MODS_DEPRESSED) & !before;
            if added & !depressed == 0 {
                kept.push(keycode);
            } else {
                // The mask says it went up.
                state.update_key(keycode, xkb::KeyDirection::Up);
            }
        }
        self.down = kept;
        // The mask wins; each key keeps what its release clears.
        state.update_mask(depressed, latched, locked, 0, 0, group);
    }

    /// A key's press or release, moving the state as xkb does.
    pub fn key(&mut self, state: &mut State, keycode: Keycode, pressed: bool) {
        let held = self.down.contains(&keycode);
        if pressed && held {
            // Its release never came.
            state.update_key(keycode, xkb::KeyDirection::Up);
        }
        self.down.retain(|down| *down != keycode);
        if pressed {
            self.down.push(keycode);
        }
        let direction = match pressed {
            true => xkb::KeyDirection::Down,
            false => xkb::KeyDirection::Up,
        };
        state.update_key(keycode, direction);
    }

    /// Focus gained with these keys down; the mask follows.
    pub fn entered(&mut self, down: Vec<Keycode>) {
        self.down = down;
    }

    /// Focus lost: no key is down here.
    pub fn clear(&mut self) {
        self.down.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::platform::modifiers_from_xkb;

    const SHIFT_L: u32 = 42 + 8;
    const SHIFT_R: u32 = 54 + 8;
    const CONTROL_L: u32 = 29 + 8;
    const CAPS: u32 = 58 + 8;
    const LWIN: u32 = 125 + 8;
    const RWIN: u32 = 126 + 8;
    const RALT: u32 = 100 + 8;
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
        held.entered(vec![Keycode::from(SHIFT_L), Keycode::from(SHIFT_R)]);
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
    fn keys_held_in_a_group_are_pressed_again_in_it() {
        // German group: Left Win and Right Alt are AltGr.
        let (mut held, mut state) = (HeldKeys::default(), layouts("us,de", "lv3:lwin_switch"));
        held.entered(vec![Keycode::from(LWIN), Keycode::from(RALT)]);
        sent(&mut held, &mut state, 0x80, 1);
        key(&mut held, &mut state, LWIN, false);
        assert_eq!(typed(&state, Q), "@");
    }
}
