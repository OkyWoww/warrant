//! # Statistical Analysis & Exact Clopper-Pearson Confidence Intervals
//!
//! Provides exact binomial confidence intervals (Clopper-Pearson)
//! and multi-run variance accounting for Project WARRANT benchmarks.
//!
//! Strictly adheres to SPEC.md Section 8 and Section 10 (Task T10).

/// P(X <= k) for X ~ Binomial(n, p).
fn binom_cdf(k: usize, n: usize, p: f64) -> f64 {
    if p <= 0.0 {
        return 1.0;
    }
    if p >= 1.0 {
        return if k >= n { 1.0 } else { 0.0 };
    }
    let ratio = p / (1.0 - p);
    let mut term = (1.0 - p).powi(n as i32);
    let mut sum = term;
    for i in 0..k.min(n) {
        term *= (n - i) as f64 / (i + 1) as f64 * ratio;
        sum += term;
    }
    sum.min(1.0)
}

/// Root of an increasing function `f(p) = target` on (0, 1).
fn bisect_increasing<F: Fn(f64) -> f64>(f: F, target: f64) -> f64 {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f(mid) < target { lo = mid } else { hi = mid }
    }
    0.5 * (lo + hi)
}

/// Root of a decreasing function `f(p) = target` on (0, 1).
fn bisect_decreasing<F: Fn(f64) -> f64>(f: F, target: f64) -> f64 {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f(mid) > target { lo = mid } else { hi = mid }
    }
    0.5 * (lo + hi)
}

/// Exact Clopper-Pearson Binomial Confidence Interval
#[derive(Debug, Clone, PartialEq)]
pub struct ClopperPearsonInterval {
    pub k: usize,
    pub n: usize,
    pub lower_bound: f64,
    pub upper_bound_two_sided_95: f64,
    pub upper_bound_one_sided_95: f64,
}

impl ClopperPearsonInterval {
    /// Compute exact Clopper-Pearson interval for 95% confidence.
    ///
    /// For the primary safety metric (k = 0 false proofs out of n trials):
    /// - Lower bound = 0.0
    /// - Two-sided 95% upper bound: 1 - (alpha / 2)^(1/n) where alpha = 0.05
    /// - One-sided 95% upper bound: 1 - (alpha)^(1/n) where alpha = 0.05
    pub fn compute_95(k: usize, n: usize) -> Self {
        assert!(k <= n, "k cannot exceed n");
        if n == 0 {
            return Self {
                k,
                n,
                lower_bound: 0.0,
                upper_bound_two_sided_95: 0.0,
                upper_bound_one_sided_95: 0.0,
            };
        }

        let alpha = 0.05;

        let (lower, upper_two_sided, upper_one_sided) = if k == 0 {
            let upper_two = 1.0 - (alpha / 2.0_f64).powf(1.0 / n as f64);
            let upper_one = 1.0 - alpha.powf(1.0 / n as f64);
            (0.0, upper_two, upper_one)
        } else if k == n {
            let lower_two = (alpha / 2.0_f64).powf(1.0 / n as f64);
            let lower_one = alpha.powf(1.0 / n as f64);
            (lower_two, 1.0, lower_one)
        } else {
            // Exact Clopper-Pearson via bisection on the binomial CDF (no normal approximation).
            let lower = bisect_increasing(|p| 1.0 - binom_cdf(k - 1, n, p), alpha / 2.0);
            let upper_two = bisect_decreasing(|p| binom_cdf(k, n, p), alpha / 2.0);
            let upper_one = bisect_decreasing(|p| binom_cdf(k, n, p), alpha);
            (lower, upper_two, upper_one)
        };

        Self {
            k,
            n,
            lower_bound: lower,
            upper_bound_two_sided_95: upper_two_sided,
            upper_bound_one_sided_95: upper_one_sided,
        }
    }

    pub fn format_interval_pct(&self) -> String {
        if self.k == self.n && self.n > 0 {
            format!(
                "[{:.2}%, {:.2}%] (Two-sided 95% CI) / > {:.2}% (One-sided 95% lower)",
                self.lower_bound * 100.0,
                self.upper_bound_two_sided_95 * 100.0,
                self.upper_bound_one_sided_95 * 100.0,
            )
        } else {
            format!(
                "[{:.2}%, {:.2}%] (Two-sided 95% CI) / < {:.2}% (One-sided 95% upper)",
                self.lower_bound * 100.0,
                self.upper_bound_two_sided_95 * 100.0,
                self.upper_bound_one_sided_95 * 100.0,
            )
        }
    }
}

/// Three-run variance and standard deviation statistics
#[derive(Debug, Clone, PartialEq)]
pub struct RunVarianceStats {
    pub runs: [f64; 3],
    pub mean: f64,
    pub variance: f64,
    pub std_dev: f64,
}

impl RunVarianceStats {
    pub fn compute(r1: f64, r2: f64, r3: f64) -> Self {
        let runs = [r1, r2, r3];
        let mean = (r1 + r2 + r3) / 3.0;
        let sum_sq_diff: f64 = runs.iter().map(|&x| (x - mean).powi(2)).sum();
        let variance = sum_sq_diff / 2.0; // sample variance with N - 1 = 2
        let std_dev = variance.sqrt();

        Self {
            runs,
            mean,
            variance,
            std_dev,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clopper_pearson_k_zero_n_sixteen() {
        let cp = ClopperPearsonInterval::compute_95(0, 16);
        assert_eq!(cp.k, 0);
        assert_eq!(cp.n, 16);
        assert_eq!(cp.lower_bound, 0.0);
        // (0.025)^(1/16) ≈ 0.7937005 -> 1 - 0.7937005 = 0.206299
        assert!((cp.upper_bound_two_sided_95 - 0.2063).abs() < 1e-3);
        // (0.05)^(1/16) ≈ 0.82915 -> 1 - 0.82915 = 0.17085
        assert!((cp.upper_bound_one_sided_95 - 0.1709).abs() < 1e-3);
    }

    #[test]
    fn test_clopper_pearson_exact_interior_values() {
        // Reference values of the exact (Beta-quantile) Clopper-Pearson 95% interval.
        let a = ClopperPearsonInterval::compute_95(3, 8);
        assert!((a.lower_bound - 0.0852).abs() < 1e-3, "lower {}", a.lower_bound);
        assert!((a.upper_bound_two_sided_95 - 0.7551).abs() < 1e-3, "upper {}", a.upper_bound_two_sided_95);
        let b = ClopperPearsonInterval::compute_95(1, 10);
        assert!((b.lower_bound - 0.0025).abs() < 1e-3, "lower {}", b.lower_bound);
        assert!((b.upper_bound_two_sided_95 - 0.4450).abs() < 1e-3, "upper {}", b.upper_bound_two_sided_95);
    }

    #[test]
    fn test_three_run_variance_deterministic() {
        let stats = RunVarianceStats::compute(100.0, 100.0, 100.0);
        assert_eq!(stats.mean, 100.0);
        assert_eq!(stats.variance, 0.0);
        assert_eq!(stats.std_dev, 0.0);
    }
}

