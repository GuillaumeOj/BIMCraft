//! Naming rules shared by the code generators (`schema-gen`, `express`): how a name from a
//! schema becomes a Rust type or field identifier.
//!
//! Each generator still checks that the identifiers it gets are distinct; these functions only
//! convert one name at a time.

/// Rust keywords that cannot be used even as raw identifiers.
pub const NOT_RAW: [&str; 4] = ["crate", "self", "super", "Self"];

/// Strict and reserved keywords of the 2024 edition, which need `r#` as identifiers.
const KEYWORDS: [&str; 48] = [
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield",
];

/// `Pset_WallCommon` becomes `PsetWallCommon`, `length_measure` `LengthMeasure`: each word
/// separated by `_` starts with a capital, the rest is kept.
#[must_use]
pub fn pascal_case(name: &str) -> String {
    name.split('_')
        .flat_map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase())
                .into_iter()
                .chain(chars)
        })
        .collect()
}

/// `IsExternal` becomes `is_external`, `UValue` becomes `u_value`, `HTTPProxy` `http_proxy`.
#[must_use]
pub fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (index, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && index > 0 {
            let previous = chars[index - 1];
            let next_is_lower = chars.get(index + 1).is_some_and(char::is_ascii_lowercase);
            if previous.is_ascii_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_ascii_uppercase() && next_is_lower)
            {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// `name` as an identifier: `r#type` for the keyword `type`, unchanged otherwise. Names in
/// [`NOT_RAW`] cannot be identifiers at all; callers reject them.
#[must_use]
pub fn identifier(name: &str) -> String {
    if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_pascal_case() {
        assert_eq!(pascal_case("Pset_WallCommon"), "PsetWallCommon");
        assert_eq!(pascal_case("Qto_wallBase"), "QtoWallBase");
        assert_eq!(pascal_case("IfcWall"), "IfcWall");
        assert_eq!(pascal_case("length_measure"), "LengthMeasure");
        assert_eq!(pascal_case("a__b"), "AB");
    }

    #[test]
    fn converts_to_snake_case() {
        assert_eq!(snake_case("IsExternal"), "is_external");
        assert_eq!(snake_case("UValue"), "u_value");
        assert_eq!(snake_case("HTTPProxy"), "http_proxy");
        assert_eq!(snake_case("Level2Height"), "level2_height");
        assert_eq!(snake_case("ABC"), "abc");
        assert_eq!(snake_case("GlobalId"), "global_id");
    }

    #[test]
    fn escapes_keywords() {
        assert_eq!(identifier("type"), "r#type");
        assert_eq!(identifier("gen"), "r#gen");
        assert_eq!(identifier("union"), "union");
        assert_eq!(identifier("height"), "height");
    }
}
