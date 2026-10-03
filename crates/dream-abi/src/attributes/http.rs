//! Web API route, middleware and parameter-binding attributes.

use super::*;

pub(super) const SPECS: &[AttributeSpec] = &[
    AttributeSpec {
        name: "get",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP GET route (`system.webapi`): `@get(\"/items/{id}\")`.",
    },
    AttributeSpec {
        name: "post",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP POST route (`system.webapi`): `@post(\"/items\")`.",
    },
    AttributeSpec {
        name: "put",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP PUT route (`system.webapi`).",
    },
    AttributeSpec {
        name: "patch",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP PATCH route (`system.webapi`).",
    },
    AttributeSpec {
        name: "delete",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP DELETE route (`system.webapi`).",
    },
    AttributeSpec {
        name: "head",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP HEAD route (`system.webapi`).",
    },
    AttributeSpec {
        name: "options",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP OPTIONS route (`system.webapi`).",
    },
    AttributeSpec {
        name: "middleware",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::Int],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Registers an onion middleware (`system.webapi`). Optional `order` int; lower runs first.",
    },
    AttributeSpec {
        name: "use",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
            AttributeTarget::Struct,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::Enum],
            min: 1,
            max: 1,
        },
        repeatable: true,
        doc: "Attaches named middleware to one route or every method of an `@http_group` class: `@use(require_auth)`.",
    },
    AttributeSpec {
        name: "http_group",
        targets: &[AttributeTarget::Struct],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Prefixes every HTTP route method on a class (`@http_group(\"/api\")` + `@get(\"/items\")` → `/api/items`).",
    },
    AttributeSpec {
        name: "websocket",
        targets: &[
            AttributeTarget::Function,
            AttributeTarget::Method,
            AttributeTarget::StaticMethod,
        ],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "HTTP GET WebSocket upgrade route (`system.webapi`): `@websocket(\"/ws\")`.",
    },
    AttributeSpec {
        name: "dep",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::Enum],
            min: 1,
            max: 1,
        },
        repeatable: false,
        doc: "Injects a dependency function (FastAPI `Depends`): `@dep(current_user) user: User`.",
    },
    AttributeSpec {
        name: "query",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a query parameter; optional name override `@query(\"q\")`.",
    },
    AttributeSpec {
        name: "header",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a request header; optional name `@header(\"Authorization\")`.",
    },
    AttributeSpec {
        name: "cookie",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a cookie; optional name `@cookie(\"session\")`.",
    },
    AttributeSpec {
        name: "body",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::None,
        repeatable: false,
        doc: "Binds the JSON (or raw) request body (`system.webapi`).",
    },
    AttributeSpec {
        name: "form",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a `multipart/form-data` text field; optional name `@form(\"title\")`.",
    },
    AttributeSpec {
        name: "file",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a `multipart/form-data` file part to `UploadedFile`; optional name `@file(\"upload\")`.",
    },
    AttributeSpec {
        name: "path",
        targets: &[AttributeTarget::Parameter],
        args: ArgShape::Args {
            kinds: &[ArgKind::String],
            min: 0,
            max: 1,
        },
        repeatable: false,
        doc: "Binds a path parameter when the name differs from `{segment}`: `@path(\"id\")`.",
    },
];
