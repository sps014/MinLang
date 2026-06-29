use crate::syntax::nodes::struct_node::StructDeclarationNode;
use crate::syntax::nodes::types::value_size_align;
use crate::syntax::nodes::Type;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StructFieldInfo {
    pub type_: Type,
    pub offset: usize,
    /// True when the field is declared `public`. Private (default) fields may only be accessed
    /// from within the declaring type's own methods.
    pub is_public: bool,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub fields: HashMap<String, StructFieldInfo>,
    pub size: usize,
    pub is_public: bool,
}

#[derive(Debug, Clone)]
pub struct StructTable {
    pub structs: HashMap<String, StructInfo>,
}

impl Default for StructTable {
    fn default() -> Self {
        Self::new()
    }
}

impl StructTable {
    pub fn new() -> Self {
        Self {
            structs: HashMap::new(),
        }
    }

    pub fn add_struct(&mut self, struct_decl: &StructDeclarationNode<'_>) -> Result<(), String> {
        let name = struct_decl.name.text.clone();
        if self.structs.contains_key(&name) {
            return Err(format!("Struct '{}' is already defined", name));
        }

        let mut fields = HashMap::new();
        let mut current_offset = 0;

        for field in &struct_decl.fields {
            let field_name = field.name.text.clone();
            if fields.contains_key(&field_name) {
                return Err(format!(
                    "Field '{}' is already defined in class '{}'",
                    field_name, name
                ));
            }

            // Use the structured type parsed by the parser, which preserves generic arguments
            // (e.g. `List<JsonValue>`, `Map<string, V>`) that the flat token text would lose.
            let field_type = field.field_type.clone();

            let (size, alignment) = value_size_align(field_type.get_type().as_str());

            // Align current_offset
            let remainder = current_offset % alignment;
            if remainder != 0 {
                current_offset += alignment - remainder;
            }

            fields.insert(
                field_name,
                StructFieldInfo {
                    type_: field_type,
                    offset: current_offset,
                    is_public: field.is_public,
                },
            );
            current_offset += size;
        }

        // Align total size to the largest alignment (usually 8 if double is present, else 4)
        let max_alignment = fields
            .values()
            .map(|f| value_size_align(f.type_.get_type().as_str()).1)
            .max()
            .unwrap_or(4);

        let remainder = current_offset % max_alignment;
        if remainder != 0 {
            current_offset += max_alignment - remainder;
        }

        self.structs.insert(
            name.clone(),
            StructInfo {
                name,
                fields,
                size: current_offset,
                is_public: struct_decl.is_public,
            },
        );

        Ok(())
    }

    pub fn get_struct(&self, name: &str) -> Option<&StructInfo> {
        self.structs.get(name)
    }

    /// Returns true if `type_name` is a heap-allocated reference type known to this table
    /// (a string, an array, or a registered struct).
    pub fn is_reference_type(&self, type_name: &str) -> bool {
        crate::syntax::nodes::types::is_reference_type_name(type_name, |name| {
            self.get_struct(name).is_some()
        })
    }
}
