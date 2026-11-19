//! The run's final verdict: one line naming every failing check, and the exit code derived from it.
//!
//! The binary runs three independent checks — the block-entry install ring ([`crate::install_ring`]),
//! ASET hash ownership ([`crate::aset_validate`]) and the consumption simulation ([`crate::simulate`]).
//! Each outcome, including a check that could not run, is recorded here; [`Verdict::line`] and
//! [`Verdict::exit_code`] both read the same failure list, so the last line printed never disagrees with
//! the exit code.

use crate::aset_validate::HashValidationStats;
use crate::install_ring::RingReport;
use crate::simulate::{fatal_findings, SimulateReport};
use std::fmt::Display;

/// `"<n> <one>"` or `"<n> <many>"`.
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The checks that ran and the reasons the run failed.
#[derive(Debug, Default)]
pub struct Verdict {
    checks: Vec<&'static str>,
    failures: Vec<String>,
}

impl Verdict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_install_ring<E: Display>(&mut self, outcome: &Result<RingReport, E>) {
        self.checks.push("install ring");
        match outcome {
            Ok(report) => self.failures.extend(report.failure()),
            Err(e) => self.failures.push(format!("install-ring check could not run: {e}")),
        }
    }

    pub fn record_aset<E: Display>(&mut self, outcome: &Result<HashValidationStats, E>) {
        self.checks.push("ASET");
        match outcome {
            Ok(stats) => self.failures.extend(stats.failure()),
            Err(e) => self.failures.push(format!("ASET check could not run: {e}")),
        }
    }

    pub fn record_simulation<E: Display>(&mut self, outcome: &Result<SimulateReport, E>) {
        self.checks.push("consumption");
        match outcome {
            Ok(report) => {
                let findings = fatal_findings(report);
                if !findings.is_empty() {
                    self.failures.push(format!("simulation: {}", findings.join(", ")));
                }
            }
            Err(e) => self.failures.push(format!("simulation could not run: {e}")),
        }
    }

    /// A failure outside the three checks (an input that could not be read, a report that could not be
    /// written).
    pub fn fail(&mut self, reason: String) {
        self.failures.push(reason);
    }

    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }

    /// 0 when nothing failed, 1 otherwise.
    pub fn exit_code(&self) -> i32 {
        if self.passed() {
            0
        } else {
            1
        }
    }

    /// `VERDICT: PASS — no fatal findings (<checks run>)` or `VERDICT: FAILED — <reason>; <reason>; …`.
    pub fn line(&self) -> String {
        if self.passed() {
            format!("VERDICT: PASS — no fatal findings ({})", self.checks.join(", "))
        } else {
            format!("VERDICT: FAILED — {}", self.failures.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_ring::Overflow;
    use std::path::PathBuf;

    const RESIDENT: &str = "blocks\\VZ\\resident_P000_Q3.block";

    fn ring(overflows: &[(&str, u32)]) -> RingReport {
        RingReport {
            ring: 7018,
            ring_from: PathBuf::from("vz.wad"),
            blocks: 10,
            overflows: overflows
                .iter()
                .map(|&(block, entries)| Overflow { wad: PathBuf::from("vz-patch.wad"), block: block.into(), entries })
                .collect(),
        }
    }

    fn all_pass() -> Verdict {
        let mut v = Verdict::new();
        v.record_install_ring::<String>(&Ok(ring(&[])));
        v.record_aset::<String>(&Ok(HashValidationStats::default()));
        v.record_simulation::<String>(&Ok(SimulateReport::default()));
        v
    }

    #[test]
    fn every_check_clean_passes() {
        let v = all_pass();
        assert_eq!(v.exit_code(), 0);
        assert_eq!(v.line(), "VERDICT: PASS — no fatal findings (install ring, ASET, consumption)");
    }

    #[test]
    fn a_skipped_check_is_not_named_in_the_pass() {
        let mut v = Verdict::new();
        v.record_install_ring::<String>(&Ok(ring(&[])));
        v.record_simulation::<String>(&Ok(SimulateReport::default()));
        assert_eq!(v.line(), "VERDICT: PASS — no fatal findings (install ring, consumption)");
    }

    #[test]
    fn install_ring_overflow_alone_fails() {
        let mut v = Verdict::new();
        v.record_install_ring::<String>(&Ok(ring(&[(RESIDENT, 7022)])));
        v.record_aset::<String>(&Ok(HashValidationStats::default()));
        v.record_simulation::<String>(&Ok(SimulateReport::default()));
        assert_eq!(v.exit_code(), 1);
        assert_eq!(
            v.line(),
            "VERDICT: FAILED — 1 block never installed (blocks\\VZ\\resident_P000_Q3.block: 7022 entries, ring 7018)"
        );
    }

    #[test]
    fn install_ring_overflows_are_one_reason() {
        let mut v = Verdict::new();
        v.record_install_ring::<String>(&Ok(ring(&[(RESIDENT, 7022), ("blocks\\VZ\\x.block", 8000)])));
        assert_eq!(
            v.line(),
            "VERDICT: FAILED — 2 blocks never installed (blocks\\VZ\\resident_P000_Q3.block: 7022 entries, blocks\\VZ\\x.block: 8000 entries, ring 7018)"
        );
    }

    #[test]
    fn install_ring_error_fails() {
        let mut v = Verdict::new();
        v.record_install_ring::<String>(&Err("vz-patch.wad: no CSUM chunk row".into()));
        assert_eq!(v.exit_code(), 1);
        assert_eq!(v.line(), "VERDICT: FAILED — install-ring check could not run: vz-patch.wad: no CSUM chunk row");
    }

    #[test]
    fn aset_alone_fails() {
        let mut v = Verdict::new();
        v.record_install_ring::<String>(&Ok(ring(&[])));
        v.record_aset::<String>(&Ok(HashValidationStats { misrouted: 2, true_ghost: 1, ..Default::default() }));
        assert_eq!(v.exit_code(), 1);
        assert_eq!(v.line(), "VERDICT: FAILED — ASET: 2 misrouted, 1 ghost");

        let mut v = Verdict::new();
        v.record_aset::<String>(&Ok(HashValidationStats { true_ghost: 3, ..Default::default() }));
        assert_eq!(v.line(), "VERDICT: FAILED — ASET: 3 ghosts");
    }

    #[test]
    fn aset_error_fails() {
        let mut v = Verdict::new();
        v.record_aset::<String>(&Err("No such file or directory (os error 2)".into()));
        assert_eq!(v.line(), "VERDICT: FAILED — ASET check could not run: No such file or directory (os error 2)");
    }

    #[test]
    fn simulation_alone_fails() {
        let mut v = Verdict::new();
        v.record_install_ring::<String>(&Ok(ring(&[])));
        v.record_simulation::<String>(&Ok(SimulateReport {
            decode_errors: vec!["a".into()],
            texture_buffer_too_small: 2,
            ..Default::default()
        }));
        assert_eq!(v.exit_code(), 1);
        assert_eq!(v.line(), "VERDICT: FAILED — simulation: 1 decode error, 2 texture buffers too small");
    }

    #[test]
    fn simulation_error_fails() {
        let mut v = Verdict::new();
        v.record_simulation::<String>(&Err("bad FFCS magic".into()));
        assert_eq!(v.line(), "VERDICT: FAILED — simulation could not run: bad FFCS magic");
    }

    #[test]
    fn several_failures_are_all_named_in_check_order() {
        let mut v = Verdict::new();
        v.record_install_ring::<String>(&Ok(ring(&[(RESIDENT, 7022)])));
        v.record_aset::<String>(&Ok(HashValidationStats { misrouted: 1, ..Default::default() }));
        v.record_simulation::<String>(&Ok(SimulateReport { access_violations: vec!["x".into(), "y".into()], ..Default::default() }));
        v.fail("JSON report not written: out.json: permission denied".into());
        assert_eq!(v.exit_code(), 1);
        assert_eq!(
            v.line(),
            "VERDICT: FAILED — 1 block never installed (blocks\\VZ\\resident_P000_Q3.block: 7022 entries, ring 7018); \
             ASET: 1 misrouted; simulation: 2 access violations; JSON report not written: out.json: permission denied"
        );
    }

    #[test]
    fn a_failure_outside_the_checks_fails() {
        let mut v = all_pass();
        v.fail("rainbow table rt.json: missing field".into());
        assert_eq!(v.exit_code(), 1);
        assert_eq!(v.line(), "VERDICT: FAILED — rainbow table rt.json: missing field");
    }

    #[test]
    fn exit_code_matches_simulate_exit_code() {
        let report = SimulateReport { ucfx_issues: vec!["codec 0x05 (Xbox ADPCM) on PC build".into()], ..Default::default() };
        let sim_code = crate::simulate::simulate_exit_code(&report);
        let mut v = Verdict::new();
        v.record_simulation::<String>(&Ok(report));
        assert_eq!((v.exit_code(), sim_code), (1, 1));
        assert_eq!(v.line(), "VERDICT: FAILED — simulation: 1 fatal audio issue");
    }
}
