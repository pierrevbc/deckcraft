//! Number formats: Excel format codes (`#,##0`, `0.0%`, `_-* # ##0 "₽"_-;…`, `General`) as chart
//! axes and data labels show them, in the chart's language (digit grouping and decimal mark).

/// Digit grouping and decimal mark for a language tag (`ru-RU`, `de-DE`, `en-US`).
fn marks(lang: Option<&str>) -> (char, char) {
    let l = lang.unwrap_or("en").split(['-', '_']).next().unwrap_or("en").to_ascii_lowercase();
    match l.as_str() {
        // Space-grouped, comma decimal.
        "ru" | "uk" | "be" | "kk" | "fr" | "pl" | "cs" | "sk" | "sv" | "fi" | "nb" | "nn" | "no" | "bg" | "lt" | "lv" | "et" | "hu" | "uz" | "hy"
        | "ka" | "az" => ('\u{a0}', ','),
        // Dot-grouped, comma decimal.
        "de" | "es" | "it" | "pt" | "nl" | "da" | "tr" | "el" | "ro" | "id" | "sr" | "hr" | "sl" | "vi" => ('.', ','),
        _ => (',', '.'),
    }
}

#[derive(Debug, PartialEq)]
enum Tok {
    /// Text shown as is.
    Lit(String),
    /// A digit placeholder: `0`, `#` or `?`.
    Digit(char),
    Dot,
    Comma,
    Percent,
    /// `E+` or `E-`: scientific notation, the exponent's digits after it (`-` shows only a
    /// negative exponent's sign).
    Exp(char),
}

/// A section's tokens (`[$₽-419]` gives its symbol; colours, conditions and locales drop out).
fn tokens(section: &str) -> Vec<Tok> {
    let mut out = vec![];
    let mut it = section.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                let mut s = String::new();
                for d in it.by_ref() {
                    if d == '"' {
                        break;
                    }
                    s.push(d);
                }
                out.push(Tok::Lit(s));
            }
            '\\' => {
                if let Some(d) = it.next() {
                    out.push(Tok::Lit(d.to_string()));
                }
            }
            // Padding the width of the next character: a space.
            '_' => {
                it.next();
                out.push(Tok::Lit(" ".into()));
            }
            // Repeat-to-fill: nothing on an axis.
            '*' => {
                it.next();
            }
            '[' => {
                let mut s = String::new();
                for d in it.by_ref() {
                    if d == ']' {
                        break;
                    }
                    s.push(d);
                }
                if let Some(sym) = s.strip_prefix('$') {
                    let sym = sym.split('-').next().unwrap_or("");
                    if !sym.is_empty() {
                        out.push(Tok::Lit(sym.to_string()));
                    }
                }
            }
            '0' | '#' | '?' => out.push(Tok::Digit(c)),
            '.' => out.push(Tok::Dot),
            ',' => out.push(Tok::Comma),
            '%' => out.push(Tok::Percent),
            'E' | 'e' if matches!(it.peek(), Some('+' | '-')) => {
                let sign = it.next().unwrap_or('+');
                out.push(Tok::Exp(sign));
            }
            '@' => {}
            _ => out.push(Tok::Lit(c.to_string())),
        }
    }
    out
}

/// The format's sections split at `;` (outside quotes, brackets and escapes).
fn sections(code: &str) -> Vec<&str> {
    let mut out = vec![];
    let (mut start, mut quoted, mut bracket, mut escaped) = (0, false, false, false);
    for (i, c) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if !quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            ';' if !quoted && !bracket => {
                out.push(code.get(start..i).unwrap_or(""));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(code.get(start..).unwrap_or(""));
    out
}

/// `v` in the General format: up to 10 significant digits, no grouping.
fn general(v: f64, dec: char) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    let s = if !(1e-9..1e11).contains(&a) {
        format!("{v:.5E}")
    } else {
        let digits = (10 - (a.log10().floor() as i32 + 1)).clamp(0, 10) as usize;
        let s = format!("{v:.digits$}");
        if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
    };
    s.replace('.', &dec.to_string())
}

/// Group an integer's digits in threes with `sep`.
fn group(digits: &str, sep: char) -> String {
    let n = digits.chars().count();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(d);
    }
    out
}

