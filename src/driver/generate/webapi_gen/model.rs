use crate::driver::source_loader::ProgramAccumulator;
use dream_syntax::nodes::function::FunctionNode;
use indexmap::IndexMap;
use std::collections::HashSet;

pub(super) enum RouteKind {
    Http { method: String, path: String },
    WebSocket { path: String },
}

pub(super) struct RouteCandidate<'a, 'src> {
    pub(super) f: &'a FunctionNode<'src>,
    pub(super) kind: RouteKind,
    pub(super) prefix: String,
    pub(super) class_uses: Vec<String>,
    pub(super) call: String,
}

pub(super) struct Route {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) fn_name: String,
    pub(super) is_async: bool,
    pub(super) ret: String,
    pub(super) params: Vec<ParamPlan>,
    pub(super) uses: Vec<String>,
    pub(super) websocket: bool,
}

pub(super) struct ParamPlan {
    pub(super) name: String,
    pub(super) ty: String,
    pub(super) kind: ParamKind,
}

pub(super) enum ParamKind {
    Incoming,
    Context,
    Path(String),
    Query(String),
    Header(String),
    Cookie(String),
    Body,
    Form(String),
    File(String),
    Dep(String),
    ServerWs,
}

/// One `@get`/`@post`/… handler as written in source, before parameter binding is resolved.
pub(super) struct RouteSpec<'a, 'src> {
    pub(super) f: &'a FunctionNode<'src>,
    pub(super) method: &'a str,
    pub(super) path: &'a str,
    pub(super) call: &'a str,
    pub(super) uses: Vec<String>,
    pub(super) websocket: bool,
}

/// Program-wide tables every route binding resolves against.
pub(super) struct RouteCx<'a, 'src> {
    pub(super) fns: &'a IndexMap<String, &'a FunctionNode<'src>>,
    pub(super) json_names: &'a HashSet<String>,
    pub(super) acc: &'a ProgramAccumulator<'src>,
}
