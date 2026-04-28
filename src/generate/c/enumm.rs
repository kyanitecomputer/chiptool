use anyhow::Result;
use crate::ir::*;

/// Render an enum as C #defines
pub fn render(enumm: &Enum, enum_name: &str) -> Result<String> {
    let mut out = String::new();
    let prefix = format!("{}_", enum_name.to_uppercase().replace("::", "_"));

    out.push_str(&format!("/* Enum: {} */\n", enum_name));

    for variant in &enumm.variants {
        out.push_str(&format!(
            "#define {}{} {}\n",
            prefix, variant.name.to_uppercase(), variant.value
        ));
    }

    Ok(out)
}
