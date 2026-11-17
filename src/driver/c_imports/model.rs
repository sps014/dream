use super::api::{Api, record_name};
use dream_abi::{c_abi::is_c_identifier, core_items::C_PTR_NAME};
use dream_types::CScalar;
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn children(node: &Value) -> impl Iterator<Item = &Value> {
    node["inner"].as_array().into_iter().flatten()
}

fn name(node: &Value) -> &str {
    node["name"].as_str().unwrap_or_default()
}

fn kind(node: &Value) -> &str {
    node["kind"].as_str().unwrap_or_default()
}

fn spelling(node: &Value) -> &str {
    node["type"]["qualType"].as_str().unwrap_or_default()
}

fn directly_in_header(node: &Value) -> bool {
    let loc = node["loc"]["expansionLoc"]
        .as_object()
        .map_or(&node["loc"], |_| &node["loc"]["expansionLoc"]);
    loc["offset"].is_number() && loc["includedFrom"].is_null() && node["isImplicit"] != true
}

pub(super) fn dream_identifier(name: &str) -> bool {
    if !is_c_identifier(name) {
        return false;
    }
    use dream_syntax::token::token_kind::TokenKind;
    let mut lexer = dream_syntax::lexer::Lexer::new(name.to_owned());
    let tokens = lexer.lex_all(&mut dream_diagnostics::DiagnosticBag::new(None));
    matches!(tokens.as_slice(), [token, _] if token.kind == TokenKind::IdentifierToken)
}

#[derive(Clone, Debug)]
pub(super) enum Type {
    Void,
    Scalar(CScalar),
    Pointer,
    Record(String),
    Callback(Vec<Type>, Box<Type>),
}

impl Type {
    pub(super) fn dream(&self, prefix: &str) -> String {
        match self {
            Self::Void => "void".into(),
            Self::Pointer => C_PTR_NAME.into(),
            Self::Record(name) => format!("{prefix}{name}"),
            Self::Scalar(scalar) => scalar.prim().name().into(),
            Self::Callback(params, ret) => format!(
                "fun({}): {}",
                params
                    .iter()
                    .map(|p| p.dream(prefix))
                    .collect::<Vec<_>>()
                    .join(", "),
                ret.dream(prefix),
            ),
        }
    }

    pub(super) fn c(&self) -> String {
        match self {
            Self::Void => "void".into(),
            Self::Pointer | Self::Callback(..) => "void *".into(),
            Self::Scalar(scalar) => scalar.c_name().into(),
            Self::Record(name) => format!("struct __dream_mirror_{name}"),
        }
    }
}

pub(super) struct Record {
    pub(super) c_name: String,
    pub(super) packed: bool,
    pub(super) fields: Vec<(String, Type)>,
}

pub(super) struct Function {
    pub(super) name: String,
    pub(super) params: Vec<Type>,
    pub(super) ret: Type,
}

pub(super) struct Header<'a> {
    nodes: &'a Value,
    typedefs: BTreeMap<&'a str, &'a str>,
    tags: BTreeMap<&'a str, (&'a Value, &'a str)>,
    long64: bool,
}

impl<'a> Header<'a> {
    pub(super) fn new(nodes: &'a Value, long64: bool) -> Self {
        let typedefs = children(nodes)
            .filter(|n| kind(n) == "TypedefDecl")
            .map(|n| (name(n), spelling(n)))
            .collect();
        let mut tags = BTreeMap::new();
        let anonymous: BTreeMap<_, _> = children(nodes)
            .filter(|n| matches!(kind(n), "RecordDecl" | "EnumDecl") && name(n).is_empty())
            .filter_map(|n| Some((n["id"].as_str()?, n)))
            .collect();
        for node in children(nodes) {
            match kind(node) {
                "RecordDecl" if node["completeDefinition"] == true && !name(node).is_empty() => {
                    tags.insert(
                        name(node),
                        (
                            node,
                            if node["tagUsed"] == "union" {
                                "union"
                            } else {
                                "struct"
                            },
                        ),
                    );
                }
                "EnumDecl"
                    if !name(node).is_empty()
                        && children(node).any(|n| kind(n) == "EnumConstantDecl") =>
                {
                    tags.insert(name(node), (node, "enum"));
                }
                "TypedefDecl" => {
                    if let Some(id) = children(node).find_map(|n| n["decl"]["id"].as_str())
                        && let Some(&tag) = anonymous.get(id)
                    {
                        tags.insert(name(node), (tag, ""));
                    }
                }
                _ => {}
            }
        }
        Self {
            nodes,
            typedefs,
            tags,
            long64,
        }
    }

