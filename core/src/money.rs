//! Currency handling. All amounts are stored as integer minor units (cents, pence, yen…).

use serde::{Deserialize, Serialize};

/// USD plus the next six most-traded world currencies (BIS ranking).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Currency {
    #[default]
    USD,
    EUR,
    JPY,
    GBP,
    CNY,
    AUD,
    CAD,
}

impl Currency {
    pub const ALL: [Currency; 7] = [
        Currency::USD,
        Currency::EUR,
        Currency::JPY,
        Currency::GBP,
        Currency::CNY,
        Currency::AUD,
        Currency::CAD,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Currency::USD => "USD",
            Currency::EUR => "EUR",
            Currency::JPY => "JPY",
            Currency::GBP => "GBP",
            Currency::CNY => "CNY",
            Currency::AUD => "AUD",
            Currency::CAD => "CAD",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Currency::USD => "US Dollar",
            Currency::EUR => "Euro",
            Currency::JPY => "Japanese Yen",
            Currency::GBP => "British Pound",
            Currency::CNY => "Chinese Yuan",
            Currency::AUD => "Australian Dollar",
            Currency::CAD => "Canadian Dollar",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Currency::USD => "$",
            Currency::EUR => "€",
            Currency::JPY => "¥",
            Currency::GBP => "£",
            Currency::CNY => "CN¥",
            Currency::AUD => "A$",
            Currency::CAD => "C$",
        }
    }

    /// Number of digits after the decimal point.
    pub fn decimals(self) -> u32 {
        match self {
            Currency::JPY => 0,
            _ => 2,
        }
    }

    /// The smallest unit's name, used in rounding notes ("1 cent").
    pub fn minor_name(self, n: i64) -> String {
        let (one, many) = match self {
            Currency::JPY => ("yen", "yen"),
            Currency::GBP => ("penny", "pence"),
            Currency::CNY => ("fen", "fen"),
            _ => ("cent", "cents"),
        };
        format!("{} {}", n, if n.abs() == 1 { one } else { many })
    }

    pub fn from_index(i: usize) -> Currency {
        Currency::ALL.get(i).copied().unwrap_or_default()
    }

    pub fn index(self) -> usize {
        Currency::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    pub fn label(self) -> String {
        format!("{} {} – {}", self.code(), self.symbol(), self.name())
    }

    fn scale(self) -> i64 {
        10i64.pow(self.decimals())
    }

    /// Plain number with thousands separators, no symbol: "1,234.50".
    pub fn format_number(self, minor: i64) -> String {
        let neg = minor < 0;
        let abs = minor.unsigned_abs() as i64;
        let scale = self.scale();
        let whole = abs / scale;
        let frac = abs % scale;
        let mut digits = whole.to_string();
        let mut grouped = String::new();
        while digits.len() > 3 {
            let tail = digits.split_off(digits.len() - 3);
            grouped = format!(",{}{}", tail, grouped);
        }
        grouped = format!("{}{}", digits, grouped);
        let body = if self.decimals() > 0 {
            format!(
                "{}.{:0width$}",
                grouped,
                frac,
                width = self.decimals() as usize
            )
        } else {
            grouped
        };
        if neg {
            format!("-{}", body)
        } else {
            body
        }
    }

    /// "$1,234.50", "-€3.00", "¥1,200"
    pub fn format(self, minor: i64) -> String {
        let n = self.format_number(minor.abs());
        if minor < 0 {
            format!("-{}{}", self.symbol(), n)
        } else {
            format!("{}{}", self.symbol(), n)
        }
    }

    /// Format with the ISO code instead of a symbol: "USD 1,234.50" (used where glyphs may be unavailable).
    pub fn format_code(self, minor: i64) -> String {
        format!("{} {}", self.code(), self.format_number(minor))
    }

    /// Parse user input like "1,234.5", "$12", "12.345" (rounded half-up). Empty → 0.
    pub fn parse(self, input: &str) -> Option<i64> {
        let cleaned: String = input
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        if cleaned.is_empty() || cleaned == "-" || cleaned == "." {
            return if input.trim().is_empty() {
                Some(0)
            } else {
                None
            };
        }
        let neg = cleaned.starts_with('-');
        let cleaned = cleaned.trim_start_matches('-');
        if cleaned.contains('-') || cleaned.matches('.').count() > 1 {
            return None;
        }
        let (w, f) = match cleaned.split_once('.') {
            Some((w, f)) => (w, f),
            None => (cleaned, ""),
        };
        let whole: i64 = if w.is_empty() { 0 } else { w.parse().ok()? };
        let d = self.decimals() as usize;
        let mut frac_digits: String = f.chars().take(d).collect();
        while frac_digits.len() < d {
            frac_digits.push('0');
        }
        let mut frac: i64 = if frac_digits.is_empty() {
            0
        } else {
            frac_digits.parse().ok()?
        };
        // round half-up on the next digit
        if let Some(next) = f.chars().nth(d) {
            if next >= '5' {
                frac += 1;
            }
        }
        let v = whole.checked_mul(self.scale())?.checked_add(frac)?;
        Some(if neg { -v } else { v })
    }

    /// Value for an editable text field (no symbol, no separators): "1234.50".
    pub fn edit_string(self, minor: i64) -> String {
        if minor == 0 {
            return String::new();
        }
        self.format_number(minor).replace(',', "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse() {
        let c = Currency::USD;
        assert_eq!(c.format(123456), "$1,234.56");
        assert_eq!(c.format(-5), "-$0.05");
        assert_eq!(c.parse("$1,234.56"), Some(123456));
        assert_eq!(c.parse("12"), Some(1200));
        assert_eq!(c.parse("12.345"), Some(1235));
        assert_eq!(c.parse(""), Some(0));
        assert_eq!(c.parse("abc"), None);
        assert_eq!(Currency::JPY.format(1200), "¥1,200");
        assert_eq!(Currency::JPY.parse("1200.6"), Some(1201));
        assert_eq!(c.edit_string(123456), "1234.56");
    }
}
