use anyhow::Result;
use crate::ir::*;
use super::{camel_case, go_comment};

/// Collapse a potentially multi-line string to a single line for use in
/// Go const blocks, where multi-line `//` comments require each line to
/// be individually prefixed (and the const block syntax makes that awkward).
fn single_line(s: &str) -> String {
    s.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render a fieldset as Go const block (bit positions and masks)
pub fn render(ir: &IR, fieldset: &FieldSet, fs_name: &str) -> Result<String> {
    let mut out = String::new();

    if let Some(desc) = &fieldset.description {
        out.push_str(&go_comment(&format!("{} {}", fs_name, single_line(desc))));
    }
    out.push_str("const (\n");

    for field in &fieldset.fields {
        let bit_offset = field.bit_offset.min_offset();
        let mask = 1u64.wrapping_shl(field.bit_size).wrapping_sub(1);
        let name = camel_case(fs_name);
        let fname = camel_case(&field.name);

        if let Some(desc) = &field.description {
            out.push_str(&format!("    // {} {}\n", fname, single_line(desc)));
        }
        out.push_str(&format!("    {}{}Pos = {}\n", name, fname, bit_offset));
        out.push_str(&format!("    {}{}Msk = 0x{:x} << {}\n", name, fname, mask, bit_offset));
    }

    out.push_str(")\n");

    // Render referenced enums outside the const block (they are typed constants)
    for field in &fieldset.fields {
        if let Some(enum_name) = &field.enumm {
            if let Some(enumm) = ir.enums.get(enum_name) {
                out.push('\n');
                out.push_str(&super::enumm::render(enumm, enum_name)?);
            }
        }
    }

    Ok(out)
}
