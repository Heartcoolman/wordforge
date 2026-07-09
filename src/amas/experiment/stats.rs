//! T1.3 A/B 统计工具：正态/Student-t 分布、Wilson score CI、两比例 z 检验、Welch t 检验、
//! 样本量/MDE/power 反推。项目偏好自实现轻量数值（避免引入统计 crate），全部纯函数 + 单测对拍。
//!
//! 指标分流（见路线图 statisticalDesign）：
//!   - 比例指标（留存/完成率/mastered 持有率）→ Wilson CI + 两比例 z 检验；
//!   - 连续指标（reviewsPerDay）→ Welch t 检验 + 正态近似 CI。

use std::f64::consts::SQRT_2;

/// 标准正态 CDF Φ(z)，经 erf（Abramowitz-Stegun 7.1.26，|误差|<1.5e-7）。
pub fn normal_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / SQRT_2))
}

/// erf(x)，Abramowitz-Stegun 7.1.26 有理-指数逼近。
fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-x * x).exp();
    sign * y
}

/// 标准正态分位数 Φ⁻¹(p)，Acklam 有理逼近（相对误差 <1.15e-9）。p∈(0,1)，越界 clamp。
#[allow(clippy::excessive_precision)] // Acklam 系数按原文保留全精度
pub fn inv_normal_cdf(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    // Acklam 系数
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_690e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239e0,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838e0,
        -2.549_732_539_343_734e0,
        4.374_664_141_464_968e0,
        2.938_163_982_698_783e0,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996e0,
        3.754_408_661_907_416e0,
    ];
    const P_LOW: f64 = 0.024_25;
    const P_HIGH: f64 = 1.0 - P_LOW;
    if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= P_HIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// 双侧显著性的 z 临界值，如 alpha=0.05 → 1.96。
pub fn z_for_alpha_two_sided(alpha: f64) -> f64 {
    inv_normal_cdf(1.0 - alpha / 2.0)
}

/// power 对应的 z_β，如 power=0.80 → 0.8416。
pub fn z_for_power(power: f64) -> f64 {
    inv_normal_cdf(power)
}

/// 比例点估计 + Wilson score CI + 样本量。
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProportionCi {
    pub p: f64,
    pub low: f64,
    pub high: f64,
    pub n: u64,
}

/// Wilson score 区间。z = 双侧临界值（1.96≈95%）。n=0 时返回宽区间 [0,1]、p=0。
/// 小样本/极端比例下比 Wald 稳健，不会越出 [0,1]。
pub fn wilson_ci(successes: u64, n: u64, z: f64) -> ProportionCi {
    if n == 0 {
        return ProportionCi {
            p: 0.0,
            low: 0.0,
            high: 1.0,
            n: 0,
        };
    }
    let n_f = n as f64;
    let p_hat = successes as f64 / n_f;
    let z2 = z * z;
    let denom = 1.0 + z2 / n_f;
    let center = (p_hat + z2 / (2.0 * n_f)) / denom;
    let margin = (z / denom) * ((p_hat * (1.0 - p_hat) / n_f) + z2 / (4.0 * n_f * n_f)).sqrt();
    ProportionCi {
        p: p_hat,
        low: (center - margin).max(0.0),
        high: (center + margin).min(1.0),
        n,
    }
}

/// 两比例 z 检验（pooled），双侧 p 值。任一桶 n=0 返回 None。
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwoPropTest {
    /// p_canary − p_baseline
    pub diff: f64,
    pub z: f64,
    pub p_value: f64,
}

/// (x1,n1)=canary，(x2,n2)=baseline。
pub fn two_proportion_z(x1: u64, n1: u64, x2: u64, n2: u64) -> Option<TwoPropTest> {
    if n1 == 0 || n2 == 0 {
        return None;
    }
    let (n1f, n2f) = (n1 as f64, n2 as f64);
    let p1 = x1 as f64 / n1f;
    let p2 = x2 as f64 / n2f;
    let p_pool = (x1 + x2) as f64 / (n1f + n2f);
    let se = (p_pool * (1.0 - p_pool) * (1.0 / n1f + 1.0 / n2f)).sqrt();
    if se == 0.0 {
        // 两桶比例完全相同（或全 0 / 全 1）→ 无差异，p=1。
        return Some(TwoPropTest {
            diff: p1 - p2,
            z: 0.0,
            p_value: 1.0,
        });
    }
    let z = (p1 - p2) / se;
    // |z| 极大时 erf 逼近误差（A&S ≤1.5e-7）可令 normal_cdf 略 >1 → p 出现微负，夹回 0。
    let p_value = (2.0 * (1.0 - normal_cdf(z.abs()))).max(0.0);
    Some(TwoPropTest {
        diff: p1 - p2,
        z,
        p_value,
    })
}

/// 均值点估计 + 正态近似 CI（大样本）。
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeanCi {
    pub mean: f64,
    pub low: f64,
    pub high: f64,
    pub n: u64,
}