    fn c_tag(&self, name: &str) -> String {
        match self.tags.get(name) {
            Some((_, tag)) if !tag.is_empty() => format!("{tag} {name}"),
            _ => name.to_owned(),
        }
    }

    pub(super) fn enum_probes(&self) -> impl Iterator<Item = (&str, String)> {
        self.tags.iter().filter(|(n, (node, _))| kind(node) == "EnumDecl" && is_c_identifier(n))
            .enumerate().map(|(i, (name, _))| {
                let c = self.c_tag(name);
                (*name, format!("enum {{ __dream_enum_{i} = _Generic(({c})0, signed char: 1, unsigned char: 2, short: 3, unsigned short: 4, int: 5, unsigned int: 6, long: 7, unsigned long: 8, long long: 9, unsigned long long: 10, default: 0) }};"))
            })
    }

    fn scalar(&self, text: &str) -> Option<CScalar> {
        use CScalar::*;
        Some(match text {
            "_Bool" | "bool" => Bool,
            "char" => Char,
            "signed char" => I8,
            "unsigned char" => U8,
            "short" | "short int" => I16,
            "unsigned short" | "unsigned short int" => U16,
            "int" | "signed int" | "signed" => I32,
            "unsigned" | "unsigned int" => U32,
            "long" | "long int" => {
                if self.long64 {
                    I64
                } else {
                    I32
                }
            }
            "unsigned long" | "unsigned long int" => {
                if self.long64 {
                    U64
                } else {
                    U32
                }
            }
            "long long" | "long long int" => I64,
            "unsigned long long" | "unsigned long long int" => U64,
            "intptr_t" | "ptrdiff_t" => ISize,
            "uintptr_t" | "size_t" => USize,
            "float" => F32,
            "double" => F64,
            _ => return None,
        })
    }

    fn ty(
        &self,
        text: &str,
        enums: &BTreeMap<String, CScalar>,
        depth: usize,
    ) -> Result<Type, String> {
        if depth > 32 {
            return Err("C type is nested too deeply".into());
        }
        let text = unqualified(text);
        if text.contains("__attribute__") || text.contains("_Atomic") {
            return Err(format!("unsupported C type attributes in '{text}'"));
        }
        if let Some(scalar) = self.scalar(text) {
            return Ok(Type::Scalar(scalar));
        }
        if text == "void" {
            return Ok(Type::Void);
        }
        if let Some((result, args)) = text.split_once("(*)(")
            && let Some(args) = args.strip_suffix(')')
        {
            let ret = self.ty(result, enums, depth + 1)?;
            if !matches!(ret, Type::Void | Type::Scalar(_) | Type::Pointer) {
                return Err(format!("unsupported C callback result '{result}'"));
            }
            if args.is_empty() {
                return Err("callback has no C prototype".into());
            }
            let params = if args == "void" {
                Vec::new()
            } else {
                args.split(',')
                    .map(|param| {
                        self.ty(param, enums, depth + 1).and_then(|ty| {
                            if matches!(ty, Type::Scalar(_) | Type::Pointer) {
                                Ok(ty)
                            } else {
                                Err(format!("unsupported C callback parameter '{param}'"))
                            }
                        })
                    })
                    .collect::<Result<_, _>>()?
            };
            return Ok(Type::Callback(params, Box::new(ret)));
        }
        if text.ends_with('*') {
            return Ok(Type::Pointer);
        }
        if let Some(tag) = text
            .strip_prefix("struct ")
            .or_else(|| text.strip_prefix("enum "))
            && self.tags.contains_key(tag)
        {
            return self.tag_type(tag, enums);
        }
        if let Some(aliased) = self.typedefs.get(text)
            && *aliased != text
        {
            return self.ty(aliased, enums, depth + 1);
        }
        Err(format!("unsupported C type '{text}'"))
    }

