//! Length units: parse user input and format values (replaces `cadcore.UnitUtils`).
//!
//! Internal unit is the centimetre.

pub const INCH: f64 = 2.54;
pub const FOOT: f64 = 30.48;
pub const POUNDS_PER_KG: f64 = 2.204_622_62;
pub const CM3_PER_LITRE: f64 = 1000.0;

/// Imperial values below this many inches are shown in inches only (widths, thicknesses), as shapers write them.
const FEET_FROM_INCHES: f64 = 36.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LengthUnit {
    /// Feet and inches with fractions, e.g. `6' 2 3/16"`.
    #[default]
    FeetInchesFraction,
    /// Feet and decimal inches, e.g. `6' 2.19"`.
    FeetInchesDecimal,
    Millimetres,
    Centimetres,
    Metres,
}

impl LengthUnit {
    pub const ALL: [LengthUnit; 5] = [
        LengthUnit::FeetInchesFraction,
        LengthUnit::FeetInchesDecimal,
        LengthUnit::Millimetres,
        LengthUnit::Centimetres,
        LengthUnit::Metres,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LengthUnit::FeetInchesFraction => "ft-in (fraction)",
            LengthUnit::FeetInchesDecimal => "ft-in (decimal)",
            LengthUnit::Millimetres => "mm",
            LengthUnit::Centimetres => "cm",
            LengthUnit::Metres => "m",
        }
    }

    pub fn is_imperial(self) -> bool {
        matches!(
            self,
            LengthUnit::FeetInchesFraction | LengthUnit::FeetInchesDecimal
        )
    }

    /// Format a length in cm. `fraction_denominator` is the finest inch fraction (e.g. 16).
    pub fn format(self, cm: f64, fraction_denominator: u32) -> String {
        if !cm.is_finite() {
            return "—".into();
        }
        no_negative_zero(self.format_raw(cm, fraction_denominator))
    }

    fn format_raw(self, cm: f64, fraction_denominator: u32) -> String {
        match self {
            LengthUnit::Millimetres => format!("{:.1} mm", cm * 10.0),
            LengthUnit::Centimetres => format!("{:.2} cm", cm),
            LengthUnit::Metres => format!("{:.4} m", cm / 100.0),
            LengthUnit::FeetInchesDecimal => {
                let (sign, cm) = sign_split(cm);
                let total = cm / INCH;
                let mut feet = if total >= FEET_FROM_INCHES {
                    (total / 12.0).floor()
                } else {
                    0.0
                };
                let mut inches = ((total - feet * 12.0) * 100.0).round() / 100.0;
                if inches >= 12.0 {
                    feet += 1.0;
                    inches -= 12.0;
                }
                if feet > 0.0 {
                    format!("{sign}{feet}' {inches:.2}\"")
                } else {
                    format!("{sign}{inches:.2}\"")
                }
            }
            LengthUnit::FeetInchesFraction => {
                let (sign, cm) = sign_split(cm);
                let den = fraction_denominator.max(1) as f64;
                let sixteenths = (cm / INCH * den).round();
                let feet = if sixteenths >= FEET_FROM_INCHES * den {
                    (sixteenths / (12.0 * den)).floor()
                } else {
                    0.0
                };
                let rest = sixteenths - feet * 12.0 * den;
                let whole = (rest / den).floor();
                let num = (rest - whole * den) as u32;
                let frac = if num == 0 {
                    String::new()
                } else {
                    let g = gcd(num, den as u32);
                    format!(" {}/{}", num / g, den as u32 / g)
                };
                match (feet > 0.0, whole > 0.0 || frac.is_empty()) {
                    (true, _) => format!("{sign}{feet}' {whole}{frac}\""),
                    (false, true) => format!("{sign}{whole}{frac}\""),
                    (false, false) => format!("{sign}{}\"", frac.trim()),
                }
            }
        }
    }

    /// Parse user input into cm. Explicit units (`'`, `"`, `in`, `ft`, `mm`, `cm`, `m`) override `self`.
    /// Accepts mixed fractions like `5 3/4` and feet-inch forms like `6'2"` or `6' 2 1/2"`.
    pub fn parse(self, input: &str) -> Option<f64> {
        // Typographic feet/inch marks (′ ″) as typed on some keyboards.
        let s = input
            .trim()
            .replace(['′', '’'], "'")
            .replace(['″', '”'], "\"")
            .to_ascii_lowercase();
        if s.is_empty() {
            return None;
        }
        let (neg, s) = match s.strip_prefix('-') {
            Some(rest) => (true, rest.trim().to_string()),
            None => (false, s),
        };
        let value = parse_unsigned(self, &s)?;
        Some(if neg { -value } else { value })
    }
}

