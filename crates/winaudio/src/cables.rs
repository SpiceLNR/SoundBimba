// SPDX-License-Identifier: AGPL-3.0-or-later
//! Известные виртуальные кабели. Пользователь ставит их сам; мы только находим их по названию
//! (решение от 2026-11-02, раздел 7 архитектуры).

/// Известный виртуальный кабель: ищем по части названия устройства или адаптера.
pub struct CableKind {
    pub label: &'static str,
    patterns: &'static [&'static str],
    pub note: Option<&'static str>,
}

/// Список расширяемый. Сравнение без учёта регистра.
pub const CABLES: &[CableKind] = &[
    CableKind { label: "VB-CABLE", patterns: &["vb-audio virtual cable"], note: None },
    CableKind { label: "VB-CABLE A", patterns: &["vb-audio cable a"], note: None },
    CableKind { label: "VB-CABLE B", patterns: &["vb-audio cable b"], note: None },
    CableKind { label: "VB-CABLE C", patterns: &["vb-audio cable c"], note: None },
    CableKind { label: "VB-CABLE D", patterns: &["vb-audio cable d"], note: None },
    CableKind {
        label: "Virtual Audio Driver (MTT)",
        patterns: &["virtual audio driver", "virtual mic driver"],
        note: Some("от него отказались: микрофон выдаёт тестовый тон, а не звук динамика"),
    },
];

/// Какой кабель (если есть) представляет устройство с таким названием и адаптером.
pub fn cable_of(name: &str, adapter: &str) -> Option<&'static CableKind> {
    let text = format!("{name} {adapter}").to_lowercase();
    CABLES.iter().find(|k| k.patterns.iter().any(|p| text.contains(p)))
}

#[cfg(test)]
mod tests {
    use super::cable_of;

    #[test]
    fn recognizes_cables() {
        let label = |n, a| cable_of(n, a).map(|k| k.label);
        assert_eq!(label("CABLE Input (VB-Audio Virtual Cable)", "VB-Audio Virtual Cable"), Some("VB-CABLE"));
        assert_eq!(label("CABLE Output (VB-Audio Virtual Cable)", "VB-Audio Virtual Cable"), Some("VB-CABLE"));
        assert_eq!(label("CABLE-A Output (VB-Audio Cable A)", "VB-Audio Cable A"), Some("VB-CABLE A"));
        assert_eq!(label("Speakers (Virtual Desktop Audio)", "Virtual Desktop Audio"), None);
        assert_eq!(label("Speakers (Realtek(R) Audio)", "Realtek(R) Audio"), None);
    }
}
