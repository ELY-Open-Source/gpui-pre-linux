//! Modifier keys a forwarder sends without `wl_keyboard.modifiers`.

use xkbcommon::xkb::{self, Keycode, State};

/// The compositor's last mask, and the modifier keys heard since.
#[derive(Default)]
pub(super) struct HeldMods {
    depressed: u32,
    latched: u32,
    locked: u32,
    group: u32,
    held: Vec<(Keycode, u32)>,
    touched: u32,
}

impl HeldMods {
    /// The compositor's mask, which outweighs keys heard before it.
    pub fn sent(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        (self.depressed, self.latched, self.locked, self.group) =
            (depressed, latched, locked, group);
        self.held.retain(|(_, bits)| depressed & bits == *bits);
        self.touched = 0;
    }

    /// A modifier key pressed or released, holding `bit`.
    pub fn key(&mut self, keycode: Keycode, bit: u32, pressed: bool) {
        self.held.retain(|(held, _)| *held != keycode);
        if pressed {
            self.held.push((keycode, bit));
        }
        self.touched |= bit;
    }

    /// Forgets everything, as a focus lost starts afresh.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// A new keymap: a fresh state, held keys reread.
    pub fn rekeyed(&mut self, keymap: &xkb::Keymap) {
        let held = std::mem::take(&mut self.held);
        self.clear();
        self.held = held
            .into_iter()
            .map(|(keycode, _)| (keycode, bits(keymap, keycode)))
            .filter(|(_, bits)| *bits != 0)
            .collect();
    }

    /// Focus gained with these modifier keys already down.
    pub fn entered(&mut self, held: Vec<(Keycode, u32)>) {
        self.clear();
        self.held = held.into_iter().filter(|(_, bit)| *bit != 0).collect();
    }

    /// Depressed modifiers: the mask, with bits keys touched since.
    pub fn depressed(&self) -> u32 {
        let held = self.held.iter().fold(0, |mask, (_, bit)| mask | bit);
        (self.depressed & !self.touched) | (held & self.touched)
    }

    /// Sets the state to the mask and keys since.
    pub fn apply(&self, state: &mut State) {
        state.update_mask(
            self.depressed(),
            self.latched,
            self.locked,
            0,
            0,
            self.group,
        );
    }
}