fn sign_split(cm: f64) -> (&'static str, f64) {
    if cm < 0.0 { ("-", -cm) } else { ("", cm) }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn parse_unsigned(default: LengthUnit, s: &str) -> Option<f64> {
    // Feet marker: everything before it is feet, the remainder inches.
    if let Some(idx) = s.find('\'').or_else(|| s.find("ft")) {
        let marker_len = if s[idx..].starts_with('\'') { 1 } else { 2 };
        let feet = parse_number(s[..idx].trim())?;
        let rest = s[idx + marker_len..]
            .trim()
            .trim_end_matches('"')
            .trim_end_matches("in")
            .trim();
        let inches = if rest.is_empty() {
            0.0
        } else {
            parse_number(rest)?
        };
        return Some(feet * FOOT + inches * INCH);
    }
    let suffixes: [(&str, f64); 5] = [
        ("mm", 0.1),
        ("cm", 1.0),
        ("in", INCH),
        ("\"", INCH),
        ("m", 100.0),
    ];
    for (suffix, factor) in suffixes {
        if let Some(num) = s.strip_suffix(suffix) {
            return Some(parse_number(num.trim())? * factor);
        }
    }
    let n = parse_number(s)?;
    Some(match default {
        LengthUnit::FeetInchesFraction | LengthUnit::FeetInchesDecimal => n * INCH,
        LengthUnit::Millimetres => n / 10.0,
        LengthUnit::Centimetres => n,
        LengthUnit::Metres => n * 100.0,
    })
}

/// `"5"`, `"5.25"`, `"3/4"`, `"5 3/4"`, also with a comma as decimal separator.
fn parse_number(s: &str) -> Option<f64> {
    // Plain decimals only: rejects signs, exponents and "nan"/"inf", which `f64::from_str` would accept.
    fn plain(p: &str) -> Option<f64> {
        let ok = !p.is_empty()
            && p.chars().all(|c| c.is_ascii_digit() || c == '.')
            && p.chars().any(|c| c.is_ascii_digit())
            && p.matches('.').count() <= 1;
        if ok { p.parse().ok() } else { None }
    }
    let s = s.replace(',', ".");
    let mut total = 0.0;
    let mut any = false;
    for part in s.split_whitespace() {
        any = true;
        total += match part.split_once('/') {
            Some((n, d)) => {
                let d = plain(d)?;
                if d == 0.0 {
                    return None;
                }
                plain(n)? / d
            }
            None => plain(part)?,
        };
    }
    (any && total.is_finite()).then_some(total)
}

/// Drop the sign from values that format as zero ("-0\"", "-0.00 cm").
fn no_negative_zero(s: String) -> String {
    if s.starts_with('-') && !s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        s[1..].to_owned()
    } else {
        s
    }
}

/// Volume in litres from cm^3.
pub fn litres(cm3: f64) -> f64 {
    cm3 / CM3_PER_LITRE
}

#[cfg(test)]
mod tests {
    use super::*;
    use LengthUnit::*;

    #[test]
    fn parses_feet_inches() {
        assert!((FeetInchesFraction.parse("6'2\"").unwrap() - 187.96).abs() < 1e-9);
        assert!((Centimetres.parse("6' 2 1/2\"").unwrap() - (74.5 * INCH)).abs() < 1e-9);
        assert!((Centimetres.parse("6ft").unwrap() - 182.88).abs() < 1e-9);
    }

    #[test]
    fn parses_metric_and_defaults() {
        assert_eq!(FeetInchesFraction.parse("50mm"), Some(5.0));
        assert_eq!(FeetInchesFraction.parse("1.2 m"), Some(120.0));
        assert!((FeetInchesFraction.parse("18 1/2").unwrap() - 18.5 * INCH).abs() < 1e-9);
        assert_eq!(Millimetres.parse("25,5"), Some(2.55));
        assert_eq!(Centimetres.parse("-3"), Some(-3.0));
        assert_eq!(Centimetres.parse("abc"), None);
        assert_eq!(Centimetres.parse("1/0"), None);
    }

    #[test]
    fn formats_fractions() {
        assert_eq!(FeetInchesFraction.format(187.96, 16), "6' 2\"");
        assert_eq!(FeetInchesFraction.format(18.5 * INCH, 16), "18 1/2\"");
        assert_eq!(FeetInchesFraction.format(0.75 * INCH, 16), "3/4\"");
        assert_eq!(FeetInchesDecimal.format(187.96, 16), "6' 2.00\"");
        assert_eq!(Centimetres.format(187.96, 16), "187.96 cm");
    }

    #[test]
    fn format_parse_round_trip() {
        for cm in [5.0, 47.0, 187.96, 274.32] {
            let text = FeetInchesFraction.format(cm, 16);
            let back = Centimetres.parse(&text).unwrap();
            assert!((back - cm).abs() <= INCH / 32.0 + 1e-9, "{text}");
        }
    }
}