/// `v` formatted with the Excel number format `code` in language `lang` (axis labels: the text
/// section of a format doesn't apply).
pub fn format(code: &str, v: f64, lang: Option<&str>) -> String {
    let (sep, dec) = marks(lang);
    if !v.is_finite() {
        return String::new();
    }
    let secs = sections(code);
    // The section for the value's sign; a negative or zero section shows the value unsigned.
    let (sec, v, minus) = match (secs.as_slice(), v) {
        ([_, neg, ..], v) if v < 0.0 => (*neg, -v, false),
        ([_, _, zero, ..], v) if v == 0.0 => (*zero, v, false),
        ([pos, ..], v) => (*pos, v.abs(), v < 0.0),
        ([], v) => ("General", v.abs(), v < 0.0),
    };
    if sec.trim().eq_ignore_ascii_case("general") || sec.trim().is_empty() {
        let s = general(v, dec);
        return if minus { format!("-{s}") } else { s };
    }
    let toks = tokens(sec);
    let first = toks.iter().position(|t| matches!(t, Tok::Digit(_)));
    let last = toks.iter().rposition(|t| matches!(t, Tok::Digit(_)));
    let (Some(first), Some(last)) = (first, last) else {
        // No digits: just the section's text ("-" for zero, a label).
        let s: String = toks.iter().filter_map(|t| if let Tok::Lit(s) = t { Some(s.as_str()) } else { None }).collect();
        return s.trim().to_string();
    };
    let percent = toks.contains(&Tok::Percent);
    if let Some(e) = toks.iter().position(|t| matches!(t, Tok::Exp(_))).filter(|e| (first..last).contains(e)) {
        return scientific(&toks, (first, e, last), v * if percent { 100.0 } else { 1.0 }, minus, dec);
    }
    let number = toks.get(first..=last).unwrap_or(&[]);
    let dot = number.iter().position(|t| *t == Tok::Dot);
    let (int_part, frac_part) = match dot {
        Some(d) => (number.get(..d).unwrap_or(&[]), number.get(d + 1..).unwrap_or(&[])),
        None => (number, &[][..]),
    };
    // Grouping: a comma (or the locale's space, written literally) between integer digits.
    let grouping = int_part.iter().any(|t| *t == Tok::Comma || matches!(t, Tok::Lit(s) if s.trim().is_empty()));
    // Commas right after the last digit scale by a thousand each (`#,##0,` = thousands).
    let after = toks.get(last + 1..).unwrap_or(&[]);
    let scale_commas = if dot.is_none() { after.iter().take_while(|t| **t == Tok::Comma).count() } else { 0 };
    let min_int = int_part.iter().filter(|t| **t == Tok::Digit('0')).count();
    let frac_digits: Vec<char> = frac_part.iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect();
    let max_frac = frac_digits.len();
    let min_frac = frac_digits.iter().filter(|c| **c == '0').count();
    let mut x = v * if percent { 100.0 } else { 1.0 } / 1000f64.powi(scale_commas as i32);
    if !x.is_finite() {
        x = 0.0;
    }
    let s = format!("{:.max_frac$}", round_half_away(x, max_frac));
    let (ip, fp) = s.split_once('.').unwrap_or((s.as_str(), ""));
    let mut fp = fp.to_string();
    while fp.len() > min_frac && fp.ends_with('0') {
        fp.pop();
    }
    let ip = ip.trim_start_matches('0');
    let ip = if ip.len() < min_int { format!("{}{ip}", "0".repeat(min_int - ip.len())) } else { ip.to_string() };
    let mut num = if grouping { group(&ip, sep) } else { ip };
    if !fp.is_empty() {
        num.push(dec);
        num.push_str(&fp);
    }
    let lits = |ts: &[Tok]| -> String {
        ts.iter()
            .filter_map(|t| match t {
                Tok::Lit(s) => Some(s.clone()),
                Tok::Percent => Some("%".into()),
                _ => None,
            })
            .collect()
    };
    let before = lits(toks.get(..first).unwrap_or(&[]));
    let after = lits(toks.get(last + 1..).unwrap_or(&[]));
    let out = format!("{before}{num}{after}");
    let out = out.trim();
    if minus && !out.is_empty() { format!("-{out}") } else { out.to_string() }
}

/// `x` rounded to `digits` decimals, halves away from zero as spreadsheets do (`format!` rounds
/// them to even).
fn round_half_away(x: f64, digits: usize) -> f64 {
    let scale = 10f64.powi(digits.min(15) as i32);
    let r = (x * scale).round() / scale;
    if r.is_finite() { r } else { x }
}

/// Literal text of `toks` (and `%` signs).
fn literals(toks: &[Tok]) -> String {
    toks.iter()
        .filter_map(|t| match t {
            Tok::Lit(s) => Some(s.clone()),
            Tok::Percent => Some("%".into()),
            _ => None,
        })
        .collect()
}

