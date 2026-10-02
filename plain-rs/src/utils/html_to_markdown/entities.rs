use regex::Regex;
use std::sync::LazyLock;

pub(super) fn decode(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    static ENTITY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z][a-zA-Z0-9]*);").unwrap());
    ENTITY
        .replace_all(input, |c: &regex::Captures| {
            let name = &c[1];
            if let Some(hex) = name.strip_prefix("#x") {
                return u32::from_str_radix(hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| c[0].to_string());
            }
            if let Some(decimal) = name.strip_prefix('#') {
                return decimal
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| c[0].to_string());
            }
            match name {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "apos" => "'",
                "nbsp" => "\u{00A0}",
                "copy" => "\u{00A9}",
                "reg" => "\u{00AE}",
                "trade" => "\u{2122}",
                "mdash" => "\u{2014}",
                "ndash" => "\u{2013}",
                "hellip" => "\u{2026}",
                "lsquo" => "\u{2018}",
                "rsquo" => "\u{2019}",
                "ldquo" => "\u{201C}",
                "rdquo" => "\u{201D}",
                "laquo" => "\u{00AB}",
                "raquo" => "\u{00BB}",
                "deg" => "\u{00B0}",
                "plusmn" => "\u{00B1}",
                "times" => "\u{00D7}",
                "divide" => "\u{00F7}",
                "frac12" => "\u{00BD}",
                "frac14" => "\u{00BC}",
                "frac34" => "\u{00BE}",
                "sup2" => "\u{00B2}",
                "sup3" => "\u{00B3}",
                "micro" => "\u{00B5}",
                "para" => "\u{00B6}",
                "middot" => "\u{00B7}",
                "cent" => "\u{00A2}",
                "pound" => "\u{00A3}",
                "yen" => "\u{00A5}",
                "euro" => "\u{20AC}",
                "sect" => "\u{00A7}",
                "bull" => "\u{2022}",
                "dagger" => "\u{2020}",
                "Dagger" => "\u{2021}",
                "permil" => "\u{2030}",
                "prime" => "\u{2032}",
                "Prime" => "\u{2033}",
                "infin" => "\u{221E}",
                "ne" => "\u{2260}",
                "le" => "\u{2264}",
                "ge" => "\u{2265}",
                "larr" => "\u{2190}",
                "uarr" => "\u{2191}",
                "rarr" => "\u{2192}",
                "darr" => "\u{2193}",
                "harr" => "\u{2194}",
                "spades" => "\u{2660}",
                "clubs" => "\u{2663}",
                "hearts" => "\u{2665}",
                "diams" => "\u{2666}",
                "alpha" => "\u{03B1}",
                "beta" => "\u{03B2}",
                "gamma" => "\u{03B3}",
                "delta" => "\u{03B4}",
                "epsilon" => "\u{03B5}",
                "pi" => "\u{03C0}",
                "omega" => "\u{03C9}",
                "Alpha" => "\u{0391}",
                "Beta" => "\u{0392}",
                "Gamma" => "\u{0393}",
                "Delta" => "\u{0394}",
                "Omega" => "\u{03A9}",
                _ => &c[0],
            }
            .to_string()
        })
        .into_owned()
}