/// The modifiers a key holds, as the keymap sets them.
pub(super) fn bits(keymap: &xkb::Keymap, keycode: Keycode) -> u32 {
    let mut state = State::new(keymap);
    state.update_key(keycode, xkb::KeyDirection::Down);
    // A lock key toggles what it locks, holding nothing.
    state.serialize_mods(xkb::STATE_MODS_DEPRESSED) & !state.serialize_mods(xkb::STATE_MODS_LOCKED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::platform::modifiers_from_xkb;

    const SHIFT_L: u32 = 42 + 8;
    const SHIFT_R: u32 = 54 + 8;
    const CONTROL_L: u32 = 29 + 8;
    const A: u32 = 30 + 8;
    const C: u32 = 46 + 8;

    const LWIN: u32 = 125 + 8;
    const RWIN: u32 = 126 + 8;

    fn keymap(options: &str) -> xkb::Keymap {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let options = Some(options.to_string()).filter(|options| !options.is_empty());
        xkb::Keymap::new_from_names(&context, "", "", "us", "", options, 0)
            .expect("the us keymap compiles")
    }

    fn us() -> State {
        State::new(&keymap(""))
    }

    /// Plays a modifier key as the Wayland client hears it.
    fn key(held: &mut HeldMods, state: &mut State, keycode: u32, pressed: bool) {
        let keycode = Keycode::from(keycode);
        let bit = bits(&state.get_keymap(), keycode);
        assert_ne!(bit, 0, "keycode {keycode:?} holds a modifier");
        held.key(keycode, bit, pressed);
        held.apply(state);
    }

    fn entered(held: &mut HeldMods, state: &mut State, keycodes: &[u32]) {
        let keys = keycodes
            .iter()
            .map(|keycode| {
                let keycode = Keycode::from(*keycode);
                (keycode, bits(&state.get_keymap(), keycode))
            })
            .collect();
        held.entered(keys);
        held.apply(state);
    }

    fn sent(held: &mut HeldMods, state: &mut State, depressed: u32) {
        held.sent(depressed, 0, 16, 0);
        held.apply(state);
    }

    #[test]
    fn shift_held_by_its_key_alone_types_a_capital() {
        let (mut held, mut state) = (HeldMods::default(), us());
        sent(&mut held, &mut state, 0);
        key(&mut held, &mut state, SHIFT_L, true);
        assert_eq!(state.key_get_utf8(Keycode::from(A)), "A");
        assert!(modifiers_from_xkb(&state).shift);
        key(&mut held, &mut state, SHIFT_L, false);
        assert_eq!(state.key_get_utf8(Keycode::from(A)), "a");
        assert!(!modifiers_from_xkb(&state).shift);
    }

    #[test]
    fn shift_after_the_mask_said_control_joins_it() {
        // The reporter's log: modifiers(4), then Shift_L's key alone, then c.
        let (mut held, mut state) = (HeldMods::default(), us());
        key(&mut held, &mut state, CONTROL_L, true);
        sent(&mut held, &mut state, 4);
        key(&mut held, &mut state, SHIFT_L, true);
        let modifiers = modifiers_from_xkb(&state);
        assert!(modifiers.control && modifiers.shift, "{modifiers:?}");
        assert_eq!(state.key_get_utf8(Keycode::from(C)), "\u{3}");
    }

    #[test]
    fn control_by_its_key_alone_joins_the_mask_s_shift() {
        // modifiers(1), then Control_L's key alone, then v.
        let (mut held, mut state) = (HeldMods::default(), us());
        sent(&mut held, &mut state, 1);
        key(&mut held, &mut state, CONTROL_L, true);
        let modifiers = modifiers_from_xkb(&state);
        assert!(modifiers.control && modifiers.shift, "{modifiers:?}");
    }

    #[test]
    fn the_mask_outweighs_a_release_never_heard() {
        // Shift_L's release swallowed; the next mask lacks Shift.
        let (mut held, mut state) = (HeldMods::default(), us());
        key(&mut held, &mut state, SHIFT_L, true);
        sent(&mut held, &mut state, 4);
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
        let (mut held, mut state) = (HeldMods::default(), us());
        sent(&mut held, &mut state, 0);
        key(&mut held, &mut state, SHIFT_L, true);
        key(&mut held, &mut state, SHIFT_R, true);
        key(&mut held, &mut state, SHIFT_L, false);
        assert!(modifiers_from_xkb(&state).shift);
    }

    #[test]
    fn a_compositor_that_sends_the_mask_is_followed_as_before() {
        let (mut held, mut state) = (HeldMods::default(), us());
        key(&mut held, &mut state, SHIFT_L, true);
        sent(&mut held, &mut state, 1);
        assert!(modifiers_from_xkb(&state).shift);
        key(&mut held, &mut state, SHIFT_L, false);
        sent(&mut held, &mut state, 0);
        assert!(!modifiers_from_xkb(&state).shift);
        assert_eq!(bits(&state.get_keymap(), Keycode::from(A)), 0);
        assert_eq!(
            bits(&state.get_keymap(), Keycode::from(58u32 + 8)),
            0,
            "Caps Lock locks"
        );
    }

    #[test]
    fn a_mask_drops_the_keys_it_leaves_out() {
        // Left Shift's release swallowed, a mask without Shift, Right Shift.
        let (mut held, mut state) = (HeldMods::default(), us());
        key(&mut held, &mut state, SHIFT_L, true);
        sent(&mut held, &mut state, 0);
        key(&mut held, &mut state, SHIFT_R, true);
        key(&mut held, &mut state, SHIFT_R, false);
        assert_eq!(state.key_get_utf8(Keycode::from(A)), "a");
    }

    #[test]
    fn keys_held_on_entering_stay_held() {
        // Both Shifts held on entering; one goes up, mask unchanged.
        let (mut held, mut state) = (HeldMods::default(), us());
        entered(&mut held, &mut state, &[SHIFT_L, SHIFT_R]);
        sent(&mut held, &mut state, 1);
        key(&mut held, &mut state, SHIFT_L, false);
        assert_eq!(state.key_get_utf8(Keycode::from(A)), "A");
    }

    #[test]
    fn a_new_keymap_keeps_the_keys_held() {
        // Both Shifts held as the keymap changes; one goes up.
        let (mut held, mut state) = (HeldMods::default(), us());
        sent(&mut held, &mut state, 0);
        key(&mut held, &mut state, SHIFT_L, true);
        key(&mut held, &mut state, SHIFT_R, true);
        let next = keymap("");
        state = State::new(&next);
        held.rekeyed(&next);
        sent(&mut held, &mut state, 1);
        key(&mut held, &mut state, SHIFT_L, false);
        assert_eq!(state.key_get_utf8(Keycode::from(A)), "A");
    }

    #[test]
    fn a_key_holds_what_the_keymap_maps_it_to() {
        // altwin:meta_win puts Meta on the Windows keys, as Mod4.
        let (mut held, mut state) = (HeldMods::default(), State::new(&keymap("altwin:meta_win")));
        key(&mut held, &mut state, LWIN, true);
        let platform = modifiers_from_xkb(&state);
        let mask = held.depressed();
        sent(&mut held, &mut state, mask);
        key(&mut held, &mut state, RWIN, true);
        let modifiers = modifiers_from_xkb(&state);
        assert!(!modifiers.alt && modifiers.platform, "{modifiers:?}");
        assert_eq!(modifiers, platform);
    }
}
