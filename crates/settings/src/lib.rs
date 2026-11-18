//! settings: user settings values and global hotkeys.
//!
//! Owns: the `Settings` struct, when each setting takes effect (`ApplyAt`),
//! hotkey parsing and formatting, the default hotkey map and the hotkey check.
//!
//! Must never: store API keys outside the OS secret store. `Settings` has no key,
//! token or secret field; secrets go through the `SecretStore` trait only.

use meeting_core::protocol::Language;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettingsError {
    #[error("invalid settings json: {0}")]
    Json(String),
    #[error("subtitle font size {0} is not valid")]
    InvalidSubtitleFontSize(u8),
    #[error(transparent)]
    Hotkey(#[from] HotkeyError),
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HotkeyError {
    #[error("hotkey is empty")]
    Empty,
    #[error("hotkey has no key, only modifiers")]
    NoKey,
    #[error("hotkey needs at least one modifier")]
    NoModifier,
    #[error("unknown key name: {0}")]
    UnknownKey(String),
    #[error("modifier {0} is repeated")]
    DuplicateModifier(String),
    #[error("{0} is reserved by the system")]
    SystemReserved(String),
    #[error("no hotkey for {0:?}")]
    MissingAction(HotkeyAction),
    #[error("{combo} is already used by {action:?}")]
    Clash { combo: String, action: HotkeyAction },
}

/// When a changed setting takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyAt {
    /// The next provider call or session start.
    NextCall,
    /// At the next sentence boundary of a running session.
    NextSentence,
    /// Right away.
    Immediate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingKey {
    TargetLanguage,
    TranslationProvider,
    MicDevice,
    GlossaryEnabled,
    LocalOnly,
    OverlayVisible,
    SubtitleFontSize,
}

impl SettingKey {
    pub const ALL: [SettingKey; 7] = [
        SettingKey::TargetLanguage,
        SettingKey::TranslationProvider,
        SettingKey::MicDevice,
        SettingKey::GlossaryEnabled,
        SettingKey::LocalOnly,
        SettingKey::OverlayVisible,
        SettingKey::SubtitleFontSize,
    ];

    pub fn apply_at(self) -> ApplyAt {
        match self {
            SettingKey::TranslationProvider | SettingKey::MicDevice => ApplyAt::NextCall,
            SettingKey::TargetLanguage | SettingKey::GlossaryEnabled => ApplyAt::NextSentence,
            SettingKey::LocalOnly | SettingKey::OverlayVisible | SettingKey::SubtitleFontSize => {
                ApplyAt::Immediate
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Modifier {
    Ctrl,
    Option,
    Shift,
    Cmd,
}

impl Modifier {
    fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Option => "Option",
            Modifier::Shift => "Shift",
            Modifier::Cmd => "Cmd",
        }
    }

    fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Modifier::Ctrl),
            "option" | "opt" | "alt" => Some(Modifier::Option),
            "shift" => Some(Modifier::Shift),
            "cmd" | "command" | "meta" => Some(Modifier::Cmd),
            _ => None,
        }
    }
}

const NAMED_KEYS: [&str; 10] = [
    "Esc",
    "Tab",
    "Space",
    "Enter",
    "Backspace",
    "Delete",
    "Up",
    "Down",
    "Left",
    "Right",
];

/// Canonical key name, or `None` when the token is not a key.
fn parse_key(token: &str) -> Option<String> {
    let lower = token.to_ascii_lowercase();
    if token.chars().count() == 1 && token.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(token.to_ascii_uppercase());
    }
    let alias = match lower.as_str() {
        "escape" => "esc",
        "return" => "enter",
        other => other,
    };
    if let Some(n) = NAMED_KEYS.iter().find(|n| n.eq_ignore_ascii_case(alias)) {
        return Some((*n).to_string());
    }
    if let Some(num) = alias.strip_prefix('f').and_then(|d| d.parse::<u8>().ok()) {
        if (1..=20).contains(&num) && !alias[1..].starts_with('0') {
            return Some(format!("F{num}"));
        }
    }
    None
}

/// A global shortcut: one or more modifiers and one key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyCombo {
    modifiers: Vec<Modifier>,
    key: String,
}

