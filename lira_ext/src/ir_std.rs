pub fn stmt_const_parse(specifier: &str) -> anyhow::Result<u128> {
    if let Some(hex) = specifier.strip_prefix("0x") {
        return Ok(u128::from_str_radix(hex, 16)?);
    }
    if let Some(bin) = specifier.strip_prefix("0b") {
        return Ok(u128::from_str_radix(bin, 2)?);
    }
    if specifier.starts_with("-") {
        return Ok(specifier.parse::<i128>()? as u128);
    }
    Ok(specifier.parse()?)
}
