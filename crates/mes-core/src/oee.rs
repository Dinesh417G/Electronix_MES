//! OEE engine (§8.2, §12 M6) — pure Availability × Performance × Quality.
//!
//! The maths live here so the Rust path is deterministic and can be cross-checked
//! against the SQL path within 0.1% on a golden day (§12 M6 acceptance). The
//! same clamping/capping rules must be mirrored in the SQL query.
//!
//! - **Availability** = run time ÷ planned production time
//!   (planned production time = window − planned stops).
//! - **Performance** = (ideal cycle time × total pieces) ÷ run time, capped at
//!   1.0 (running faster than the ideal rate does not exceed 100%).
//! - **Quality** = good pieces ÷ total pieces.
//! - **OEE** = A × P × Q.

use serde::{Deserialize, Serialize};

/// Raw scalar inputs to an OEE calculation, all in base units (seconds/pieces).
#[derive(Debug, Clone, Copy)]
pub struct OeeInputs {
    pub planned_production_s: f64,
    pub run_s: f64,
    pub ideal_cycle_s: f64,
    pub total_count: f64,
    pub good_count: f64,
}

/// The three OEE factors and their product.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OeeResult {
    pub availability: f64,
    pub performance: f64,
    pub quality: f64,
    pub oee: f64,
}

fn ratio(num: f64, den: f64) -> f64 {
    if den > 0.0 {
        num / den
    } else {
        0.0
    }
}

/// Compute OEE from raw inputs. Zero denominators yield a zero factor rather
/// than NaN; performance is capped at 1.0 (see module docs).
pub fn compute(i: OeeInputs) -> OeeResult {
    let availability = ratio(i.run_s, i.planned_production_s);
    let performance = ratio(i.ideal_cycle_s * i.total_count, i.run_s).min(1.0);
    let quality = ratio(i.good_count, i.total_count);
    OeeResult {
        availability,
        performance,
        quality,
        oee: availability * performance * quality,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden fixture: A=0.8, P=0.75, Q=0.9, OEE=0.54 (hand-computed).
    #[test]
    fn golden_factors() {
        let r = compute(OeeInputs {
            planned_production_s: 27000.0, // 8h window − 30min planned stop
            run_s: 21600.0,                // running seconds
            ideal_cycle_s: 20.0,
            total_count: 810.0,
            good_count: 729.0,
        });
        assert!((r.availability - 0.80).abs() < 1e-9, "A={}", r.availability);
        assert!((r.performance - 0.75).abs() < 1e-9, "P={}", r.performance);
        assert!((r.quality - 0.90).abs() < 1e-9, "Q={}", r.quality);
        assert!((r.oee - 0.54).abs() < 1e-9, "OEE={}", r.oee);
    }

    #[test]
    fn performance_capped_at_one() {
        let r = compute(OeeInputs {
            planned_production_s: 100.0,
            run_s: 100.0,
            ideal_cycle_s: 2.0,
            total_count: 100.0, // ideal*total = 200 > run 100 ⇒ would be 2.0
            good_count: 100.0,
        });
        assert!((r.performance - 1.0).abs() < 1e-9);
    }

    #[test]
    fn zero_denominators_are_zero_not_nan() {
        let r = compute(OeeInputs {
            planned_production_s: 0.0,
            run_s: 0.0,
            ideal_cycle_s: 20.0,
            total_count: 0.0,
            good_count: 0.0,
        });
        assert_eq!(r.availability, 0.0);
        assert_eq!(r.performance, 0.0);
        assert_eq!(r.quality, 0.0);
        assert_eq!(r.oee, 0.0);
    }
}