impl KeyCombo {
    pub fn new(modifiers: &[Modifier], key: &str) -> Result<Self, HotkeyError> {
        let mut mods = modifiers.to_vec();
        mods.sort();
        for pair in mods.windows(2) {
            if pair[0] == pair[1] {
                return Err(HotkeyError::DuplicateModifier(pair[0].name().to_string()));
            }
        }
        if mods.is_empty() {
            return Err(HotkeyError::NoModifier);
        }
        let key = parse_key(key).ok_or_else(|| HotkeyError::UnknownKey(key.to_string()))?;
        Ok(Self {
            modifiers: mods,
            key,
        })
    }

    pub fn modifiers(&self) -> &[Modifier] {
        &self.modifiers
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

/// Canonical text, modifiers first in the order Ctrl, Option, Shift, Cmd.
impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in &self.modifiers {
            write!(f, "{}+", m.name())?;
        }
        write!(f, "{}", self.key)
    }
}

impl FromStr for KeyCombo {
    type Err = HotkeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(HotkeyError::Empty);
        }
        let tokens: Vec<&str> = s.split('+').map(str::trim).collect();
        let (last, head) = tokens.split_last().ok_or(HotkeyError::Empty)?;
        let mut mods = Vec::new();
        for t in head {
            let m = Modifier::parse(t).ok_or_else(|| HotkeyError::UnknownKey((*t).to_string()))?;
            mods.push(m);
        }
        if Modifier::parse(last).is_some() {
            return Err(HotkeyError::NoKey);
        }
        KeyCombo::new(&mods, last)
    }
}

impl TryFrom<String> for KeyCombo {
    type Error = HotkeyError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<KeyCombo> for String {
    fn from(k: KeyCombo) -> String {
        k.to_string()
    }
}

impl Serialize for KeyCombo {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for KeyCombo {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    StartMeeting,
    MuteMyVoice,
    QuickTranslate,
    FixGrammar,
    SaveNote,
}

/// Every action has exactly one combo; no combo is a system key or is used
/// twice. The field is private, so only `new` and `Default` build a map, and
/// deserializing goes through `new`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BTreeMap<HotkeyAction, KeyCombo>")]
pub struct HotkeyMap(BTreeMap<HotkeyAction, KeyCombo>);

impl HotkeyAction {
    pub const ALL: [HotkeyAction; 5] = [
        HotkeyAction::StartMeeting,
        HotkeyAction::MuteMyVoice,
        HotkeyAction::QuickTranslate,
        HotkeyAction::FixGrammar,
        HotkeyAction::SaveNote,
    ];
}

/// What `HotkeyMap::rebind` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rebind {
    /// The action now holds the new combo.
    Bound,
    /// The combo clashes and the owner has not confirmed. The map is unchanged.
    NeedsConfirm(Warning),
    /// The owner confirmed a clash: the other action took this action's old
    /// combo, so the map stays free of duplicates.
    Swapped { with: HotkeyAction },
}

impl Default for HotkeyMap {
    // The default table is a constant; an invalid entry is a programming error, so panic.
    #[allow(clippy::expect_used)]
    fn default() -> Self {
        use HotkeyAction::*;
        use Modifier::*;
        let entries = [
            (StartMeeting, &[Ctrl, Option][..], "S"),
            (MuteMyVoice, &[Ctrl, Option][..], "M"),
            (QuickTranslate, &[Ctrl, Option][..], "T"),
            (FixGrammar, &[Ctrl, Option][..], "G"),
            (SaveNote, &[Ctrl, Option][..], "N"),
        ];
        let map = entries
            .into_iter()
            .map(|(a, m, k)| (a, KeyCombo::new(m, k).expect("default hotkey is valid")))
            .collect();
        Self(map)
    }
}

impl TryFrom<BTreeMap<HotkeyAction, KeyCombo>> for HotkeyMap {
    type Error = HotkeyError;

    fn try_from(map: BTreeMap<HotkeyAction, KeyCombo>) -> Result<Self, Self::Error> {
        HotkeyMap::new(map)
    }
}

impl HotkeyMap {
    /// Builds a map after checking it: a hotkey for every action, none a
    /// system key, none used twice.
    pub fn new(map: BTreeMap<HotkeyAction, KeyCombo>) -> Result<Self, HotkeyError> {
        for combo in map.values() {
            if is_system_key(combo) {
                return Err(HotkeyError::SystemReserved(combo.to_string()));
            }
        }
        for (i, (_, combo)) in map.iter().enumerate() {
            if let Some((first, _)) = map.iter().take(i).find(|(_, c)| *c == combo) {
                return Err(HotkeyError::Clash {
                    combo: combo.to_string(),
                    action: *first,
                });
            }
        }
        if let Some(missing) = HotkeyAction::ALL.iter().find(|a| !map.contains_key(a)) {
            return Err(HotkeyError::MissingAction(*missing));
        }
        Ok(Self(map))
    }

