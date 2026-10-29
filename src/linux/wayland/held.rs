//! Keys a forwarder sends without `wl_keyboard.modifiers`.

use xkbcommon::xkb::{self, Keycode, State};

/// A key down, with the state its press found.
struct Down {
    keycode: Keycode,
    /// Depressed, latched, locked, group; `None` if down unseen.
    under: Option<[u32; 4]>,
    /// The modifiers its release would clear.
    own: u32,
}

/// Every key through xkb, beside the masks the window reads.
#[derive(Default)]
pub(super) struct HeldKeys {
    /// The keys down, oldest first.
    down: Vec<Down>,
    /// What unseen keys hold, by the first mask after them.
    pinned: Option<u32>,
    /// The state every key moved, as a compositor's moves.
    shadow: Option<State>,
    /// A key moved the shadow past the mask, unanswered.
    ahead: bool,
    /// Depressed, latched and locked bits settling added since the mask.
    inferred: [u32; 3],
}

impl HeldKeys {
    /// The compositor's mask: the window's state, and the shadow's.
    pub fn sent(&mut self, state: &mut State, mask: [u32; 4]) {
        let [depressed, latched, locked, group] = mask;
        state.update_mask(depressed, latched, locked, 0, 0, group);
        self.ahead = false;
        self.inferred = [0; 3];
        let shadow = shadow(&mut self.shadow, state);
        let seen = self.down.iter().fold(0, |seen, down| seen | down.own);
        // A seen key's modifier the mask lacks: a lost release.
        if shadow.serialize_mods(xkb::STATE_MODS_DEPRESSED) & !depressed & seen != 0 {
            *shadow = State::new(&shadow.get_keymap());
            // Seen keys again, each under the state its press found.
            self.down.retain(|down| match down.under {
                _ if down.own & !depressed != 0 => false,
                Some([depressed, latched, locked, group]) => {
                    shadow.update_mask(depressed, latched, locked, 0, 0, group);
                    shadow.update_key(down.keycode, xkb::KeyDirection::Down);
                    true
                }
                None => true,
            });
        }
        // The mask wins; each key keeps what its release clears.
        shadow.update_mask(depressed, latched, locked, 0, 0, group);
        // Later masks may only show unseen keys letting go.
        self.pinned = self
            .pinned
            .map(|pinned| pinned & depressed)
            .or(Some(depressed));
        if !self.unseen_down() {
            self.pinned = None;
        }
    }

    /// Moves the shadow through a key, as xkb does.
    pub fn key(&mut self, state: &State, keycode: Keycode, pressed: bool) {
        let shadow = shadow(&mut self.shadow, state);
        if let Some(at) = self.down.iter().position(|down| down.keycode == keycode) {
            let down = self.down.remove(at);
            if pressed && down.under.is_some() {
                // Its release never came.
                shadow.update_key(keycode, xkb::KeyDirection::Up);
            }
        }
        if pressed {
            let under = serialized(shadow);
            shadow.update_key(keycode, xkb::KeyDirection::Down);
            self.down.push(Down {
                keycode,
                under: Some(under),
                own: own(shadow, keycode, under),
            });
        } else {
            shadow.update_key(keycode, xkb::KeyDirection::Up);
        }
        if !self.down.iter().any(|down| down.under.is_none()) {
            self.pinned = None;
        }
        // Unseen keys still hold what the mask said they did.
        let pinned = self.pinned.unwrap_or(0);
        let depressed = shadow.serialize_mods(xkb::STATE_MODS_DEPRESSED);
        if pinned & !depressed != 0 {
            let [_, latched, locked, group] = serialized(shadow);
            shadow.update_mask(depressed | pinned, latched, locked, 0, 0, group);
        }
        self.ahead = serialized(shadow) != serialized(state);
    }

    /// Before the next key: no mask came, so the shadow's.
    pub fn settle(&mut self, state: &mut State) -> bool {
        let Some(shadow) = self.shadow.as_mut().filter(|_| self.ahead) else {
            return false;
        };
        self.ahead = false;
        let (theirs, mine) = (serialized(shadow), serialized(state));
        let mut next = mine;
        // What it adds; drops only what settling added.
        for at in 0..3 {
            let added = theirs[at] & !mine[at];
            let dropped = mine[at] & !theirs[at] & self.inferred[at];
            self.inferred[at] = (self.inferred[at] | added) & !dropped;
            next[at] = (mine[at] | added) & !dropped;
        }
        next[3] = theirs[3];
        // The shadow keeps what only a mask may drop.
        let [depressed, latched, locked, group] = next;
        shadow.update_mask(depressed, latched, locked, 0, 0, group);
        if next == mine {
            return false;
        }
        state.update_mask(depressed, latched, locked, 0, 0, group);
        true
    }

    /// Keys down before focus, which the mask alone speaks for.
    pub fn unseen(&mut self, down: Vec<Keycode>) {
        self.pinned = None;
        self.down = down
            .into_iter()
            .map(|keycode| Down {
                keycode,
                under: None,
                own: 0,
            })
            .collect();
    }

    /// A new keymap: the shadow starts again, its keys unseen.
    pub fn rekeyed(&mut self) {
        self.pinned = None;
        self.shadow = None;
        self.ahead = false;
        self.inferred = [0; 3];
        for down in &mut self.down {
            down.under = None;
            down.own = 0;
        }
    }

    /// Focus lost: no key down here, nor in the shadow.
    pub fn left(&mut self) {
        self.down.clear();
        self.pinned = None;
        self.ahead = false;
        self.inferred = [0; 3];
        if let Some(shadow) = &mut self.shadow {
            // Its group and locks stay, as the layout shown.
            let [_, _, locked, group] = serialized(shadow);
            *shadow = State::new(&shadow.get_keymap());
            shadow.update_mask(0, 0, locked, 0, 0, group);
        }
    }

    fn unseen_down(&self) -> bool {
        self.down.iter().any(|down| down.under.is_none())
    }
}

/// The shadow, started from the window's state when missing.
fn shadow<'a>(shadow: &'a mut Option<State>, state: &State) -> &'a mut State {
    shadow.get_or_insert_with(|| {
        let mut shadow = State::new(&state.get_keymap());
        let [depressed, latched, locked, group] = serialized(state);
        shadow.update_mask(depressed, latched, locked, 0, 0, group);
        shadow
    })
}

/// A state as a mask says it.
fn serialized(state: &State) -> [u32; 4] {
    [
        state.serialize_mods(xkb::STATE_MODS_DEPRESSED),
        state.serialize_mods(xkb::STATE_MODS_LATCHED),
        state.serialize_mods(xkb::STATE_MODS_LOCKED),
        state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE),
    ]
}

/// What a key alone holds: what its release would clear.
fn own(state: &State, keycode: Keycode, under: [u32; 4]) -> u32 {
    let [depressed, latched, locked, group] = under;
    let mut probe = State::new(&state.get_keymap());
    probe.update_mask(depressed, latched, locked, 0, 0, group);
    probe.update_key(keycode, xkb::KeyDirection::Down);
    let held = probe.serialize_mods(xkb::STATE_MODS_DEPRESSED);
    probe.update_key(keycode, xkb::KeyDirection::Up);
    held & !probe.serialize_mods(xkb::STATE_MODS_DEPRESSED)
}

#[cfg(test)]
mod tests;
