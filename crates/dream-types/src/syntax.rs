use crate::{TyKind, TypeCtx, TypeId};
use dream_syntax::nodes::types::{Type, primitive_type};
use dream_syntax::token::{syntax_token::SyntaxToken, token_kind::TokenKind};
use dream_text::{line_text::LineText, text_span::TextSpan};

impl TypeCtx {
    /// Finds an already-interned syntax type without changing the interner.
    pub fn lookup_type(&self, ty: &Type) -> Option<TypeId> {
        let kind = match ty {
            Type::Array(elem) => TyKind::Array(self.lookup_type(elem)?),
            Type::Tuple(elems) => TyKind::Tuple(
                elems
                    .iter()
                    .map(|elem| self.lookup_type(elem))
                    .collect::<Option<_>>()?,
            ),
            Type::Function(params, ret) => TyKind::Func(
                params
                    .iter()
                    .map(|param| self.lookup_type(param))
                    .collect::<Option<_>>()?,
                self.lookup_type(ret)?,
            ),
            Type::Struct(token, args) => {
                if args.is_none() {
                    return self.resolved_type(&token.text);
                }
                let kind = self.nominal_kind(&token.text)?;
                let def = self.resolve(kind, &token.text)?;
                let args = args
                    .as_ref()?
                    .iter()
                    .map(|arg| self.lookup_type(arg))
                    .collect::<Option<_>>()?;
                match kind {
                    crate::DefKind::Struct => TyKind::Struct(def, args),
                    crate::DefKind::Union => TyKind::Union(def, args),
                    crate::DefKind::Interface => TyKind::Interface(def, args),
                    crate::DefKind::Enum => TyKind::Enum(def),
                    crate::DefKind::Function => return None,
                }
            }
            Type::Generic(_) | Type::GenericFunctionItem(_) => return None,
            Type::Unknown => return Some(self.interner.error()),
            Type::Void => return Some(self.interner.void()),
            Type::Object(_) => return Some(self.interner.object()),
            Type::Integer(_) => TyKind::Prim(crate::PrimTy::Int),
            Type::UInt(_) => TyKind::Prim(crate::PrimTy::UInt),
            Type::Long(_) => TyKind::Prim(crate::PrimTy::Long),
            Type::ULong(_) => TyKind::Prim(crate::PrimTy::ULong),
            Type::ISize(_) => TyKind::Prim(crate::PrimTy::ISize),
            Type::USize(_) => TyKind::Prim(crate::PrimTy::USize),
            Type::Byte(_) => TyKind::Prim(crate::PrimTy::Byte),
            Type::Float(_) => TyKind::Prim(crate::PrimTy::Float),
            Type::Double(_) => TyKind::Prim(crate::PrimTy::Double),
            Type::Boolean(_) => TyKind::Prim(crate::PrimTy::Bool),
            Type::Char(_) => TyKind::Prim(crate::PrimTy::Char),
            Type::String(_) => TyKind::Prim(crate::PrimTy::String),
        };
        self.interner.lookup(&kind)
    }

    /// AST substitution still needs syntax nodes. A nominal is qualified unless its bare name means
    /// the same definition in every module, preserving its defining module when a concrete
    /// argument is substituted into a foreign template's lexical scope.
    pub fn syntax_type(&self, ty: TypeId) -> Type {
        let token = |name: &str| {
            SyntaxToken::new(
                TokenKind::IdentifierToken,
                TextSpan::new((0, 0), &LineText::new(String::new())),
                name.to_string(),
            )
        };
        let nominal = |def, args: &[TypeId]| {
            let info = self.defs.get(def);
            let name = if info.module_path.is_empty()
                || info.name.starts_with(&format!("{}::", info.module_path))
                || self.resolves_everywhere(def, &info.name)
            {
                info.name.clone()
            } else {
                format!("{}::{}", info.module_path, info.name)
            };
            Type::Struct(
                token(&name),
                (!args.is_empty()).then(|| args.iter().map(|&arg| self.syntax_type(arg)).collect()),
            )
        };
        match self.interner.kind(ty) {
            TyKind::Prim(prim) => {
                primitive_type(prim.name(), token(prim.name())).unwrap_or(Type::Unknown)
            }
            TyKind::Object => Type::Object(token("object")),
            TyKind::Void => Type::Void,
            TyKind::Error => Type::Unknown,
            TyKind::Js => Type::Struct(token("js"), None),
            TyKind::Array(elem) => Type::Array(Box::new(self.syntax_type(*elem))),
            TyKind::Tuple(elems) => {
                Type::Tuple(elems.iter().map(|&elem| self.syntax_type(elem)).collect())
            }
            TyKind::Func(params, ret) => Type::Function(
                params
                    .iter()
                    .map(|&param| self.syntax_type(param))
                    .collect(),
                Box::new(self.syntax_type(*ret)),
            ),
            TyKind::Struct(def, args) | TyKind::Union(def, args) | TyKind::Interface(def, args) => {
                nominal(*def, args)
            }
            TyKind::Enum(def) => nominal(*def, &[]),
        }
    }
}