    pub fn get(&self, action: HotkeyAction) -> Option<&KeyCombo> {
        self.0.get(&action)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&HotkeyAction, &KeyCombo)> {
        self.0.iter()
    }

    /// Binds `action` to `combo`. A system key is an error. A clash with
    /// another action is a warning: without `override_clash` the map is left
    /// as it is and `NeedsConfirm` comes back; with it the two actions swap
    /// combos.
    pub fn rebind(
        &mut self,
        action: HotkeyAction,
        combo: KeyCombo,
        override_clash: bool,
    ) -> Result<Rebind, HotkeyError> {
        let Some(warning) = check_hotkey_for(action, &combo, self)? else {
            self.0.insert(action, combo);
            return Ok(Rebind::Bound);
        };
        if !override_clash {
            return Ok(Rebind::NeedsConfirm(warning));
        }
        let Warning::Clash { action: other, .. } = warning;
        if let Some(old) = self.0.insert(action, combo) {
            self.0.insert(other, old);
        }
        Ok(Rebind::Swapped { with: other })
    }
}

/// Shortcuts the system owns: a key and the exact set of modifiers held with
/// it. The order of modifiers never matters. Shortcuts with the same key and
/// a different set (for example Cmd+Option+Q) are not system keys.
const SYSTEM_KEYS: [(&str, &[Modifier]); 15] = [
    ("Q", &[Modifier::Cmd]),
    ("Q", &[Modifier::Shift, Modifier::Cmd]),
    ("Q", &[Modifier::Ctrl, Modifier::Cmd]),
    ("Tab", &[Modifier::Cmd]),
    ("Tab", &[Modifier::Shift, Modifier::Cmd]),
    ("Space", &[Modifier::Cmd]),
    ("Space", &[Modifier::Ctrl]),
    ("Space", &[Modifier::Ctrl, Modifier::Cmd]),
    ("Space", &[Modifier::Option, Modifier::Cmd]),
    ("Esc", &[Modifier::Option, Modifier::Cmd]),
    ("W", &[Modifier::Cmd]),
    ("H", &[Modifier::Cmd]),
    ("3", &[Modifier::Shift, Modifier::Cmd]),
    ("4", &[Modifier::Shift, Modifier::Cmd]),
    ("5", &[Modifier::Shift, Modifier::Cmd]),
];

fn is_system_key(combo: &KeyCombo) -> bool {
    SYSTEM_KEYS.iter().any(|(key, mods)| {
        // `combo.modifiers` is sorted and unique, so a sorted copy compares equal.
        let mut want = mods.to_vec();
        want.sort();
        combo.key == *key && combo.modifiers == want
    })
}

/// A hotkey that works but needs the owner's confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// Another Interlo action already holds this combo.
    Clash { combo: String, action: HotkeyAction },
}

/// Check a new hotkey. A system key is an error. A clash with a key in the
/// map is a warning: the owner may confirm it (see `HotkeyMap::rebind`).
pub fn check_hotkey(new: &KeyCombo, existing: &HotkeyMap) -> Result<Option<Warning>, HotkeyError> {
    if is_system_key(new) {
        return Err(HotkeyError::SystemReserved(new.to_string()));
    }
    Ok(existing
        .iter()
        .find(|(_, k)| *k == new)
        .map(|(action, _)| Warning::Clash {
            combo: new.to_string(),
            action: *action,
        }))
}

/// Like `check_hotkey`, but ignores the key that `action` holds now, so an
/// action may keep its own combo.
pub fn check_hotkey_for(
    action: HotkeyAction,
    new: &KeyCombo,
    existing: &HotkeyMap,
) -> Result<Option<Warning>, HotkeyError> {
    let mut others = existing.clone();
    others.0.remove(&action);
    check_hotkey(new, &others)
}

/// Settings as read from JSON, before the checks in `Settings::try_from`.
#[derive(Deserialize)]
struct RawSettings {
    target_language: Language,
    translation_provider: String,
    mic_device: Option<String>,
    glossary_enabled: bool,
    local_only: bool,
    overlay_visible: bool,
    subtitle_font_size: u8,
    hotkeys: BTreeMap<HotkeyAction, KeyCombo>,
}

