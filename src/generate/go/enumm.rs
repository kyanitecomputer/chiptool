use anyhow::Result;
use crate::ir::*;
use super::camel_case;

/// Render an enum as Go type + constants
pub fn render(enumm: &Enum, enum_name: &str) -> Result<String> {
    let mut out = String::new();
    let type_name = camel_case(enum_name);

    let go_type = go_type_for_bits(enumm.bit_size);
    out.push_str(&format!("/* Enum: {} */\n", enum_name));
    out.push_str(&format!("type {} {}\n\n", type_name, go_type));

    out.push_str("const (\n");
    for variant in &enumm.variants {
        out.push_str(&format!(
            "    {} {} = {}\n",
            camel_case(&variant.name), type_name, variant.value
        ));
    }
    out.push_str(")\n");

    Ok(out)
}

fn go_type_for_bits(bit_size: u32) -> &'static str {
    match bit_size {
        1..=8 => "uint8",
        9..=16 => "uint16",
        17..=32 => "uint32",
        33..=64 => "uint64",
        _ => "uint32",
    }
}
