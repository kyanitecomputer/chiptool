use anyhow::Result;
use crate::ir::*;

/// Render a fieldset as C bit-field defines
pub fn render(ir: &IR, fieldset: &FieldSet, fs_name: &str) -> Result<String> {
    let mut out = String::new();
    let prefix = format!("{}_", fs_name.to_uppercase().replace("::", "_"));

    out.push_str(&format!("/* Fieldset: {} */\n", fs_name));

    for field in &fieldset.fields {
        let bit_offset = match &field.bit_offset {
            BitOffset::Regular(pos) => *pos,
            BitOffset::Cursed(_) => {
                // For cursed offsets, use the min offset as position
                field.bit_offset.min_offset()
            }
        };

        let mask = 1u64.wrapping_shl(field.bit_size).wrapping_sub(1);

        out.push_str(&format!(
            "#define {}{}_Pos {}\n",
            prefix, field.name.to_uppercase(), bit_offset
        ));
        out.push_str(&format!(
            "#define {}{}_Msk ({}U << {})\n",
            prefix, field.name.to_uppercase(), mask, bit_offset
        ));

        // Render referenced enum if present
        if let Some(enum_name) = &field.enumm {
            if let Some(enumm) = ir.enums.get(enum_name) {
                out.push_str(&super::enumm::render(enumm, enum_name)?);
            }
        }
    }

    Ok(out)
}