impl TryFrom<RawSettings> for Settings {
    type Error = SettingsError;

    fn try_from(raw: RawSettings) -> Result<Self, Self::Error> {
        if raw.subtitle_font_size == 0 {
            return Err(SettingsError::InvalidSubtitleFontSize(0));
        }
        Ok(Self {
            target_language: raw.target_language,
            translation_provider: raw.translation_provider,
            mic_device: raw.mic_device,
            glossary_enabled: raw.glossary_enabled,
            local_only: raw.local_only,
            overlay_visible: raw.overlay_visible,
            subtitle_font_size: raw.subtitle_font_size,
            hotkeys: HotkeyMap::new(raw.hotkeys)?,
        })
    }
}

/// Deserializing runs the same checks as `Settings::from_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawSettings")]
pub struct Settings {
    pub target_language: Language,
    pub translation_provider: String,
    pub mic_device: Option<String>,
    pub glossary_enabled: bool,
    pub local_only: bool,
    pub overlay_visible: bool,
    pub subtitle_font_size: u8,
    pub hotkeys: HotkeyMap,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target_language: Language::Vi,
            translation_provider: "local".into(),
            mic_device: None,
            glossary_enabled: true,
            local_only: true,
            overlay_visible: true,
            subtitle_font_size: 18,
            hotkeys: HotkeyMap::default(),
        }
    }
}

impl Settings {
    pub fn to_json(&self) -> Result<String, SettingsError> {
        serde_json::to_string(self).map_err(|e| SettingsError::Json(e.to_string()))
    }

    pub fn from_json(text: &str) -> Result<Self, SettingsError> {
        let raw: RawSettings =
            serde_json::from_str(text).map_err(|e| SettingsError::Json(e.to_string()))?;
        Self::try_from(raw)
    }
}

/// Where API keys live: the OS secret store, never `Settings`.
pub trait SecretStore {
    fn get(&self, name: &str) -> Result<Option<String>, SettingsError>;
    fn set(&self, name: &str, value: &str) -> Result<(), SettingsError>;
    fn remove(&self, name: &str) -> Result<(), SettingsError>;
}

/// Platform secret-store binding. Stub: the platform call comes with the desktop app.
pub struct PlatformSecretStub;

