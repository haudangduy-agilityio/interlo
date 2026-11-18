//! Hotkey rules: default map and system keys.

// Test helpers outside #[test] functions may unwrap and expect.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;

use settings::{
    check_hotkey, check_hotkey_for, HotkeyAction, HotkeyError, HotkeyMap, KeyCombo, Modifier,
    Rebind, Settings, SettingsError, Warning,
};

const ALL_ACTIONS: [HotkeyAction; 5] = [
    HotkeyAction::StartMeeting,
    HotkeyAction::MuteMyVoice,
    HotkeyAction::QuickTranslate,
    HotkeyAction::FixGrammar,
    HotkeyAction::SaveNote,
];

#[test]
fn default_hotkeys_cover_every_action_without_a_clash() {
    let map = HotkeyMap::default();
    let mut seen = HashSet::new();
    for action in ALL_ACTIONS {
        let combo = map.get(action).expect("every action has a default hotkey");
        assert!(seen.insert(combo.to_string()), "{combo} is used twice");
        // Each default passes the same check a user rebind goes through.
        assert_eq!(
            check_hotkey_for(action, combo, &map),
            Ok(None),
            "{action:?}"
        );
    }
    assert_eq!(map.iter().count(), ALL_ACTIONS.len());
}

#[test]
fn mute_my_voice_defaults_to_control_option_m() {
    let map = HotkeyMap::default();
    let combo = map.get(HotkeyAction::MuteMyVoice).unwrap();
    assert_eq!(combo.to_string(), "Ctrl+Option+M");
}

#[test]
fn every_default_hotkey_uses_control_plus_option_and_no_system_key() {
    let map = HotkeyMap::default();
    for action in ALL_ACTIONS {
        let combo = map.get(action).unwrap();
        assert_eq!(
            combo.modifiers(),
            [Modifier::Ctrl, Modifier::Option],
            "{action:?} default is {combo}"
        );
        // A system key is an error, never a warning or Ok.
        let others = HotkeyMap::default();
        assert!(
            !matches!(
                check_hotkey_for(action, combo, &others),
                Err(HotkeyError::SystemReserved(_))
            ),
            "{action:?} default {combo} is a system key"
        );
    }
}

#[test]
fn default_settings_carry_the_default_hotkeys() {
    assert_eq!(Settings::default().hotkeys, HotkeyMap::default());
}

#[test]
fn a_macos_system_key_is_refused() {
    let mut map = HotkeyMap::default();
    for text in [
        "Cmd+Q",
        "Cmd+Tab",
        "Cmd+Space",
        "Shift+Cmd+4",
        // Modifier subsets and orders that the exact-string list missed.
        "Ctrl+Space",
        "Shift+Cmd+Tab",
        "Cmd+Shift+Tab",
        "Cmd+W",
        "Cmd+Option+Space",
    ] {
        let combo: KeyCombo = text.parse().expect("valid combo");
        assert!(
            matches!(
                check_hotkey(&combo, &map),
                Err(HotkeyError::SystemReserved(_))
            ),
            "{text}"
        );
        let before = map.clone();
        assert!(matches!(
            map.rebind(HotkeyAction::SaveNote, combo, true),
            Err(HotkeyError::SystemReserved(_))
        ));
        assert_eq!(map, before, "a refused rebind must not change the map");
    }
}

#[test]
fn a_hotkey_already_in_use_warns_and_binds_only_when_the_owner_confirms() {
    let mut map = HotkeyMap::default();
    let taken: KeyCombo = "Ctrl+Option+S".parse().expect("valid combo");
    let warning = Warning::Clash {
        combo: "Ctrl+Option+S".into(),
        action: HotkeyAction::StartMeeting,
    };
    assert_eq!(check_hotkey(&taken, &map), Ok(Some(warning.clone())));

    // Not confirmed: nothing changes.
    let before = map.clone();
    assert_eq!(
        map.rebind(HotkeyAction::SaveNote, taken.clone(), false),
        Ok(Rebind::NeedsConfirm(warning))
    );
    assert_eq!(map, before);

    // Confirmed: the two actions swap, so no combo is used twice.
    assert_eq!(
        map.rebind(HotkeyAction::SaveNote, taken.clone(), true),
        Ok(Rebind::Swapped {
            with: HotkeyAction::StartMeeting
        })
    );
    assert_eq!(map.get(HotkeyAction::SaveNote), Some(&taken));
    let distinct: HashSet<String> = map.iter().map(|(_, k)| k.to_string()).collect();
    assert_eq!(distinct.len(), ALL_ACTIONS.len());
}

#[test]
fn a_free_hotkey_is_accepted() {
    let mut map = HotkeyMap::default();
    let free: KeyCombo = "Ctrl+Option+K".parse().expect("valid combo");
    assert_eq!(
        map.rebind(HotkeyAction::SaveNote, free.clone(), false),
        Ok(Rebind::Bound)
    );
    assert_eq!(map.get(HotkeyAction::SaveNote), Some(&free));
}

#[test]
fn stored_settings_with_a_bad_hotkey_map_are_refused() {
    let defaults = Settings::default().to_json().expect("serialize");
    let full: serde_json::Value = serde_json::from_str(&defaults).expect("json");

    // The probe: two actions on a system key.
    let mut probe = full.clone();
    probe["hotkeys"] = serde_json::json!({
        "start_meeting": "Cmd+Q",
        "mute_my_voice": "Cmd+Q"
    });
    assert_eq!(
        Settings::from_json(&probe.to_string()),
        Err(SettingsError::Hotkey(HotkeyError::SystemReserved(
            "Cmd+Q".into()
        )))
    );

    // A missing action.
    let mut missing = full.clone();
    missing["hotkeys"]
        .as_object_mut()
        .expect("object")
        .remove("save_note");
    assert_eq!(
        Settings::from_json(&missing.to_string()),
        Err(SettingsError::Hotkey(HotkeyError::MissingAction(
            HotkeyAction::SaveNote
        )))
    );

    // A zero font size.
    let mut zero = full;
    zero["subtitle_font_size"] = serde_json::json!(0);
    assert_eq!(
        Settings::from_json(&zero.to_string()),
        Err(SettingsError::InvalidSubtitleFontSize(0))
    );
}
