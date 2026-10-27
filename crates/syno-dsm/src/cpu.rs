//! Microarchitecture names of the processors Synology builds into its NAS. DSM
//! reports vendor, family and series ("INTEL", "Celeron", "J4125") but not the
//! generation, which is what tells a user what the box can do.

/// Series as DSM reports them, without spaces and in capitals, to the architecture.
/// A series matches when it equals an entry or starts with it.
const ARCHITECTURES: &[(&str, &str)] = &[
    // Intel Atom, Celeron and Pentium
    ("D410", "Pineview"),
    ("D425", "Pineview"),
    ("D510", "Pineview"),
    ("D525", "Pineview"),
    ("D2550", "Cedarview"),
    ("D2700", "Cedarview"),
    ("J1800", "Bay Trail"),
    ("J1850", "Bay Trail"),
    ("J1900", "Bay Trail"),
    ("C2338", "Rangeley"),
    ("C2538", "Rangeley"),
    ("C2558", "Rangeley"),
    ("C2758", "Rangeley"),
    ("C3338", "Denverton"),
    ("C3538", "Denverton"),
    ("C3558", "Denverton"),
    ("C3758", "Denverton"),
    ("C3958", "Denverton"),
    ("N3000", "Braswell"),
    ("N3010", "Braswell"),
    ("N3050", "Braswell"),
    ("N3060", "Braswell"),
    ("N3150", "Braswell"),
    ("N3160", "Braswell"),
    ("N3700", "Braswell"),
    ("J3355", "Apollo Lake"),
    ("J3455", "Apollo Lake"),
    ("J4005", "Gemini Lake"),
    ("J4105", "Gemini Lake"),
    ("J4025", "Gemini Lake Refresh"),
    ("J4125", "Gemini Lake Refresh"),
    ("N5105", "Jasper Lake"),
    ("N5095", "Jasper Lake"),
    // Intel Core, Pentium and Xeon
    ("I3-4130", "Haswell"),
    ("I3-6100", "Skylake"),
    ("I3-6300", "Skylake"),
    ("G4400", "Skylake"),
    ("G4560", "Kaby Lake"),
    ("E3-1230V2", "Ivy Bridge"),
    ("E3-1230V5", "Skylake"),
    ("E3-1230V6", "Kaby Lake"),
    ("E3-1275V6", "Kaby Lake"),
    ("E-2124", "Coffee Lake"),
    ("E-2234", "Coffee Lake"),
    ("W-1250", "Comet Lake"),
    ("D-1508", "Broadwell-DE"),
    ("D-1520", "Broadwell-DE"),
    ("D-1521", "Broadwell-DE"),
    ("D-1527", "Broadwell-DE"),
    ("D-1531", "Broadwell-DE"),
    ("D-1537", "Broadwell-DE"),
    ("D-1541", "Broadwell-DE"),
    ("D-1567", "Broadwell-DE"),
    ("D-2123IT", "Skylake-D"),
    ("D-2143IT", "Skylake-D"),
    ("4314", "Ice Lake"),
    // AMD
    ("R1505G", "Zen"),
    ("R1600", "Zen"),
    ("R1606G", "Zen"),
    ("V1500B", "Zen"),
    ("V1780B", "Zen"),
    ("7272", "Zen 2"),
    // ARM
    ("RTD1293", "Cortex-A53"),
    ("RTD1296", "Cortex-A53"),
    ("RTD1619B", "Cortex-A55"),
    ("AL-212", "Cortex-A15"),
    ("AL-214", "Cortex-A15"),
    ("AL-314", "Cortex-A57"),
    ("AL-324", "Cortex-A57"),
    ("ARMADA3720", "Cortex-A53"),
    ("ARMADA385", "Cortex-A9"),
    ("ARMADA388", "Cortex-A9"),
];

/// The microarchitecture of a processor series, or `None` for one not in the table.
pub(crate) fn architecture(series: &str) -> Option<&'static str> {
    let key: String = series
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    if key.is_empty() {
        return None;
    }
    ARCHITECTURES
        .iter()
        .find(|(pattern, _)| key == *pattern || key.starts_with(pattern))
        .map(|(_, name)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_common_synology_processors() {
        assert_eq!(architecture("J4125"), Some("Gemini Lake Refresh"));
        assert_eq!(architecture("N3160"), Some("Braswell"));
        assert_eq!(architecture("J3455"), Some("Apollo Lake"));
        assert_eq!(architecture("R1600"), Some("Zen"));
        assert_eq!(architecture("E3-1230 v6"), Some("Kaby Lake"));
        assert_eq!(architecture("D-1541"), Some("Broadwell-DE"));
        assert_eq!(architecture("rtd1296"), Some("Cortex-A53"));
    }

    #[test]
    fn unknown_series_have_no_architecture() {
        assert_eq!(architecture("X9999"), None);
        assert_eq!(architecture(""), None);
    }
}