impl SecretStore for PlatformSecretStub {
    fn get(&self, _name: &str) -> Result<Option<String>, SettingsError> {
        Err(SettingsError::NotImplemented("secret store get"))
    }
    fn set(&self, _name: &str, _value: &str) -> Result<(), SettingsError> {
        Err(SettingsError::NotImplemented("secret store set"))
    }
    fn remove(&self, _name: &str) -> Result<(), SettingsError> {
        Err(SettingsError::NotImplemented("secret store remove"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(s: &str) -> KeyCombo {
        s.parse().unwrap()
    }

    #[test]
    fn apply_at_per_setting() {
        let want = [
            (SettingKey::TargetLanguage, ApplyAt::NextSentence),
            (SettingKey::TranslationProvider, ApplyAt::NextCall),
            (SettingKey::MicDevice, ApplyAt::NextCall),
            (SettingKey::GlossaryEnabled, ApplyAt::NextSentence),
            (SettingKey::LocalOnly, ApplyAt::Immediate),
            (SettingKey::OverlayVisible, ApplyAt::Immediate),
            (SettingKey::SubtitleFontSize, ApplyAt::Immediate),
        ];
        // The table covers every key, so a new key fails here until it is added.
        assert_eq!(want.len(), SettingKey::ALL.len());
        for key in SettingKey::ALL {
            assert!(want.iter().any(|(k, _)| *k == key), "{key:?} missing");
        }
        for (key, at) in want {
            assert_eq!(key.apply_at(), at, "{key:?}");
        }
    }

    #[test]
    fn parse_and_format_round_trip() {
        assert_eq!(combo("Ctrl+Option+S").to_string(), "Ctrl+Option+S");
        assert_eq!(combo("ctrl + opt + s").to_string(), "Ctrl+Option+S");
        assert_eq!(combo("Cmd+Ctrl+Q").to_string(), "Ctrl+Cmd+Q");
        assert_eq!(combo("alt+m").to_string(), "Option+M");
        assert_eq!(combo("Cmd+Escape").to_string(), "Cmd+Esc");
        assert_eq!(combo("Option+f5").to_string(), "Option+F5");
        let c = combo("Ctrl+Shift+Cmd+Space");
        assert_eq!(c.to_string().parse::<KeyCombo>().unwrap(), c);
    }

    #[test]
    fn bad_hotkey_text_is_rejected() {
        assert_eq!("".parse::<KeyCombo>(), Err(HotkeyError::Empty));
        assert_eq!("Ctrl+Option".parse::<KeyCombo>(), Err(HotkeyError::NoKey));
        assert_eq!("S".parse::<KeyCombo>(), Err(HotkeyError::NoModifier));
        assert_eq!(
            "Ctrl+Ctrl+S".parse::<KeyCombo>(),
            Err(HotkeyError::DuplicateModifier("Ctrl".into()))
        );
        assert_eq!(
            "Ctrl+Banana".parse::<KeyCombo>(),
            Err(HotkeyError::UnknownKey("Banana".into()))
        );
        assert!(matches!(
            "Ctrl+F21".parse::<KeyCombo>(),
            Err(HotkeyError::UnknownKey(_))
        ));
        assert!(matches!(
            "Ctrl+F05".parse::<KeyCombo>(),
            Err(HotkeyError::UnknownKey(_))
        ));
    }

    #[test]
    fn default_hotkeys_match_the_decision() {
        let map = HotkeyMap::default();
        let want = [
            (HotkeyAction::StartMeeting, "Ctrl+Option+S"),
            (HotkeyAction::MuteMyVoice, "Ctrl+Option+M"),
            (HotkeyAction::QuickTranslate, "Ctrl+Option+T"),
            (HotkeyAction::FixGrammar, "Ctrl+Option+G"),
            (HotkeyAction::SaveNote, "Ctrl+Option+N"),
        ];
        for (action, text) in want {
            assert_eq!(map.get(action).unwrap().to_string(), text);
        }
        assert_eq!(map.iter().count(), 5);
    }

    #[test]
    fn every_default_hotkey_uses_control_and_option_and_is_no_system_key() {
        let map = HotkeyMap::default();
        for (action, k) in map.iter() {
            assert_eq!(
                k.modifiers(),
                [Modifier::Ctrl, Modifier::Option],
                "{action:?} default is {k}"
            );
            assert!(!is_system_key(k), "{action:?} default {k} is a system key");
        }
    }

    #[test]
    fn default_hotkeys_pass_their_own_checks() {
        let map = HotkeyMap::default();
        for (action, k) in map.iter() {
            assert_eq!(check_hotkey_for(*action, k, &map), Ok(None));
        }
    }

    #[test]
    fn system_keys_are_refused() {
        let map = HotkeyMap::default();
        for text in [
            "Cmd+Q",
            "Shift+Cmd+Q",
            "Ctrl+Cmd+Q",
            "Cmd+Tab",
            "Shift+Cmd+Tab",
            "Cmd+Space",
            "Ctrl+Space",
            "Ctrl+Cmd+Space",
            "Cmd+Option+Space",
            "Cmd+Option+Esc",
            "Cmd+W",
            "Cmd+H",
            "Shift+Cmd+3",
            "Shift+Cmd+4",
            "Shift+Cmd+5",
        ] {
            assert!(
                matches!(
                    check_hotkey(&combo(text), &map),
                    Err(HotkeyError::SystemReserved(_))
                ),
                "{text} should be reserved"
            );
        }
    }

    #[test]
    fn system_keys_are_refused_in_any_modifier_order() {
        let map = HotkeyMap::default();
        for text in [
            "Cmd+Shift+Q",
            "Cmd+Ctrl+Q",
            "Cmd+Shift+Tab",
            "Cmd+Ctrl+Space",
            "Option+Cmd+Space",
            "Option+Cmd+Esc",
            "Cmd+Shift+3",
            "Cmd+Shift+4",
            "Cmd+Shift+5",
            "command+shift+q",
        ] {
            assert!(
                matches!(
                    check_hotkey(&combo(text), &map),
                    Err(HotkeyError::SystemReserved(_))
                ),
                "{text} should be reserved"
            );
        }
    }

    #[test]
    fn near_misses_of_system_keys_are_allowed() {
        let map = HotkeyMap::default();
        for text in [
            "Cmd+Option+Q",
            "Option+W",
            "Ctrl+Option+Space",
            "Shift+Space",
            "Option+Tab",
            "Shift+Cmd+6",
            "Ctrl+Cmd+W",
        ] {
            assert!(
                check_hotkey(&combo(text), &map) == Ok(None),
                "{text} is not a system key"
            );
        }
    }

    #[test]
    fn clash_with_an_existing_key_is_a_warning() {
        let map = HotkeyMap::default();
        assert_eq!(
            check_hotkey(&combo("Ctrl+Option+T"), &map),
            Ok(Some(Warning::Clash {
                combo: "Ctrl+Option+T".into(),
                action: HotkeyAction::QuickTranslate
            }))
        );
        assert_eq!(check_hotkey(&combo("Ctrl+Option+K"), &map), Ok(None));
    }

    #[test]
    fn a_system_key_is_an_error_not_a_warning() {
        let map = HotkeyMap::default();
        assert_eq!(
            check_hotkey(&combo("Cmd+Q"), &map),
            Err(HotkeyError::SystemReserved("Cmd+Q".into()))
        );
    }

    #[test]
    fn rebind_to_a_free_combo_binds() {
        let mut map = HotkeyMap::default();
        assert_eq!(
            map.rebind(HotkeyAction::SaveNote, combo("Ctrl+Option+K"), false),
            Ok(Rebind::Bound)
        );
        assert_eq!(
            map.get(HotkeyAction::SaveNote).unwrap().to_string(),
            "Ctrl+Option+K"
        );
    }

    #[test]
    fn rebind_may_keep_the_own_combo() {
        let mut map = HotkeyMap::default();
        assert_eq!(
            map.rebind(HotkeyAction::SaveNote, combo("Ctrl+Option+N"), false),
            Ok(Rebind::Bound)
        );
    }

    #[test]
    fn rebind_to_a_system_key_is_refused_even_when_confirmed() {
        for confirmed in [false, true] {
            let mut map = HotkeyMap::default();
            let before = map.clone();
            assert_eq!(
                map.rebind(HotkeyAction::SaveNote, combo("Cmd+Q"), confirmed),
                Err(HotkeyError::SystemReserved("Cmd+Q".into()))
            );
            assert_eq!(map, before);
        }
    }

    #[test]
    fn rebind_to_a_clash_needs_the_owner_to_confirm() {
        let mut map = HotkeyMap::default();
        let before = map.clone();
        assert_eq!(
            map.rebind(HotkeyAction::SaveNote, combo("Ctrl+Option+G"), false),
            Ok(Rebind::NeedsConfirm(Warning::Clash {
                combo: "Ctrl+Option+G".into(),
                action: HotkeyAction::FixGrammar
            }))
        );
        assert_eq!(map, before, "an unconfirmed clash must not change the map");
    }

    #[test]
    fn confirmed_clash_swaps_the_two_combos_and_keeps_the_map_valid() {
        let mut map = HotkeyMap::default();
        assert_eq!(
            map.rebind(HotkeyAction::SaveNote, combo("Ctrl+Option+G"), true),
            Ok(Rebind::Swapped {
                with: HotkeyAction::FixGrammar
            })
        );
        assert_eq!(
            map.get(HotkeyAction::SaveNote).unwrap().to_string(),
            "Ctrl+Option+G"
        );
        assert_eq!(
            map.get(HotkeyAction::FixGrammar).unwrap().to_string(),
            "Ctrl+Option+N"
        );
        // Still a valid map: a round trip through the validating constructor.
        let json = serde_json::to_string(&map).unwrap();
        assert_eq!(serde_json::from_str::<HotkeyMap>(&json).unwrap(), map);
    }

    #[test]
    fn default_map_passes_the_validating_constructor() {
        let map = HotkeyMap::default();
        let raw: BTreeMap<HotkeyAction, KeyCombo> =
            map.iter().map(|(a, k)| (*a, k.clone())).collect();
        assert_eq!(HotkeyMap::new(raw), Ok(map));
    }

    #[test]
    fn settings_round_trip_through_json() {
        let s = Settings::default();
        let back = Settings::from_json(&s.to_json().unwrap()).unwrap();
        assert_eq!(back, s);
        assert!(s.to_json().unwrap().contains("Ctrl+Option+S"));
        assert!(matches!(
            Settings::from_json("{"),
            Err(SettingsError::Json(_))
        ));
    }

    /// Settings JSON with the given `hotkeys` object and font size.
    fn settings_json(hotkeys: &str, font_size: u8) -> String {
        format!(
            r#"{{"target_language":"vi","translation_provider":"local","mic_device":null,
"glossary_enabled":true,"local_only":true,"overlay_visible":true,
"subtitle_font_size":{font_size},"hotkeys":{hotkeys}}}"#
        )
    }

    fn default_hotkeys_json() -> String {
        serde_json::to_string(&HotkeyMap::default()).unwrap()
    }

    #[test]
    fn probe_json_with_a_system_key_is_rejected() {
        // The bare probe is also refused, but only because fields are missing.
        let probe = r#"{"hotkeys":{"start_meeting":"Cmd+Q","mute_my_voice":"Cmd+Q"}}"#;
        assert!(Settings::from_json(probe).is_err());
        // With every other field present, the hotkeys are the reason.
        let hotkeys = r#"{"start_meeting":"Cmd+Q","mute_my_voice":"Cmd+Q"}"#;
        assert_eq!(
            Settings::from_json(&settings_json(hotkeys, 18)),
            Err(SettingsError::Hotkey(HotkeyError::SystemReserved(
                "Cmd+Q".into()
            )))
        );
    }

    #[test]
    fn from_json_rejects_a_system_key() {
        let hotkeys = default_hotkeys_json().replace("Ctrl+Option+S", "Cmd+Q");
        let err = Settings::from_json(&settings_json(&hotkeys, 18)).unwrap_err();
        assert_eq!(
            err,
            SettingsError::Hotkey(HotkeyError::SystemReserved("Cmd+Q".into()))
        );
    }

    #[test]
    fn from_json_rejects_a_missing_action() {
        let hotkeys = r#"{"start_meeting":"Ctrl+Option+S","mute_my_voice":"Ctrl+Option+M",
"quick_translate":"Ctrl+Option+T","fix_grammar":"Ctrl+Option+G"}"#;
        let err = Settings::from_json(&settings_json(hotkeys, 18)).unwrap_err();
        assert_eq!(
            err,
            SettingsError::Hotkey(HotkeyError::MissingAction(HotkeyAction::SaveNote))
        );
    }

