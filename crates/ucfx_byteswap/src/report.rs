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
    /// Tags present in WAD data that are registered but NOT yet validated as WAD
    /// chunks (see mercs2_formats::tag_registry). These need deeper investigation
    /// before we can trust the conversion; surfaced loudly, not silently swapped.
    pub needs_investigation_tags: Vec<NeedsInvestigationEntry>,
}

/// A schema field with an unrecognized type code.
#[derive(Debug)]
pub struct UnknownFieldEntry {
    /// ECS component name (e.g., "Transform", "__hash_0x1234").
    pub component_name: String,
    /// Raw type code that didn't match SchemaFieldType::from_code().
    pub type_code: u32,
    /// Name hash of the field (helps identify which field it was).
    pub field_name_hash: u32,
    /// Byte offset of the field within the component data body.
    pub field_byte_offset: u16,
}

/// A COMP group with a schm chunk that failed to parse.
#[derive(Debug)]
pub struct SchemaFailureEntry {
    /// ECS component name.
    pub component_name: String,
    /// Type codes encountered in the schm body that prevented parsing.
    pub unknown_codes_in_body: Vec<u32>,
}

/// A COMP group with no schema (used hardcoded handler or generic fallback).
#[derive(Debug)]
pub struct NoSchemaEntry {
    /// ECS component name.
    pub component_name: String,
    /// Size of the data body in bytes.
    pub data_size: usize,
    /// How the body was swapped (e.g., "hardcoded handler", "numeric records", "u32_array sweep").
    pub swap_strategy: &'static str,
}

/// A non-ECS descriptor body that used the generic fallback swap.
#[derive(Debug)]
pub struct GenericFallbackEntry {
    /// Entry index in the block.
    pub entry_idx: usize,
    /// Type hash (identifies the container type, e.g., Texture, Mesh).
    pub type_hash: u32,
    /// Human-readable type name.
    pub type_name: String,
    /// Chunk tag (4-char identifier).
    pub tag: String,
    /// Size of the body in bytes.
    pub body_size: u32,
}

/// A tag present in WAD data that the engine dispatches on but which we have not
/// yet validated as a WAD chunk (registered-but-unvalidated UCFX tag, or a
/// non-UCFX subsystem). Converted with a generic u32 sweep that may be wrong.
#[derive(Debug)]
pub struct NeedsInvestigationEntry {
    pub entry_idx: usize,
    pub tag: String,
    pub subsystem: &'static str,
    pub note: &'static str,
    pub body_size: u32,
}

impl SchemaCoverageReport {
    /// Record a schema field with its type code.
    pub fn record_field(&mut self, type_code: u32) {
        self.total_schema_fields += 1;
        *self.type_code_counts.entry(type_code).or_insert(0) += 1;
    }

    /// Record a field with an unrecognized type code.
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

    /// Record a component whose schema failed to parse.
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

    /// Record a component with no schema (using a hardcoded handler or generic fallback).
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

    /// Record a non-ECS body that used the generic fallback swap.
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

    pub fn record_needs_investigation(
        &mut self,
        entry_idx: usize,
        tag: &str,
        subsystem: &'static str,
        note: &'static str,
        body_size: u32,
    ) {
        self.needs_investigation_tags.push(NeedsInvestigationEntry {
            entry_idx,
            tag: tag.to_string(),
            subsystem,
            note,
            body_size,
        });
    }

    /// Print the report to stderr in human-readable format.
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

        if !self.needs_investigation_tags.is_empty() {
            eprintln!("** REQUIRES DEEPER INVESTIGATION: {} tag occurrence(s) **",
                self.needs_investigation_tags.len());
            eprintln!("   (registered-but-unvalidated WAD tags / non-UCFX subsystems — \
                converted with a generic u32 sweep that may be wrong)");
            for entry in &self.needs_investigation_tags {
                eprintln!("  entry[{}] tag={:<4} [{}] body_size={} :: {}",
                    entry.entry_idx, entry.tag, entry.subsystem, entry.body_size, entry.note);
            }
            eprintln!();
        }

        let total_issues = unknown_count
            + self.schema_parse_failures.len()
            + self.no_schema_components.iter()
                .filter(|e| e.swap_strategy == "u32_array sweep")
                .count()
            + self.generic_fallback_tags.len()
            + self.needs_investigation_tags.len();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aset::AsetEntry;

    #[test]
    fn test_empty_report() {
        let report = SchemaCoverageReport::default();
        assert_eq!(report.total_schema_fields, 0);
        assert!(report.unknown_fields.is_empty());
        assert!(report.schema_parse_failures.is_empty());
        assert!(report.no_schema_components.is_empty());
        assert!(report.generic_fallback_tags.is_empty());
    }

    #[test]
    fn test_record_field() {
        let mut report = SchemaCoverageReport::default();
        report.record_field(6); // U32
        report.record_field(6);
        report.record_field(5); // F32

        assert_eq!(report.total_schema_fields, 3);
        assert_eq!(report.type_code_counts[&6], 2);
        assert_eq!(report.type_code_counts[&5], 1);
    }

    #[test]
    fn test_record_unknown_field() {
        let mut report = SchemaCoverageReport::default();
        report.record_unknown_field("Transform", 99, 0x12345678, 42);

        assert_eq!(report.unknown_fields.len(), 1);
        let entry = &report.unknown_fields[0];
        assert_eq!(entry.component_name, "Transform");
        assert_eq!(entry.type_code, 99);
        assert_eq!(entry.field_name_hash, 0x12345678);
        assert_eq!(entry.field_byte_offset, 42);
    }

    #[test]
    fn test_record_schema_parse_failure() {
        let mut report = SchemaCoverageReport::default();
        report.record_schema_parse_failure("CustomComp", vec![99, 88]);

        assert_eq!(report.schema_parse_failures.len(), 1);
        let entry = &report.schema_parse_failures[0];
        assert_eq!(entry.component_name, "CustomComp");
        assert_eq!(entry.unknown_codes_in_body, vec![99, 88]);
    }

    #[test]
    fn test_record_no_schema() {
        let mut report = SchemaCoverageReport::default();
        report.record_no_schema("Transform", 1024, "hardcoded handler");

        assert_eq!(report.no_schema_components.len(), 1);
        let entry = &report.no_schema_components[0];
        assert_eq!(entry.component_name, "Transform");
        assert_eq!(entry.data_size, 1024);
        assert_eq!(entry.swap_strategy, "hardcoded handler");
    }

    #[test]
    fn test_record_generic_fallback() {
        let mut report = SchemaCoverageReport::default();
        report.record_generic_fallback(5, 0xDEADBEEF, "Texture", "Body", 4096);

        assert_eq!(report.generic_fallback_tags.len(), 1);
        let entry = &report.generic_fallback_tags[0];
        assert_eq!(entry.entry_idx, 5);
        assert_eq!(entry.type_hash, 0xDEADBEEF);
        assert_eq!(entry.type_name, "Texture");
        assert_eq!(entry.tag, "Body");
        assert_eq!(entry.body_size, 4096);
    }

    #[test]
    fn test_aset_entry_sub_accessors() {
        let mut entry = AsetEntry {
            asset_hash: 0x12345678,
            u32_2: 0xABCD_1234,
            primary: false,
            in_base: true,
        };

        assert_eq!(entry.sub(), 0x1234);
        entry.set_sub(0x5678);
        assert_eq!(entry.sub(), 0x5678);
        assert_eq!(entry.u32_2, 0xABCD_5678);
    }
}
