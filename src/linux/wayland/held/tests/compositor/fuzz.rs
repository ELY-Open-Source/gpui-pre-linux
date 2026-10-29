use super::*;

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

/// Random keys; the first `entering` and those `away` go unseen.
fn fuzz(entering: usize, steps: usize, away: std::ops::Range<usize>) {
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
                if step == away.start {
                    held.left(&mut state);
                }
                if step == entering || step == away.end {
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
                if step < entering || away.contains(&step) {
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
fn focus_lost_and_found_is_matched_at_every_key() {
    fuzz(0, 40, 15..22);
}

#[test]
fn keys_held_into_the_window_are_matched_at_every_key() {
    fuzz(6, 30, 0..0);
}

#[test]
fn a_compositor_sending_its_mask_is_matched_at_every_key() {
    fuzz(0, 40, 0..0);
}
