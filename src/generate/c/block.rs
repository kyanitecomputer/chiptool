use anyhow::{bail, Result};
use crate::ir::*;
use super::COptions;

/// Render a block as a C header file (pac/xxx.h).
pub fn render(ir: &IR, block: &Block, block_name: &str, opts: &COptions) -> Result<String> {
    let mut out = String::new();

    // Include guard
    let guard = format!(
        "{}{}_H",
        opts.include_guard_prefix.to_uppercase(),
        block_name.to_uppercase().replace("::", "_")
    );
    out.push_str(&format!("#ifndef {}\n#define {}\n\n", guard, guard));
    out.push_str("#include <stdint.h>\n\n");

    // Collect unique fieldsets referenced by this block's registers.
    // Enums are emitted inline inside fieldset rendering.
    let mut fieldsets: Vec<(String, &FieldSet)> = Vec::new();

    // Sort items by (byte_offset, name) — same ordering as the Rust generator.
    // This is required for correct padding calculation regardless of insertion order.
    let mut items: Vec<&BlockItem> = block.items.iter().collect();
    items.sort_by_key(|i| (i.byte_offset, i.name.clone()));

    out.push_str("typedef struct {\n");

    let mut prev_end: u32 = 0;

    for item in &items {
        let curr_offset = item.byte_offset;

        // Compute how many bytes this item occupies (element_size * count for arrays).
        let item_span = item_byte_span(ir, item);

        // Insert padding bytes to bridge any gap between consecutive items.
        if curr_offset > prev_end {
            let pad_len = curr_offset - prev_end;
            out.push_str(&format!(
                "    volatile uint8_t _pad{:#04x}[{}];\n",
                curr_offset, pad_len
            ));
        }

        match &item.inner {
            BlockItemInner::Register(r) => {
                let c_type = c_type_for_bits(r.bit_size)?;
                let desc = item.description.as_deref().unwrap_or("");

                // Collect fieldset ref for emission below the struct.
                if let Some(fs_name) = &r.fieldset {
                    if let Some(fs) = ir.fieldsets.get(fs_name) {
                        if !fieldsets.iter().any(|(n, _)| n == fs_name) {
                            fieldsets.push((fs_name.clone(), fs));
                        }
                    }
                }

                if let Some(array) = &item.array {
                    let len = array.len();
                    out.push_str(&format!(
                        "    volatile {} {}[{}]; /* 0x{:04x}: {} */\n",
                        c_type, item.name, len, curr_offset, desc
                    ));
                } else {
                    out.push_str(&format!(
                        "    volatile {} {}; /* 0x{:04x}: {} */\n",
                        c_type, item.name, curr_offset, desc
                    ));
                }
            }
            BlockItemInner::Block(b) => {
                let td = typedef_name(&b.block);
                let desc = item.description.as_deref().unwrap_or("");
                if let Some(array) = &item.array {
                    let len = array.len();
                    out.push_str(&format!(
                        "    {} {}[{}]; /* 0x{:04x}: {} */\n",
                        td, item.name, len, curr_offset, desc
                    ));
                } else {
                    out.push_str(&format!(
                        "    {} {}; /* 0x{:04x}: {} */\n",
                        td, item.name, curr_offset, desc
                    ));
                }
            }
        }

        prev_end = curr_offset + item_span;
    }

    // Close struct typedef.
    out.push_str(&format!("}} {};\n\n", typedef_name(block_name)));

    // Emit fieldset bit-field defines after the struct (matches ast-cf-runtime layout).
    for (fs_name, fs) in fieldsets {
        out.push_str(&super::fieldset::render(ir, fs, &fs_name)?);
        out.push('\n');
    }

    out.push_str(&format!("#endif /* {} */\n", guard));
    Ok(out)
}

/// Compute the full byte span of a block item, accounting for arrays.
///
/// For a scalar register this is `bit_size / 8`.
/// For a regular array this is `len * stride` (the stride already encodes element
/// size plus any per-element padding).
/// For a cursed array this is `last_offset + element_size` relative to the
/// item's own base offset (offsets are relative to the item base).
/// For a nested block we sort its items so `items.last()` is the highest-offset
/// member, then add that member's own span.
fn item_byte_span(ir: &IR, item: &BlockItem) -> u32 {
    match &item.inner {
        BlockItemInner::Register(r) => {
            let elem = r.bit_size / 8;
            match &item.array {
                None => elem,
                Some(Array::Regular(a)) => a.len * a.stride,
                Some(Array::Cursed(a)) => {
                    // Offsets are relative; the last element ends at max_offset + elem.
                    a.offsets.iter().copied().max().unwrap_or(0) + elem
                }
            }
        }
        BlockItemInner::Block(b) => {
            // Size of one instance of the referenced block.
            let one = if let Some(ref_block) = ir.blocks.get(&b.block) {
                let mut sub: Vec<&BlockItem> = ref_block.items.iter().collect();
                sub.sort_by_key(|i| i.byte_offset);
                sub.last()
                    .map(|last| last.byte_offset + item_byte_span(ir, last))
                    .unwrap_or(4)
            } else {
                4
            };
            // Account for array span when the block item itself is arrayed.
            // Use stride (not natural size) for regular arrays, since the
            // stride may include inter-element padding.
            match &item.array {
                None => one,
                Some(Array::Regular(a)) => a.len * a.stride,
                Some(Array::Cursed(a)) => {
                    a.offsets.iter().copied().max().unwrap_or(0) + one
                }
            }
        }
    }
}

/// Generate the C typedef name for a block path (e.g. "uart::Uart" → "UART_UARTTypeDef").
pub fn typedef_name(block_name: &str) -> String {
    format!("{}TypeDef", block_name.to_uppercase().replace("::", "_"))
}

fn c_type_for_bits(bit_size: u32) -> Result<&'static str> {
    match bit_size {
        8 => Ok("uint8_t"),
        16 => Ok("uint16_t"),
        32 => Ok("uint32_t"),
        64 => Ok("uint64_t"),
        _ => bail!("Unsupported register bit size: {}", bit_size),
    }
}