/// 大样本均值 CI：mean ± z·sqrt(var/n)。var 为样本方差。n<2 返回点估计宽区间。
pub fn mean_ci_normal(mean: f64, var: f64, n: u64, z: f64) -> MeanCi {
    if n < 2 {
        return MeanCi {
            mean,
            low: mean,
            high: mean,
            n,
        };
    }
    let se = (var / n as f64).sqrt();
    MeanCi {
        mean,
        low: mean - z * se,
        high: mean + z * se,
        n,
    }
}

/// Welch t 检验（不等方差），双侧 p 值。
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WelchTest {
    /// mean_canary − mean_baseline
    pub diff: f64,
    pub t: f64,
    pub df: f64,
    pub p_value: f64,
}

/// (mean1,var1,n1)=canary，(mean2,var2,n2)=baseline。var 为样本方差。
pub fn welch_t(
    mean1: f64,
    var1: f64,
    n1: u64,
    mean2: f64,
    var2: f64,
    n2: u64,
) -> Option<WelchTest> {
    if n1 < 2 || n2 < 2 {
        return None;
    }
    let (n1f, n2f) = (n1 as f64, n2 as f64);
    let s1 = var1 / n1f;
    let s2 = var2 / n2f;
    let se2 = s1 + s2;
    if se2 <= 0.0 {
        return Some(WelchTest {
            diff: mean1 - mean2,
            t: 0.0,
            df: (n1 + n2 - 2) as f64,
            p_value: 1.0,
        });
    }
    let t = (mean1 - mean2) / se2.sqrt();
    // Welch–Satterthwaite 自由度
    let df = se2 * se2 / (s1 * s1 / (n1f - 1.0) + s2 * s2 / (n2f - 1.0));
    let p_value = student_t_two_sided_p(t, df);
    Some(WelchTest {
        diff: mean1 - mean2,
        t,
        df,
        p_value,
    })
}

/// Student-t 双侧 p 值 P(|T|≥|t|)，经正则不完全 beta：betai(df/2, 1/2, df/(df+t²))。
pub fn student_t_two_sided_p(t: f64, df: f64) -> f64 {
    if df <= 0.0 {
        return f64::NAN;
    }
    let x = df / (df + t * t);
    betai(0.5 * df, 0.5, x)
}

/// 两比例每桶最小样本量。p0=基线比例，mde_rel=相对提升（0.05=+5%），alpha 双侧，power。
/// pooled 公式：n = (z_{1-α/2}·√(2·p̄·q̄) + z_β·√(p0·q0+p1·q1))² / (p1−p0)²。向上取整。
pub fn sample_size_two_proportions(p0: f64, mde_rel: f64, alpha: f64, power: f64) -> u64 {
    let p1 = (p0 * (1.0 + mde_rel)).clamp(0.0, 1.0);
    let delta = (p1 - p0).abs();
    if delta <= 0.0 || p0 <= 0.0 || p0 >= 1.0 {
        return u64::MAX;
    }
    let z_a = z_for_alpha_two_sided(alpha);
    let z_b = z_for_power(power);
    let p_bar = (p0 + p1) / 2.0;
    let term1 = z_a * (2.0 * p_bar * (1.0 - p_bar)).sqrt();
    let term2 = z_b * (p0 * (1.0 - p0) + p1 * (1.0 - p1)).sqrt();
    let n = ((term1 + term2) / delta).powi(2);
    n.ceil() as u64
}

/// 两均值每桶最小样本量。sigma=标准差，delta=绝对最小可检测增量。向上取整。
/// n = 2·(z_{1-α/2}+z_β)²·σ²/Δ²。
pub fn sample_size_two_means(sigma: f64, delta: f64, alpha: f64, power: f64) -> u64 {
    if delta <= 0.0 || sigma <= 0.0 {
        return u64::MAX;
    }
    let z_a = z_for_alpha_two_sided(alpha);
    let z_b = z_for_power(power);
    let n = 2.0 * (z_a + z_b).powi(2) * sigma * sigma / (delta * delta);
    n.ceil() as u64
}

// ---- 数值底座：ln Γ、正则不完全 beta ----

/// ln Γ(x)，Lanczos 逼近（g=7，相对误差 <1e-13，x>0）。
#[allow(clippy::excessive_precision)] // Lanczos 系数按原文保留全精度
fn ln_gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_93,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // 反射公式
        std::f64::consts::PI.ln()
            - (std::f64::consts::PI * x).sin().abs().ln()
            - ln_gamma(1.0 - x)
    } else {
        let x = x - 1.0;
        let mut a = C[0];
        let t = x + G + 0.5;
        for (i, &c) in C.iter().enumerate().skip(1) {
            a += c / (x + i as f64);
        }
        0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
    }
}

/// 正则不完全 beta I_x(a,b)，Numerical Recipes betacf 连分式。
fn betai(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b)
        + a * x.ln()
        + b * (1.0 - x).ln())
    .exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        bt * betacf(a, b, x) / a
    } else {
        1.0 - bt * betacf(b, a, 1.0 - x) / b
    }
}

