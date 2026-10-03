use super::*;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn validate_compute_shader(
        &mut self,
        function: &FunctionNode<'a>,
        info: &FunctionTableInfo,
        diagnostics: &mut DiagnosticBag,
    ) {
        if function.is_async {
            diagnostics.report_error(
                format!("@compute kernel '{}' cannot be async", function.name.text),
                Some(function.name.position),
            );
        }
        if function.is_extern {
            diagnostics.report_error(
                format!("@compute kernel '{}' cannot be extern", function.name.text),
                Some(function.name.position),
            );
        }
        if !matches!(info.return_type, None | Some(Type::Void)) {
            diagnostics.report_error(
                format!("@compute kernel '{}' must return void", function.name.text),
                Some(function.name.position),
            );
        }
        for p in function.parameters.iter() {
            if !is_compute_param_type(&p.type_) {
                diagnostics.report_error(
                    format!(
                        "@compute kernel '{}' parameter '{}' has type '{}'; only primitives, unmanaged value structs, GpuBuffer<T>, GpuTexture, and GpuSampler are allowed",
                        function.name.text,
                        p.name.text,
                        self.ty_display(&p.type_)
                    ),
                    Some(p.name.position),
                );
            }
        }
    }

    pub(in crate::analyzer) fn validate_vertex_shader(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if function.is_async {
            diagnostics.report_error(
                format!("@vertex shader '{}' cannot be async", function.name.text),
                Some(function.name.position),
            );
        }
        if function.is_extern {
            diagnostics.report_error(
                format!("@vertex shader '{}' cannot be extern", function.name.text),
                Some(function.name.position),
            );
        }
        match &function.return_type {
            Some(Type::Struct(tok, None)) => {
                if let Some(info) = self.struct_table.get_struct(&tok.text) {
                    let ok = info.fields.iter().any(|(fname, f)| {
                        let is_pos =
                            f.builtin.as_deref() == Some("position") || fname == "position";
                        is_pos && matches!(&f.type_, Type::Struct(t, None) if t.text == "GpuVec4")
                    });
                    if !ok {
                        diagnostics.report_error(
                            format!(
                                "@vertex shader '{}' return struct '{}' must have a position builtin (`position: GpuVec4` or `@builtin(\"position\") GpuVec4`)",
                                function.name.text, tok.text
                            ),
                            Some(function.name.position),
                        );
                    }
                    self.check_location_duplicates(info, diagnostics, function.name.position);
                } else {
                    diagnostics.report_error(
                        format!(
                            "@vertex shader '{}' return type '{}' is not a known struct",
                            function.name.text, tok.text
                        ),
                        Some(function.name.position),
                    );
                }
            }
            _ => {
                diagnostics.report_error(
                    format!(
                        "@vertex shader '{}' must return a value struct with a position builtin",
                        function.name.text
                    ),
                    Some(function.name.position),
                );
            }
        }
        for p in function.parameters.iter() {
            if !is_render_param_type(&p.type_) {
                diagnostics.report_error(
                    format!(
                        "@vertex shader '{}' parameter '{}' has type '{}'; only primitives, unmanaged value structs, @readonly GpuBuffer, GpuTexture, and GpuSampler are allowed",
                        function.name.text,
                        p.name.text,
                        self.ty_display(&p.type_)
                    ),
                    Some(p.name.position),
                );
            }
            if let Type::Struct(tok, None) = &p.type_ {
                if !matches!(tok.text.as_str(), "GpuTexture" | "GpuSampler") {
                    if let Some(info) = self.struct_table.get_struct(&tok.text) {
                        self.check_location_duplicates(info, diagnostics, p.name.position);
                    }
                }
            }
        }
    }

    pub(in crate::analyzer) fn validate_fragment_shader(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if function.is_async {
            diagnostics.report_error(
                format!("@fragment shader '{}' cannot be async", function.name.text),
                Some(function.name.position),
            );
        }
        if function.is_extern {
            diagnostics.report_error(
                format!("@fragment shader '{}' cannot be extern", function.name.text),
                Some(function.name.position),
            );
        }
        match &function.return_type {
            Some(Type::Struct(tok, None)) if tok.text == "GpuVec4" => {}
            Some(Type::Struct(tok, None)) => {
                if let Some(info) = self.struct_table.get_struct(&tok.text) {
                    let mut has_color = false;
                    for (fname, field) in &info.fields {
                        if let Some(b) = field.builtin.as_deref() {
                            match b {
                                "frag_depth" if !matches!(&field.type_, Type::Float(_)) => {
                                    diagnostics.report_error(
                                        format!(
                                            "@fragment shader '{}' frag_depth field '{}' must be float",
                                            function.name.text, fname
                                        ),
                                        Some(function.name.position),
                                    );
                                }
                                // Writing coverage is how alpha-to-coverage and custom MSAA
                                // masking work. `int` because that is what the bitwise
                                // operators building a mask produce.
                                "sample_mask" if !matches!(&field.type_, Type::Integer(_)) => {
                                    diagnostics.report_error(
                                        format!(
                                            "@fragment shader '{}' sample_mask field '{}' must be int",
                                            function.name.text, fname
                                        ),
                                        Some(function.name.position),
                                    );
                                }
                                "frag_depth" | "sample_mask" => {}
                                _ => {
                                    diagnostics.report_error(
                                        format!(
                                            "@fragment shader '{}' output field '{}' has unsupported @builtin(\"{b}\")",
                                            function.name.text, fname
                                        ),
                                        Some(function.name.position),
                                    );
                                }
                            }
                        } else {
                            has_color = true;
                            if !matches!(&field.type_, Type::Struct(t, None) if t.text == "GpuVec4")
                            {
                                diagnostics.report_error(
                                    format!(
                                        "@fragment shader '{}' color output '{}' must be GpuVec4",
                                        function.name.text, fname
                                    ),
                                    Some(function.name.position),
                                );
                            }
                        }
                    }
                    if !has_color {
                        diagnostics.report_error(
                            format!(
                                "@fragment shader '{}' output struct '{}' needs at least one color @location field",
                                function.name.text, tok.text
                            ),
                            Some(function.name.position),
                        );
                    }
                    self.check_location_duplicates(info, diagnostics, function.name.position);
                } else {
                    diagnostics.report_error(
                        format!(
                            "@fragment shader '{}' return type '{}' is not a known struct",
                            function.name.text, tok.text
                        ),
                        Some(function.name.position),
                    );
                }
            }
            _ => {
                diagnostics.report_error(
                    format!(
                        "@fragment shader '{}' must return GpuVec4 or an unmanaged output struct",
                        function.name.text
                    ),
                    Some(function.name.position),
                );
            }
        }
        if let Some(first) = function.parameters.first() {
            if let Type::Struct(tok, None) = &first.type_ {
                if !matches!(tok.text.as_str(), "GpuTexture" | "GpuSampler") {
                    if let Some(info) = self.struct_table.get_struct(&tok.text) {
                        let ok = info.fields.iter().any(|(fname, f)| {
                            let is_pos =
                                f.builtin.as_deref() == Some("position") || fname == "position";
                            is_pos
                                && matches!(&f.type_, Type::Struct(t, None) if t.text == "GpuVec4")
                        });
                        if !ok {
                            diagnostics.report_error(
                                format!(
                                    "@fragment shader '{}' input struct '{}' must include a position builtin",
                                    function.name.text, tok.text
                                ),
                                Some(first.name.position),
                            );
                        }
                        self.check_location_duplicates(info, diagnostics, first.name.position);
                    }
                }
            }
        }
        for p in function.parameters.iter() {
            if !is_render_param_type(&p.type_) {
                diagnostics.report_error(
                    format!(
                        "@fragment shader '{}' parameter '{}' has type '{}'; only primitives, unmanaged value structs, @readonly GpuBuffer, GpuTexture, and GpuSampler are allowed",
                        function.name.text,
                        p.name.text,
                        self.ty_display(&p.type_)
                    ),
                    Some(p.name.position),
                );
            }
        }
    }

    pub(in crate::analyzer) fn check_location_duplicates(
        &self,
        info: &crate::struct_table::StructInfo,
        diagnostics: &mut DiagnosticBag,
        span: dream_text::text_span::TextSpan,
    ) {
        let mut used = indexmap::IndexMap::<u32, String>::new();
        let mut auto = 0u32;
        for (fname, field) in &info.fields {
            if field.builtin.is_some() || fname == "position" {
                continue;
            }
            let loc = match field.location {
                Some(n) => n,
                None => {
                    while used.contains_key(&auto) {
                        auto += 1;
                    }
                    auto
                }
            };
            if let Some(prev) = used.insert(loc, fname.clone()) {
                diagnostics.report_error(
                    format!(
                        "duplicate @location({}) on fields '{}' and '{}' in struct '{}'",
                        loc, prev, fname, info.name
                    ),
                    Some(span),
                );
            }
            if field.location.is_none() {
                auto = loc + 1;
            }
        }
    }
}
