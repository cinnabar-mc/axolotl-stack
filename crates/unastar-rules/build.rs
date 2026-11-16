//! Schema identities and immutable numeric tables generated at build time.

use convert_case::{Case, Casing};
use proc_macro2::Literal;
use quote::{format_ident, quote};
use std::path::{Path, PathBuf};

fn main() {
    let schema_path = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join(
        "../unastar-data/data/vanilla_bp/metadata/json_schemas/server/entity/1.21.120/FilterSubject.json",
    );
    println!("cargo:rerun-if-changed={}", schema_path.display());
    let bytes = std::fs::read(&schema_path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; initialise crates/unastar-data/data/vanilla_bp",
            schema_path.display()
        )
    });
    let schema: serde_json::Value =
        serde_json::from_slice(&bytes).expect("FilterSubject schema JSON");
    assert_eq!(
        schema["x-underlying-type"], "int16",
        "review changed subject encoding"
    );
    let values = schema["enum"].as_array().expect("FilterSubject enum");
    let names: Vec<_> = values
        .iter()
        .map(|v| v.as_str().expect("subject name"))
        .collect();
    assert!(
        names.len() <= usize::from(u8::MAX) + 1,
        "subject ids no longer fit the runtime enum"
    );
    let variants: Vec<_> = names
        .iter()
        .map(|name| {
            // `Self` is reserved by Rust, unlike every other schema subject name.
            if *name == "self" {
                format_ident!("Self_")
            } else {
                format_ident!("{}", name.to_case(Case::Pascal))
            }
        })
        .collect();
    let ids: Vec<_> = (0..names.len()).map(Literal::usize_unsuffixed).collect();
    let count = names.len();
    let tokens = quote! {
        /// Filter subject from Mojang's canonical enum order. Compact JSON
        /// carries an integer; runtime storage needs only these logical kinds.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum Subject { #(#variants = #ids,)* }
        impl Subject {
            pub const ALL: [Subject; #count] = [#(Self::#variants,)*];
            pub fn name(self) -> &'static str {
                match self { #(Self::#variants => #names,)* }
            }
            pub fn from_name(name: &str) -> Option<Self> {
                match name { #(#names => Some(Self::#variants),)* _ => None }
            }
            pub fn from_id(id: i64) -> Option<Self> {
                match id { #(#ids => Some(Self::#variants),)* _ => None }
            }
        }
    };
    let file: syn::File = syn::parse2(tokens).expect("generated subject Rust");
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(
        out_dir.join("filter_subject.rs"),
        prettyplease::unparse(&file),
    )
    .expect("writing filter subject schema");
    write_sine_exceptions(&out_dir);
}

/// Decodes `data/sine-exceptions.bin` (format in `examples/sine_exceptions.rs`)
/// into sorted `u32` LE entries: the angle's bits, bit 31 set for one ULP up.
fn write_sine_exceptions(out_dir: &Path) {
    let path = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("data/sine-exceptions.bin");
    println!("cargo:rerun-if-changed={}", path.display());
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let count = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let counts = [count(0), count(4)];
    let mut at = 8;
    for (name, count) in ["sin", "cos"].into_iter().zip(counts) {
        let (mut out, mut bits) = (Vec::new(), 0u32);
        for _ in 0..count {
            let (mut value, mut shift) = (0u64, 0);
            loop {
                let byte = bytes[at];
                at += 1;
                value |= u64::from(byte & 0x7f) << shift;
                shift += 7;
                if byte & 0x80 == 0 {
                    break;
                }
            }
            bits += u32::try_from(value >> 1).expect("an angle step fits u32");
            let up = if value & 1 == 1 { 1u32 << 31 } else { 0 };
            out.extend((bits | up).to_le_bytes());
        }
        std::fs::write(out_dir.join(format!("{name}_exceptions.bin")), out)
            .expect("writing the sine exceptions");
    }
    assert_eq!(at, bytes.len(), "trailing bytes in {}", path.display());
}