fn betacf(a: f64, b: f64, x: f64) -> f64 {
    const MAXIT: usize = 200;
    const EPS: f64 = 3.0e-12;
    const FPMIN: f64 = 1.0e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAXIT {
        let m_f = m as f64;
        let m2 = 2.0 * m_f;
        let aa = m_f * (b - m_f) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m_f) * (qab + m_f) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn normal_cdf_known_points() {
        assert!(approx(normal_cdf(0.0), 0.5, 1e-6));
        assert!(approx(normal_cdf(1.96), 0.975, 1e-3));
        assert!(approx(normal_cdf(-1.96), 0.025, 1e-3));
        assert!(approx(normal_cdf(2.575_829), 0.995, 1e-3));
    }

    #[test]
    fn inv_normal_cdf_known_points() {
        assert!(approx(inv_normal_cdf(0.975), 1.959_964, 1e-4));
        assert!(approx(inv_normal_cdf(0.5), 0.0, 1e-6));
        assert!(approx(inv_normal_cdf(0.8413), 1.0, 1e-3));
        assert!(approx(z_for_alpha_two_sided(0.05), 1.959_964, 1e-4));
        assert!(approx(z_for_power(0.80), 0.841_621, 1e-4));
    }

    #[test]
    fn wilson_ci_known_example() {
        // 80/100，95% → 约 [0.711, 0.867]（教科书 Wilson 值）
        let ci = wilson_ci(80, 100, 1.959_964);
        assert!(approx(ci.p, 0.80, 1e-9));
        assert!(approx(ci.low, 0.7111, 2e-3), "low={}", ci.low);
        assert!(approx(ci.high, 0.8666, 2e-3), "high={}", ci.high);
        // n=0 边界
        let z = wilson_ci(0, 0, 1.96);
        assert_eq!(z.n, 0);
        assert!(z.low <= z.high);
    }

    #[test]
    fn two_proportion_z_directional_and_pvalue() {
        // canary 60/100 vs baseline 50/100 → 正向，z≈1.42，p≈0.155
        let t = two_proportion_z(60, 100, 50, 100).unwrap();
        assert!(t.diff > 0.0);
        assert!(approx(t.z, 1.4213, 5e-3), "z={}", t.z);
        assert!(approx(t.p_value, 0.1552, 5e-3), "p={}", t.p_value);
        // 相同比例 → z=0, p≈1（受 A&S erf 逼近精度 ~1.5e-7 限）
        let same = two_proportion_z(50, 100, 50, 100).unwrap();
        assert!(approx(same.z, 0.0, 1e-12));
        assert!(approx(same.p_value, 1.0, 1e-6));
        // 空桶 → None
        assert!(two_proportion_z(1, 0, 1, 10).is_none());
    }

    #[test]
    fn student_t_two_sided_known() {
        // t=2.0, df=10 → 双侧 p≈0.0734
        assert!(approx(student_t_two_sided_p(2.0, 10.0), 0.0734, 2e-3));
        // t=0 → p=1
        assert!(approx(student_t_two_sided_p(0.0, 10.0), 1.0, 1e-6));
        // 大 df → 趋近正态：t=1.96, df=1e6 → p≈0.05
        assert!(approx(student_t_two_sided_p(1.959_964, 1e6), 0.05, 1e-3));
    }

    #[test]
    fn welch_t_basic() {
        // 两组明显不同 → 小 p
        let r = welch_t(10.0, 4.0, 50, 8.0, 4.0, 50).unwrap();
        assert!(r.diff > 0.0);
        assert!(r.p_value < 0.01, "p={}", r.p_value);
        // 相同均值 → t=0, p=1
        let same = welch_t(5.0, 2.0, 30, 5.0, 3.0, 30).unwrap();
        assert!(approx(same.t, 0.0, 1e-9));
        assert!(approx(same.p_value, 1.0, 1e-6));
        assert!(welch_t(1.0, 1.0, 1, 1.0, 1.0, 10).is_none());
    }

    #[test]
    fn sample_size_two_proportions_textbook() {
        // p0=0.20, 相对 MDE +25%（p1=0.25）, alpha=0.05, power=0.80
        // 教科书每桶 ≈ 1093（pooled 公式量级）
        let n = sample_size_two_proportions(0.20, 0.25, 0.05, 0.80);
        assert!((900..=1300).contains(&n), "n={n}");
        // 退化输入 → MAX
        assert_eq!(sample_size_two_proportions(0.0, 0.1, 0.05, 0.8), u64::MAX);
    }

    #[test]
    fn sample_size_two_means_basic() {
        // sigma=10, delta=2, alpha=0.05, power=0.80 → 2*(1.96+0.84)²*100/4 ≈ 392
        let n = sample_size_two_means(10.0, 2.0, 0.05, 0.80);
        assert!((380..=400).contains(&n), "n={n}");
        assert_eq!(sample_size_two_means(0.0, 1.0, 0.05, 0.8), u64::MAX);
    }

    #[test]
    fn ln_gamma_known() {
        // Γ(5)=24 → lnΓ(5)=ln24
        assert!(approx(ln_gamma(5.0), 24.0_f64.ln(), 1e-9));
        // Γ(0.5)=√π
        assert!(approx(ln_gamma(0.5), std::f64::consts::PI.sqrt().ln(), 1e-9));
    }
}