    fn tag_type(&self, name: &str, enums: &BTreeMap<String, CScalar>) -> Result<Type, String> {
        let (node, tag) = self.tags[name];
        if kind(node) == "EnumDecl" {
            return enums
                .get(name)
                .copied()
                .map(Type::Scalar)
                .ok_or_else(|| format!("unsupported C enum representation for '{name}'"));
        }
        if tag == "union" || node["tagUsed"] != "struct" {
            return Err(format!(
                "C union '{name}' has no Dream value representation"
            ));
        }
        Ok(Type::Record(name.to_owned()))
    }

    fn record(&self, name: &str, enums: &BTreeMap<String, CScalar>) -> Result<Record, String> {
        let (node, _) = self.tags[name];
        self.tag_type(name, enums)?;
        let invalid = children(node).any(|f| {
            let k = kind(f);
            k.ends_with("Attr") && k != "PackedAttr"
        });
        if invalid {
            return Err(format!(
                "unsupported layout attributes on C record '{name}'"
            ));
        }
        let fields = children(node)
            .filter(|n| kind(n) == "FieldDecl")
            .map(|field| {
                let field_name = self::name(field);
                if field["isBitfield"] == true
                    || !dream_identifier(field_name)
                    || children(field).any(|n| kind(n).ends_with("Attr"))
                {
                    return Err(format!(
                        "unsupported field '{field_name}' in C record '{name}'"
                    ));
                }
                match self.ty(spelling(field), enums, 0)? {
                    ty @ (Type::Scalar(_) | Type::Record(_)) => Ok((field_name.to_string(), ty)),
                    _ => Err(format!(
                        "unsupported field type for '{field_name}' in C record '{name}'"
                    )),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if fields.is_empty() {
            return Err(format!("C record '{name}' has no supported fields"));
        }
        Ok(Record {
            c_name: self.c_tag(name),
            packed: children(node).any(|n| kind(n) == "PackedAttr"),
            fields,
        })
    }

    fn function(
        &self,
        node: &Value,
        enums: &BTreeMap<String, CScalar>,
    ) -> Result<Function, String> {
        let name = name(node);
        if node["variadic"] == true || spelling(node).contains("...") {
            return Err("variadic C functions need a fixed-arity C wrapper".into());
        }
        if !dream_identifier(name) || node["storageClass"] == "static" || node["inline"] == true {
            return Err("the declaration is not an importable external C function".into());
        }
        let text = spelling(node);
        if text.contains("__attribute__")
            || children(node).any(|n| matches!(kind(n), "AsmLabelAttr" | "AliasAttr"))
        {
            return Err("nonstandard calling convention or symbol name".into());
        }
        let Some((ret, args)) = text.split_once('(') else {
            return Err("missing C function prototype".into());
        };
        if args == ")" {
            return Err("missing C parameter prototype".into());
        }
        let ret = self.ty(ret, enums, 0)?;
        if matches!(ret, Type::Callback(..)) {
            return Err("function pointer results need an explicit C wrapper".into());
        }
        let params = children(node)
            .filter(|n| kind(n) == "ParmVarDecl")
            .map(|p| self.ty(spelling(p), enums, 0))
            .collect::<Result<_, _>>()?;
        Ok(Function {
            name: name.to_owned(),
            params,
            ret,
        })
    }
}

fn unqualified(text: &str) -> &str {
    let mut text = text.trim();
    while let Some(next) = ["const ", "volatile ", "restrict "]
        .iter()
        .find_map(|p| text.strip_prefix(p))
    {
        text = next;
    }
    while let Some(next) = [
        " const",
        " volatile",
        " restrict",
        " _Nullable",
        " _Nonnull",
        " _Null_unspecified",
    ]
    .iter()
    .find_map(|p| text.strip_suffix(p))
    {
        text = next;
    }
    text.trim()
}

pub(super) fn constant_values(ast: &Value) -> BTreeMap<String, (String, String)> {
    fn constant(node: &Value) -> Option<&Value> {
        if kind(node) == "ConstantExpr" {
            return Some(node);
        }
        children(node).find_map(constant)
    }
    children(ast)
        .filter(|n| kind(n) == "EnumDecl")
        .flat_map(children)
        .filter_map(|n| {
            let value = constant(n)?;
            Some((
                name(n).to_owned(),
                (
                    spelling(value).to_owned(),
                    value["value"].as_str()?.to_owned(),
                ),
            ))
        })
        .collect()
}

impl Header<'_> {
    pub(super) fn api(
        &self,
        values: &BTreeMap<String, (String, String)>,
        macros: &[String],
    ) -> Api {
        let enums: BTreeMap<_, _> = self
            .enum_probes()
            .enumerate()
            .filter_map(|(i, (name, _))| {
                let value = &values.get(&format!("__dream_enum_{i}"))?.1;
                let spelling = [
                    "",
                    "signed char",
                    "unsigned char",
                    "short",
                    "unsigned short",
                    "int",
                    "unsigned int",
                    "long",
                    "unsigned long",
                    "long long",
                    "unsigned long long",
                ];
                Some((
                    name.to_owned(),
                    self.scalar(spelling.get(value.parse::<usize>().ok()?)?)?,
                ))
            })
            .collect();
        let mut api = Api {
            records: BTreeMap::new(),
            functions: BTreeMap::new(),
            constants: BTreeMap::new(),
            omissions: BTreeMap::new(),
        };
        for (name, (node, _)) in &self.tags {
            if directly_in_header(node) && kind(node) == "RecordDecl" {
                self.require_record(name, &enums, &mut api, 0);
            }
        }
        for node in children(self.nodes).filter(|n| directly_in_header(n)) {
            match kind(node) {
                "FunctionDecl" => match self.function(node, &enums) {
                    Ok(function) => {
                        let types = function.params.iter().chain([&function.ret]);
                        if types
                            .filter_map(record_name)
                            .all(|n| self.require_record(n, &enums, &mut api, 0))
                        {
                            api.functions.insert(function.name.clone(), function);
                        } else {
                            api.omissions.insert(
                                name(node).into(),
                                "a by-value record is unsupported".into(),
                            );
                        }
                    }
                    Err(reason) => {
                        api.omissions.insert(name(node).into(), reason);
                    }
                },
                "EnumDecl" => {
                    let mut next = Some(0_i128);
                    for c in children(node).filter(|n| kind(n) == "EnumConstantDecl") {
                        let name = name(c);
                        if let Some((_, value)) = values.get(name) {
                            next = value.parse().ok();
                        }
                        if let Some(value) = next {
                            api.add_constant(name, self.scalar(spelling(c)), &value.to_string());
                        }
                        next = next.and_then(|n| n.checked_add(1));
                    }
                }
                "VarDecl" => {
                    api.omissions.insert(
                        name(node).into(),
                        "C global variables are not supported".into(),
                    );
                }
                _ => {}
            }
        }
        for (i, name) in macros.iter().enumerate() {
            if let Some((ty, value)) = values.get(&format!("__dream_macro_{i}")) {
                api.add_constant(
                    name,
                    match self.ty(ty, &enums, 0) {
                        Ok(Type::Scalar(scalar)) => Some(scalar),
                        _ => None,
                    },
                    value,
                );
            } else {
                api.omissions
                    .entry(name.clone())
                    .or_insert_with(|| "macro does not evaluate to an integer constant".into());
            }
        }
        api
    }

    fn require_record(
        &self,
        name: &str,
        enums: &BTreeMap<String, CScalar>,
        api: &mut Api,
        depth: usize,
    ) -> bool {
        if depth >= 128 {
            return false;
        }
        if api.records.contains_key(name) {
            return true;
        }
        if api.omissions.contains_key(name) {
            return false;
        }
        match self.record(name, enums) {
            Ok(record)
                if record
                    .fields
                    .iter()
                    .filter_map(|(_, t)| record_name(t))
                    .all(|nested| self.require_record(nested, enums, api, depth + 1)) =>
            {
                api.records.insert(name.into(), record);
                true
            }
            Ok(_) => {
                api.omissions.insert(
                    name.into(),
                    "contains an unsupported nested C record".into(),
                );
                false
            }
            Err(reason) => {
                api.omissions.insert(name.into(), reason);
                false
            }
        }
    }
}
