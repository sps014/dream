use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::{Type, Visibility};
use dream_types::TypeId;
use indexmap::IndexMap;

#[derive(Debug, Clone)]
pub struct StructFieldInfo {
    pub type_: Type,
    pub ty: TypeId,
    /// Accessibility of the field. Private (default) fields may only be accessed from within the
    /// declaring type's own methods; `internal` fields from anywhere in the same module.
    pub visibility: Visibility,
    /// True when declared `weak`: an `Option<T>` field that does not hold a strong reference to
    /// its referent and is excluded from the reference-cycle graph.
    pub is_weak: bool,
    /// True when declared `unowned`: a plain reference-type field that does not hold a strong
    /// reference to its referent and is excluded from the reference-cycle graph.
    pub is_unowned: bool,
}

impl StructFieldInfo {
    /// True when this field is excluded from ARC strong-reference bookkeeping (`weak` or
    /// `unowned`), and therefore does not contribute an edge to the reference-cycle graph.
    pub fn is_non_owning(&self) -> bool {
        self.is_weak || self.is_unowned
    }
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub has_destructor: bool,
    pub name: String,
    /// Insertion-ordered declaration order, matching HIR layout field indices.
    pub fields: IndexMap<String, StructFieldInfo>,
    pub visibility: Visibility,
    /// True for `struct` (value) types: stored inline with copy semantics, not heap-allocated and
    /// reference-counted. Unions are always reference types (`false`).
    pub is_value: bool,
    /// True when the (value) struct carries `@packed`: fields are laid out with no inter-field
    /// alignment padding and the struct is `align=1`. Only meaningful for C ABI interop; a heap
    /// class or a union is always `false`.
    pub packed: bool,
    /// Source file this type was declared in, for file/module-level visibility: a non-public type
    /// is only referenceable from its own file. `None` for synthesized types (always visible).
    pub file_path: Option<std::rc::Rc<str>>,
}

#[derive(Debug, Clone)]
pub struct StructTable {
    /// Insertion-ordered (registration order) so codegen iterates types deterministically.
    pub structs: IndexMap<TypeId, StructInfo>,
}

impl Default for StructTable {
    fn default() -> Self {
        Self::new()
    }
}

impl StructTable {
    pub fn new() -> Self {
        Self {
            structs: IndexMap::new(),
        }
    }

    pub fn add_struct(
        &mut self,
        ty: TypeId,
        struct_decl: &StructDeclarationNode<'_>,
        field_types: &[TypeId],
    ) -> Result<(), String> {
        let name = struct_decl.name.text.clone();
        if self.structs.contains_key(&ty) {
            return Err(format!("Struct '{}' is already defined", name));
        }

        let packed =
            struct_decl.is_value && dream_abi::attributes::has_packed_attr(&struct_decl.attributes);

        let mut fields = IndexMap::new();
        debug_assert_eq!(struct_decl.fields.len(), field_types.len());
        for (field, &field_ty) in struct_decl.fields.iter().zip(field_types) {
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

            fields.insert(
                field_name,
                StructFieldInfo {
                    type_: field_type,
                    ty: field_ty,
                    visibility: field.visibility,
                    is_weak: field.is_weak,
                    is_unowned: field.is_unowned,
                },
            );
        }

        self.structs.insert(
            ty,
            StructInfo {
                has_destructor: struct_decl
                    .methods
                    .iter()
                    .any(|method| method.name.text == dream_syntax::nodes::types::DESTRUCTOR_NAME),
                name,
                fields,
                visibility: struct_decl.visibility,
                is_value: struct_decl.is_value,
                packed,
                file_path: struct_decl.file_path.clone(),
            },
        );

        Ok(())
    }

    /// Registers a discriminated union under `name` as a heap reference type. Unions carry no
    /// flat field map (their payload layout is variant-dependent and lives in the union table),
    /// but they still need an entry here so they receive a runtime type tag, count as a reference
    /// type, and get a (discriminant-aware) `$release_*` helper generated.
    pub fn add_union(
        &mut self,
        ty: TypeId,
        name: &str,
        visibility: Visibility,
        file_path: Option<std::rc::Rc<str>>,
    ) -> Result<(), String> {
        if self.structs.contains_key(&ty) {
            return Err(format!("Type '{}' is already defined", name));
        }
        self.structs.insert(
            ty,
            StructInfo {
                has_destructor: false,
                name: name.to_string(),
                fields: IndexMap::new(),
                visibility,
                is_value: false,
                packed: false,
                file_path,
            },
        );
        Ok(())
    }

    pub fn get_struct(&self, ty: TypeId) -> Option<&StructInfo> {
        self.structs.get(&ty)
    }
}