    #[test]
    fn from_json_rejects_a_duplicate_combo() {
        let hotkeys = default_hotkeys_json().replace("Ctrl+Option+N", "Ctrl+Option+G");
        let err = Settings::from_json(&settings_json(&hotkeys, 18)).unwrap_err();
        assert!(
            matches!(err, SettingsError::Hotkey(HotkeyError::Clash { .. })),
            "{err:?}"
        );
    }

    #[test]
    fn from_json_rejects_a_zero_subtitle_font_size() {
        let err = Settings::from_json(&settings_json(&default_hotkeys_json(), 0)).unwrap_err();
        assert_eq!(err, SettingsError::InvalidSubtitleFontSize(0));
        assert!(Settings::from_json(&settings_json(&default_hotkeys_json(), 1)).is_ok());
    }

    #[test]
    fn hotkey_map_deserialize_is_validating() {
        let bad = default_hotkeys_json().replace("Ctrl+Option+S", "Cmd+Q");
        assert!(serde_json::from_str::<HotkeyMap>(&bad).is_err());
        let good = default_hotkeys_json();
        assert_eq!(
            serde_json::from_str::<HotkeyMap>(&good).unwrap(),
            HotkeyMap::default()
        );
    }

    #[test]
    fn settings_deserialize_is_validating() {
        let json = settings_json(&default_hotkeys_json(), 0);
        assert!(serde_json::from_str::<Settings>(&json).is_err());
    }

    #[test]
    fn settings_json_has_no_secret_fields() {
        let json: serde_json::Value =
            serde_json::from_str(&Settings::default().to_json().unwrap()).unwrap();
        let mut names = Vec::new();
        fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
            if let serde_json::Value::Object(m) = v {
                for (k, v) in m {
                    out.push(k.to_lowercase());
                    walk(v, out);
                }
            }
        }
        walk(&json, &mut names);
        assert!(!names.is_empty());
        for n in names {
            for bad in ["secret", "token", "password", "apikey", "api_key"] {
                assert!(!n.contains(bad), "settings field {n} looks like a secret");
            }
        }
    }

    #[test]
    fn secret_store_is_a_stub() {
        assert!(matches!(
            PlatformSecretStub.get("k"),
            Err(SettingsError::NotImplemented(_))
        ));
    }
}
