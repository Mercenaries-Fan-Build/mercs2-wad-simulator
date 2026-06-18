use std::collections::BTreeMap;
use mercs2_formats::schema::SchemaFieldType;

/// Tracks schema field coverage during BE→LE conversion.
#[derive(Debug, Default)]
pub struct SchemaCoverageReport {
    /// Total schema field entries scanned across all COMP groups.
    pub total_schema_fields: u64,
    /// Raw type code → count of fields with that type.
    pub type_code_counts: BTreeMap<u32, u64>,
    /// Fields whose type code was NOT recognized by SchemaFieldType::from_code.
    pub unknown_fields: Vec<UnknownFieldEntry>,
    /// COMP groups that had a schm chunk but from_schm_body() returned None.
    pub schema_parse_failures: Vec<SchemaFailureEntry>,
    /// COMP groups with NO schm chunk → fell back to u32 sweep or hardcoded handler.
    pub no_schema_components: Vec<NoSchemaEntry>,
    /// Non-ECS descriptor bodies that hit the catch-all swap_u32_array path.
    pub generic_fallback_tags: Vec<GenericFallbackEntry>,
}

#[derive(Debug)]
pub struct UnknownFieldEntry {
    pub component_name: String,
    pub type_code: u32,
    pub field_name_hash: u32,
    pub field_byte_offset: u16,
}

#[derive(Debug)]
pub struct SchemaFailureEntry {
    pub component_name: String,
    pub unknown_codes_in_body: Vec<u32>,
}

#[derive(Debug)]
pub struct NoSchemaEntry {
    pub component_name: String,
    pub data_size: usize,
    pub swap_strategy: &'static str,
}

#[derive(Debug)]
pub struct GenericFallbackEntry {
    pub entry_idx: usize,
    pub type_hash: u32,
    pub type_name: String,
    pub tag: String,
    pub body_size: u32,
}

impl SchemaCoverageReport {
    pub fn record_field(&mut self, type_code: u32) {
        self.total_schema_fields += 1;
        *self.type_code_counts.entry(type_code).or_insert(0) += 1;
    }

    pub fn record_unknown_field(
        &mut self,
        component_name: &str,
        type_code: u32,
        field_name_hash: u32,
        field_byte_offset: u16,
    ) {
        self.unknown_fields.push(UnknownFieldEntry {
            component_name: component_name.to_string(),
            type_code,
            field_name_hash,
            field_byte_offset,
        });
    }

    pub fn record_schema_parse_failure(
        &mut self,
        component_name: &str,
        unknown_codes: Vec<u32>,
    ) {
        self.schema_parse_failures.push(SchemaFailureEntry {
            component_name: component_name.to_string(),
            unknown_codes_in_body: unknown_codes,
        });
    }

    pub fn record_no_schema(
        &mut self,
        component_name: &str,
        data_size: usize,
        swap_strategy: &'static str,
    ) {
        self.no_schema_components.push(NoSchemaEntry {
            component_name: component_name.to_string(),
            data_size,
            swap_strategy,
        });
    }

    pub fn record_generic_fallback(
        &mut self,
        entry_idx: usize,
        type_hash: u32,
        type_name: &str,
        tag: &str,
        body_size: u32,
    ) {
        self.generic_fallback_tags.push(GenericFallbackEntry {
            entry_idx,
            type_hash,
            type_name: type_name.to_string(),
            tag: tag.to_string(),
            body_size,
        });
    }

    pub fn print_report(&self) {
        eprintln!();
        eprintln!("=== Schema Coverage Report ===");
        eprintln!();

        eprintln!("Total schema fields scanned: {}", self.total_schema_fields);
        eprintln!();

        if !self.type_code_counts.is_empty() {
            eprintln!("Type code breakdown:");
            for (&code, &count) in &self.type_code_counts {
                let name = type_code_display_name(code);
                let known = SchemaFieldType::from_code(code).is_some();
                let marker = if known { " " } else { "!" };
                eprintln!("  {} code {:>2} ({:<10}) : {:>6} field(s)", marker, code, name, count);
            }
            eprintln!();
        }

        let unknown_count = self.unknown_fields.len();
        if unknown_count > 0 {
            eprintln!("** UNKNOWN type codes: {} field(s) across schema bodies **", unknown_count);
            for entry in &self.unknown_fields {
                eprintln!("  component={:<24} field_hash=0x{:08X} offset={:<4} type_code={}",
                    entry.component_name, entry.field_name_hash,
                    entry.field_byte_offset, entry.type_code);
            }
            eprintln!();
        } else {
            eprintln!("Unknown type codes: none (all fields recognized)");
            eprintln!();
        }

        if !self.schema_parse_failures.is_empty() {
            eprintln!("Schema parse failures (schm present, parse returned None): {}",
                self.schema_parse_failures.len());
            for entry in &self.schema_parse_failures {
                let codes: Vec<String> = entry.unknown_codes_in_body.iter()
                    .map(|c| c.to_string()).collect();
                eprintln!("  component={:<24} unknown_code(s): [{}]",
                    entry.component_name, codes.join(", "));
            }
            eprintln!();
        }

        if !self.no_schema_components.is_empty() {
            eprintln!("Components with NO schema ({} total):", self.no_schema_components.len());
            for entry in &self.no_schema_components {
                eprintln!("  {:<28} data_size={:<6} swap={}",
                    entry.component_name, entry.data_size, entry.swap_strategy);
            }
            eprintln!();
        }

        if !self.generic_fallback_tags.is_empty() {
            eprintln!("Non-ECS generic fallback tags ({} total):", self.generic_fallback_tags.len());
            for entry in &self.generic_fallback_tags {
                eprintln!("  entry[{}] type=0x{:08X} ({}) tag={:<4} body_size={} → u32_array sweep",
                    entry.entry_idx, entry.type_hash, entry.type_name,
                    entry.tag, entry.body_size);
            }
            eprintln!();
        }

        let total_issues = unknown_count
            + self.schema_parse_failures.len()
            + self.no_schema_components.iter()
                .filter(|e| e.swap_strategy == "u32_array sweep")
                .count()
            + self.generic_fallback_tags.len();

        if total_issues == 0 {
            eprintln!("Result: all data bodies have typed swap coverage.");
        } else {
            eprintln!("Result: {} item(s) using fallback/unknown swap paths.", total_issues);
        }
        eprintln!("=== End Schema Coverage Report ===");
    }
}

fn type_code_display_name(code: u32) -> &'static str {
    match code {
        1 => "Bit",
        2 => "U8",
        4 => "U16",
        5 => "F32",
        6 => "U32",
        7 => "Ref",
        8 => "StringRef",
        9 => "Flags",
        10 => "Vec3",
        11 => "Blob32",
        _ => "UNKNOWN",
    }
}
