const INTEGER_SUFFIXES: &[&str] = &[
    "c_ulonglong",
    "c_longlong",
    "c_ushort",
    "c_ulong",
    "c_short",
    "c_uint",
    "c_long",
    "c_schar",
    "c_uchar",
    "c_char",
    "c_int",
    "usize",
    "isize",
    "i64",
    "u64",
    "i32",
    "u32",
    "i16",
    "u16",
    "i8",
    "u8",
];

pub(crate) fn parse_integer(raw: &str) -> Result<(u128, Option<&'static str>), String> {
    let (digits, suffix) = INTEGER_SUFFIXES
        .iter()
        .find_map(|suffix| {
            raw.strip_suffix(suffix)
                .filter(|digits| !digits.is_empty())
                .map(|digits| (digits, Some(*suffix)))
        })
        .unwrap_or((raw, None));
    let (radix, digits) = if let Some(digits) = digits.strip_prefix("0b") {
        (2, digits)
    } else if let Some(digits) = digits.strip_prefix("0o") {
        (8, digits)
    } else if let Some(digits) = digits.strip_prefix("0x") {
        (16, digits)
    } else {
        (10, digits)
    };
    validate_digit_separators(digits)?;
    let cleaned = digits.replace('_', "");
    if cleaned.is_empty() || !cleaned.chars().all(|character| character.is_digit(radix)) {
        return Err(format!("invalid base-{radix} integer digits"));
    }
    let value = u128::from_str_radix(&cleaned, radix)
        .map_err(|_| "integer literal is too large".to_owned())?;
    Ok((value, suffix))
}

pub(crate) fn parse_float(raw: &str) -> Result<f64, String> {
    let digits = raw
        .strip_suffix("f32")
        .or_else(|| raw.strip_suffix("f64"))
        .unwrap_or(raw);
    validate_digit_separators(digits)?;
    let cleaned = digits.replace('_', "");
    let value = cleaned
        .parse::<f64>()
        .map_err(|_| "invalid floating-point literal".to_owned())?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err("floating-point literal is out of range".to_owned())
    }
}

fn validate_digit_separators(value: &str) -> Result<(), String> {
    if value.starts_with('_') || value.ends_with('_') || value.contains("__") {
        Err("`_` must appear only between literal digits".to_owned())
    } else {
        Ok(())
    }
}

pub(crate) fn decode_quoted(raw: &str, prefix: &str) -> Result<String, String> {
    let quoted = raw
        .strip_prefix(prefix)
        .ok_or_else(|| "invalid literal prefix".to_owned())?;
    if quoted.len() < 2 {
        return Err("unterminated literal".to_owned());
    }
    let quote = quoted
        .chars()
        .next()
        .ok_or_else(|| "unterminated literal".to_owned())?;
    if !matches!(quote, '\'' | '"') || !quoted.ends_with(quote) {
        return Err("unterminated literal".to_owned());
    }
    let content = &quoted[quote.len_utf8()..quoted.len() - quote.len_utf8()];
    let mut characters = content.chars().peekable();
    let mut output = String::new();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let escape = characters
            .next()
            .ok_or_else(|| "incomplete escape sequence".to_owned())?;
        match escape {
            '\\' => output.push('\\'),
            '"' => output.push('"'),
            '\'' => output.push('\''),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            '0' => output.push('\0'),
            'x' => {
                let high = characters
                    .next()
                    .and_then(|value| value.to_digit(16))
                    .ok_or_else(|| "`\\x` escape requires exactly two hex digits".to_owned())?;
                let low = characters
                    .next()
                    .and_then(|value| value.to_digit(16))
                    .ok_or_else(|| "`\\x` escape requires exactly two hex digits".to_owned())?;
                let byte = ((high << 4) | low) as u8;
                output.push(char::from(byte));
            }
            'u' => {
                if characters.next() != Some('{') {
                    return Err("Unicode escape must start with `\\u{`".to_owned());
                }
                let mut digits = String::new();
                let mut closed = false;
                for value in characters.by_ref() {
                    if value == '}' {
                        closed = true;
                        break;
                    }
                    digits.push(value);
                }
                if !closed
                    || digits.is_empty()
                    || digits.len() > 6
                    || !digits.chars().all(|value| value.is_ascii_hexdigit())
                {
                    return Err("invalid Unicode escape".to_owned());
                }
                let scalar = u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| "Unicode escape is not a scalar value".to_owned())?;
                output.push(scalar);
            }
            other => return Err(format!("unknown escape sequence `\\{other}`")),
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integer_bases_and_suffixes() {
        assert_eq!(
            parse_integer("0b1010_u8"),
            Err("`_` must appear only between literal digits".to_owned())
        );
        assert_eq!(parse_integer("0b1010u8"), Ok((10, Some("u8"))));
        assert_eq!(parse_integer("0o755"), Ok((493, None)));
        assert_eq!(parse_integer("0xff"), Ok((255, None)));
    }

    #[test]
    fn decodes_string_and_unicode_escapes() {
        assert_eq!(decode_quoted(r#""a\n\u{03bb}""#, "").unwrap(), "a\nλ");
        assert_eq!(decode_quoted(r#"'\x41'"#, "").unwrap(), "A");
        assert!(decode_quoted(r#""\q""#, "").is_err());
    }

    #[test]
    fn covers_literal_validation_failures_and_all_escapes() {
        assert!(parse_integer("").is_err());
        assert!(parse_integer("0x").is_err());
        assert!(parse_integer("0o8").is_err());
        assert!(parse_integer("_1").is_err());
        assert!(parse_integer("1__0").is_err());
        assert!(parse_integer("1_").is_err());
        assert!(parse_integer("9999999999999999999999999999999999999999").is_err());

        assert_eq!(parse_float("1.25f32"), Ok(1.25));
        assert_eq!(parse_float("1_000.5f64"), Ok(1000.5));
        assert!(parse_float("_1.0").is_err());
        assert!(parse_float("not-a-float").is_err());
        assert!(parse_float("1e9999").is_err());

        assert!(decode_quoted("x", "c").is_err());
        assert!(decode_quoted("", "").is_err());
        assert!(decode_quoted("xx", "").is_err());
        assert!(decode_quoted("\"unterminated", "").is_err());
        assert!(decode_quoted("\"trailing\\\"", "").is_err());
        assert_eq!(
            decode_quoted(r#""\\\"\'\n\r\t\0\x41""#, "").unwrap(),
            "\\\"'\n\r\t\0A"
        );
        assert!(decode_quoted(r#""\x""#, "").is_err());
        assert!(decode_quoted(r#""\x0g""#, "").is_err());
        assert!(decode_quoted(r#""\u1234""#, "").is_err());
        assert!(decode_quoted(r#""\u{}""#, "").is_err());
        assert!(decode_quoted(r#""\u{1234567}""#, "").is_err());
        assert!(decode_quoted(r#""\u{xyz}""#, "").is_err());
        assert!(decode_quoted(r#""\u{d800}""#, "").is_err());
        assert!(decode_quoted(r#""\u{1234""#, "").is_err());
    }
}