/// Scientific notation (`0.00E+00`): `v` (unsigned, `minus` for its sign) as a mantissa with the
/// decimals the code before the `E` asks for, and an exponent at least as wide as its digits.
/// `at`: the positions of the first digit, the `E` and the last digit.
fn scientific(toks: &[Tok], at: (usize, usize, usize), v: f64, minus: bool, dec: char) -> String {
    let (first, e, last) = at;
    let mant = toks.get(first..e).unwrap_or(&[]);
    let decimals = match mant.iter().position(|t| *t == Tok::Dot) {
        Some(d) => mant.get(d + 1..).unwrap_or(&[]).iter().filter(|t| matches!(t, Tok::Digit(_))).count().min(15),
        None => 0,
    };
    let exp_digits = toks.get(e + 1..=last).unwrap_or(&[]).iter().filter(|t| matches!(t, Tok::Digit(_))).count().clamp(1, 3);
    let sign = if let Some(Tok::Exp(s)) = toks.get(e) { *s } else { '+' };
    let s = format!("{v:.decimals$e}");
    let (m, x) = s.split_once('e').unwrap_or((s.as_str(), "0"));
    let x: i32 = x.parse().unwrap_or(0);
    let x_sign = if x < 0 {
        "-"
    } else if sign == '+' {
        "+"
    } else {
        ""
    };
    let num = format!("{}E{x_sign}{:0exp_digits$}", m.replace('.', &dec.to_string()), x.abs());
    let out = format!("{}{num}{}", literals(toks.get(..first).unwrap_or(&[])), literals(toks.get(last + 1..).unwrap_or(&[])));
    let out = out.trim();
    if minus && !out.is_empty() { format!("-{out}") } else { out.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUB: &str = r#"_-* #\ ##0\ "₽"_-;\-* #\ ##0\ "₽"_-;_-* "-"??\ "₽"_-;_-@_-"#;

    #[test]
    fn russian_accounting_roubles() {
        assert_eq!(format(RUB, 800_000.0, Some("ru-RU")), "800\u{a0}000 ₽");
        assert_eq!(format(RUB, 1_234_567.0, Some("ru-RU")), "1\u{a0}234\u{a0}567 ₽");
        assert_eq!(format(RUB, -5_000.0, Some("ru-RU")), "-5\u{a0}000 ₽");
        assert_eq!(format(RUB, 0.0, Some("ru-RU")), "- ₽");
    }

    #[test]
    fn common_codes() {
        assert_eq!(format("#,##0", 7000.0, Some("ru-RU")), "7\u{a0}000");
        assert_eq!(format("#,##0", 7000.0, Some("en-US")), "7,000");
        assert_eq!(format("#,##0", 7000.0, Some("de-DE")), "7.000");
        assert_eq!(format("0.0%", 0.125, Some("ru-RU")), "12,5%");
        assert_eq!(format("0.0%", 0.125, None), "12.5%");
        assert_eq!(format("0%", 0.0, None), "0%");
        assert_eq!(format("General", 2.5, Some("ru-RU")), "2,5");
        assert_eq!(format("General", 400_000.0, Some("ru-RU")), "400000");
        assert_eq!(format("0.00", 1.23456, None), "1.23");
        assert_eq!(format("#,##0.0#", 1234.5, None), "1,234.5");
        assert_eq!(format("#,##0,", 12_345_678.0, None), "12,346");
        assert_eq!(format("[$₽-419]#,##0", 1500.0, Some("ru-RU")), "₽1\u{a0}500");
        assert_eq!(format("0;[Red]-0", -3.0, None), "-3");
        assert_eq!(format("0;0", -3.0, None), "3");
        assert_eq!(format("0", -3.0, None), "-3");
        assert_eq!(format("\"x\"0", 5.0, None), "x5");
    }

    #[test]
    fn scientific_and_spreadsheet_rounding() {
        assert_eq!(format("0.00E+00", 1500.0, None), "1.50E+03");
        assert_eq!(format("0.0E+00", 0.00042, Some("de-DE")), "4,2E-04");
        assert_eq!(format("0E+0", -2.0e7, None), "-2E+7");
        assert_eq!(format("#,##0", 1234.5, None), "1,235");
        assert_eq!(format("0", 2.5, None), "3");
        assert_eq!(format("$#,##0", -12.0, None), "-$12");
        assert_eq!(format("0;(0)", -12.0, None), "(12)");
        assert_eq!(format("0.0 \"pts\"", 3.5, None), "3.5 pts");
        assert_eq!(format("#,##0,,\"M\"", 2_600_000.0, None), "3M");
        assert_eq!(format("General", 1.0 / 3.0, None), "0.3333333333");
    }

    #[test]
    fn never_panics() {
        for code in ["", ";", ";;;", "\"", "[", "\\", "0.", ".", "#,", "%", "_", "*", "@", "0E+00", "[$", "\"unterminated"] {
            for v in [0.0, -1.5, 1e300, f64::NAN, f64::INFINITY, 1e-300] {
                let _ = format(code, v, Some("ru"));
            }
        }
    }
}
