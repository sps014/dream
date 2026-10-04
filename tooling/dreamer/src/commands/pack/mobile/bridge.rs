use super::identifier;
use anyhow::{bail, Result};
use dream_abi::exports::{ExportFunction, ExportKind as K, ExportType};
use std::collections::BTreeSet;

fn c_type(kind: &K) -> &'static str {
    match kind {
        K::Void => "void",
        K::Int => "int32_t",
        K::UInt | K::Bool | K::Byte | K::Char => "uint32_t",
        K::Long => "int64_t",
        K::ULong => "uint64_t",
        K::ISize => "intptr_t",
        K::USize => "uintptr_t",
        K::Float => "float",
        K::Double => "double",
        K::Opaque => "void *",
    }
}

pub(super) fn validate(functions: &[ExportFunction]) -> Result<()> {
    let mut names = BTreeSet::new();
    for f in functions {
        if !identifier(&f.name) || !names.insert(&f.name) {
            bail!("invalid or duplicate export {}", f.name);
        }
        for ty in std::iter::once(&f.ret).chain(f.params.iter().map(|p| &p.ty)) {
            if ty.c_type != c_type(&ty.kind) {
                bail!("export {} has inconsistent C type", f.name);
            }
        }
        if f.params.iter().any(|p| p.ty.kind == K::Void) {
            bail!("void parameter on export {}", f.name);
        }
    }
    Ok(())
}

pub(super) fn objc(name: &str, functions: &[ExportFunction]) -> (String, String) {
    let class = format!("Dream_{name}");
    let mut header = format!("#import <Foundation/Foundation.h>\n#include <stdint.h>\n#include \"dream_library.h\"\n@interface {class} : NSObject\n+ (void)attach;\n+ (void)detach;\n+ (void)releaseHandle:(uintptr_t)handle;\n");
    let mut implementation = format!("#import \"{class}.h\"\n@implementation {class}\n+ (void)attach {{ dream_thread_attach(); }}\n+ (void)detach {{ dream_thread_detach(); }}\n+ (void)releaseHandle:(uintptr_t)handle {{ dream_release((void *)handle); }}\n");
    for f in functions {
        let mut method = format!("+ ({})call_{}", f.ret.c_type, f.name);
        for (i, p) in f.params.iter().enumerate() {
            let label = if i == 0 {
                String::new()
            } else {
                format!(" arg{i}")
            };
            method.push_str(&format!("{label}:({})arg{i}", p.ty.c_type));
        }
        header.push_str(&format!("{method};\n"));
        let args = (0..f.params.len())
            .map(|i| format!("arg{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = if f.ret.kind == K::Void { "" } else { "return " };
        implementation.push_str(&format!("{method} {{ {ret}{}({args}); }}\n", f.name));
    }
    header.push_str("@end\n");
    implementation.push_str("@end\n");
    (header, implementation)
}

fn java_type(ty: &ExportType) -> &'static str {
    match ty.kind {
        K::Void => "void",
        K::Bool => "boolean",
        K::Float => "float",
        K::Double => "double",
        K::Long | K::ULong | K::ISize | K::USize | K::Opaque => "long",
        _ => "int",
    }
}

fn jni_type(ty: &ExportType) -> &'static str {
    match ty.kind {
        K::Void => "void",
        K::Bool => "jboolean",
        K::Float => "jfloat",
        K::Double => "jdouble",
        K::Long | K::ULong | K::ISize | K::USize | K::Opaque => "jlong",
        _ => "jint",
    }
}

fn jni_name(name: &str) -> String {
    name.replace('_', "_1").replace('.', "_")
}

pub(super) fn java_package(value: &str) -> Result<()> {
    let keywords = [
        "class",
        "package",
        "public",
        "private",
        "protected",
        "static",
        "native",
        "void",
        "int",
        "long",
        "float",
        "double",
        "boolean",
        "char",
        "byte",
        "short",
        "return",
        "if",
        "else",
        "switch",
        "case",
        "default",
        "while",
        "for",
        "do",
        "break",
        "continue",
        "new",
        "this",
        "super",
        "extends",
        "implements",
        "interface",
        "enum",
        "abstract",
        "final",
        "try",
        "catch",
        "finally",
        "throw",
        "throws",
        "synchronized",
        "volatile",
        "transient",
        "instanceof",
        "import",
        "assert",
        "const",
        "goto",
        "strictfp",
        "true",
        "false",
        "null",
        "_",
    ];
    if value
        .split('.')
        .any(|v| !identifier(v) || keywords.contains(&v))
    {
        bail!("invalid Java package {value}");
    }
    Ok(())
}

pub(super) fn jni(name: &str, package: &str, functions: &[ExportFunction]) -> (String, String) {
    let class = "DreamLibrary";
    let prefix = format!("Java_{}_{}", jni_name(package), class);
    let mut java = format!("package {package};\npublic final class {class} {{\n    private {class}() {{}}\n    static {{ System.loadLibrary(\"{name}_jni\"); }}\n    public static native void attach();\n    public static native void detach();\n    public static native void releaseHandle(long handle);\n");
    let mut c = format!("#include <jni.h>\n#include <stdint.h>\n#include \"dream_library.h\"\nJNIEXPORT void JNICALL {prefix}_attach(JNIEnv *env, jclass cls) {{ (void)env; (void)cls; dream_thread_attach(); }}\nJNIEXPORT void JNICALL {prefix}_detach(JNIEnv *env, jclass cls) {{ (void)env; (void)cls; dream_thread_detach(); }}\nJNIEXPORT void JNICALL {prefix}_releaseHandle(JNIEnv *env, jclass cls, jlong handle) {{ (void)env; (void)cls; dream_release((void *)(uintptr_t)handle); }}\n");
    for f in functions {
        let params = f
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{} arg{i}", java_type(&p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        java.push_str(&format!(
            "    public static native {} call_{}({params});\n",
            java_type(&f.ret),
            f.name
        ));
        let params = f
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| format!(", {} arg{i}", jni_type(&p.ty)))
            .collect::<String>();
        let args = f
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if p.ty.kind == K::Opaque {
                    format!("(void *)(uintptr_t)arg{i}")
                } else {
                    format!("({})arg{i}", p.ty.c_type)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let ret = match f.ret.kind {
            K::Void => String::new(),
            K::Opaque => "return (jlong)(uintptr_t)".into(),
            _ => format!("return ({})", jni_type(&f.ret)),
        };
        let method = jni_name(&format!("call_{}", f.name));
        c.push_str(&format!("JNIEXPORT {} JNICALL {prefix}_{method}(JNIEnv *env, jclass cls{params}) {{ (void)env; (void)cls; {ret}{}({args}); }}\n", jni_type(&f.ret), f.name));
    }
    java.push_str("}\n");
    (java, c)
}
