// ============================================================================
// dhv/src/codegen/contract.rs — 通用契约后端（31 种 contract 语言）
// ----------------------------------------------------------------------------
// v0.2.36 升级：从纯注释式输出升级为真实目标语言语法输出。
// 每语言按 LangSpec 的注释方言 + 类型映射 + 语法族生成可读代码。
// 类型映射对齐 dhv-ts backends/registry.ts types: TypeMap（32 语言全覆盖）。
// ============================================================================

use crate::ast::*;
use crate::langs::{LangFamily, LangSpec};
use crate::langs::type_map_for;
use super::CodegenBackend;

pub struct ContractBackend {
    spec: LangSpec,
    family: LangFamily,
    type_map: &'static [(&'static str, &'static str)],
}

impl ContractBackend {
    pub fn new(spec: &LangSpec) -> Self {
        Self {
            spec: *spec,
            family: crate::langs::family_for(spec.id),
            type_map: type_map_for(spec.id),
        }
    }

    fn comment(&self, text: &str) -> String {
        match self.spec.comment_close {
            Some(close) => format!("{} {} {}", self.spec.line_comment, text, close),
            None => format!("{} {}", self.spec.line_comment, text),
        }
    }

    fn indent(&self, text: &str, level: usize) -> String {
        let pad = "    ".repeat(level);
        text.lines()
            .map(|line| format!("{}{}", pad, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // -----------------------------------------------------------------------
    // 类型翻译：使用每语言类型映射表
    // -----------------------------------------------------------------------

    fn map_primitive(&self, name: &str) -> Option<&'static str> {
        self.type_map.iter()
            .find(|(k, _)| *k == name && !k.contains('%'))
            .map(|(_, v)| *v)
    }

    fn map_generic(&self, name: &str) -> Option<&'static str> {
        self.type_map.iter()
            .find(|(k, _)| *k == name && k.contains('%'))
            .map(|(_, v)| *v)
    }

    fn ty_text(&self, ty: &Type) -> String {
        match &ty.kind {
            TypeKind::Path(p) => {
                let base = p.path.segments.last().map(|s| s.name.as_str()).unwrap_or("?");
                let args: Vec<String> = p.generic_args.iter()
                    .map(|a| match a {
                        GenericArg::Type(t) => self.ty_text(t),
                        GenericArg::Const(_) => "_".into(),
                    })
                    .collect();
                // 泛型容器模板匹配
                if !args.is_empty() {
                    if let Some(template) = self.map_generic(base) {
                        return self.substitute_placeholders(template, &args);
                    }
                }
                // 基本类型映射
                if args.is_empty() {
                    if let Some(mapped) = self.map_primitive(base) {
                        return mapped.to_string();
                    }
                }
                // 无映射：保留 HSL 原名
                let mut text = p.path.segments.iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                if !args.is_empty() {
                    text = format!("{}<{}>", text, args.join(", "));
                }
                text
            }
            TypeKind::Ref { inner, .. } => self.ty_text(inner),
            TypeKind::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(|t| self.ty_text(t)).collect();
                format!("({})", parts.join(", "))
            }
            TypeKind::Paren(inner) => self.ty_text(inner),
            TypeKind::Array { elem, .. } => {
                match self.family {
                    LangFamily::CFamily => format!("std::array<{}>", self.ty_text(elem)),
                    _ => format!("{}[]", self.ty_text(elem)),
                }
            }
            TypeKind::Slice { 0: base, .. } => {
                match self.family {
                    LangFamily::CFamily => format!("std::span<{}>", self.ty_text(base)),
                    _ => format!("{}[]", self.ty_text(base)),
                }
            }
            TypeKind::FnPtr { params, ret } => {
                let ps: Vec<String> = params.iter().map(|t| self.ty_text(t)).collect();
                let r = ret.as_ref().map(|t| self.ty_text(t)).unwrap_or_else(|| self.void_type().to_string());
                format!("fn({}) -> {}", ps.join(", "), r)
            }
            TypeKind::Never => "!".into(),
            _ => "Any".to_string(),
        }
    }

    /// 替换泛型模板中的占位符：%K, %V, %E, %T
    fn substitute_placeholders(&self, template: &str, args: &[String]) -> String {
        let mut result = template.to_string();
 let placeholders = [("%K", 0usize), ("%V", 1), ("%E", 2), ("%T", 3)];
        for (ph, idx) in &placeholders {
            if let Some(mapped) = args.get(*idx) {
                result = result.replace(ph, mapped);
            }
        }
        // 处理仅 %T 的单参数模板
        if result.contains("%T") && !args.is_empty() {
            result = result.replace("%T", &args[0]);
        }
        result
    }

    fn void_type(&self) -> &str {
        self.map_primitive("unit").unwrap_or("void")
    }

    // -----------------------------------------------------------------------
    // 参数/签名辅助
    // -----------------------------------------------------------------------

    fn param_text(&self, p: &Param) -> Option<String> {
        match &p.kind {
            ParamKind::Self_(_) => None,
            ParamKind::Pattern(pat) => {
                let name = match &pat.kind {
                    PatternKind::Ident { name, .. } => name.name.clone(),
                    _ => "_".to_string(),
                };
                Some(format!("{}: {}", name, self.ty_text(&p.ty)))
            }
        }
    }

    fn fn_sig_text(&self, f: &FnDef) -> String {
        let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
        let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or_default();
        format!("fn {}({}) -> {}", f.name.name, params.join(", "), ret)
    }

    fn fnsig_text(&self, f: &FnSig) -> String {
        let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
        let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or_default();
        format!("fn {}({}) -> {}", f.name.name, params.join(", "), ret)
    }

    fn capitalize(s: &str) -> String {
        let mut c = s.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    }

    // -----------------------------------------------------------------------
    // 按语法族生成真实目标语言代码
    // -----------------------------------------------------------------------

    fn emit_struct(&self, s: &StructDef) -> String {
        let name = &s.name.name;
        match (self.family, &s.kind) {
            (LangFamily::OOClass, StructKind::Named(fields)) => {
                let body = fields.iter()
                    .map(|f| {
                        let n = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                        format!("    public {} {};", self.ty_text(&f.ty), n)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("public class {} {{\n{}\n}}", name, body)
            }
            (LangFamily::OOClass, StructKind::Tuple(fields)) => {
                let types: Vec<String> = fields.iter().map(|f| self.ty_text(&f.ty)).collect();
                format!("// HSL tuple struct {}({}) — 目标语言无原生元组结构体", name, types.join(", "))
            }
            (LangFamily::OOClass, StructKind::Unit) => {
                format!("public class {} {{}}", name)
            }
            (LangFamily::CFamily, StructKind::Named(fields)) => {
                let body = fields.iter()
                    .map(|f| {
                        let n = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                        format!("    {} {};", self.ty_text(&f.ty), n)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("struct {} {{\n{}\n}};", name, body)
            }
            (LangFamily::CFamily, _) => {
                format!("// HSL struct {} — 见 HSL 源码", name)
            }
            (LangFamily::Script, StructKind::Named(fields)) => {
                let attrs: Vec<String> = fields.iter()
                    .map(|f| f.name.as_ref().map(|n| format!(":{}", n.name)).unwrap_or(":_".into()))
                    .collect();
                format!("class {}\n    attr_accessor {}\nend", name, attrs.join(", "))
            }
            (LangFamily::Script, _) => {
                format!("# HSL struct {} — 见 HSL 源码", name)
            }
            (LangFamily::Functional, StructKind::Named(fields)) => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| {
                        let n = f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_");
                        format!("    {} :: {}", n, self.ty_text(&f.ty))
                    })
                    .collect();
                format!("data {} = {}\n    {{ {} }}", name, name, fs.join(",\n"))
            }
            (LangFamily::Functional, _) => {
                format!("-- HSL struct {} — 见 HSL 源码", name)
            }
        }
    }

    fn emit_enum(&self, e: &EnumDef) -> String {
        let name = &e.name.name;
        let variants: Vec<String> = e.variants.iter().map(|v| v.name.name.clone()).collect();
        match self.family {
            LangFamily::OOClass => {
                format!("public enum {} {{ {} }}", name, variants.join(", "))
            }
            LangFamily::CFamily => {
                format!("enum class {} {{ {} }};", name, variants.join(", "))
            }
            LangFamily::Script => {
                format!("# enum {} = {}", name, variants.join(" | "))
            }
            LangFamily::Functional => {
                format!("data {} = {}", name, variants.join(" | "))
            }
        }
    }

    fn emit_fn(&self, f: &FnDef) -> String {
        let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
        let ret = f.ret.as_ref()
            .map(|t| self.ty_text(t))
            .unwrap_or_else(|| self.void_type().to_string());
        let name = &f.name.name;
        match self.family {
            LangFamily::OOClass => {
                let throws = if self.spec.id == "java" || self.spec.id == "scala" { " throws Exception" } else { "" };
                if self.spec.id == "kotlin" {
                    let ret_ann = if f.ret.is_some() { format!(": {}", ret) } else { String::new() };
                    let suspend = if f.is_async { "suspend " } else { "" };
                    format!("{}fun {}({}){} {{\n    TODO(\"contract 级后端：函数体需 dhv-ts 或升级\")\n}}",
                        suspend, name, params.join(", "), ret_ann)
                } else if self.spec.id == "swift" {
                    let ret_ann = if f.ret.is_some() { format!(" -> {}", ret) } else { String::new() };
                    let async_kw = if f.is_async { " async" } else { "" };
                    format!("func {}({}){}{} {{\n    fatalError(\"contract 级后端\")\n}}",
                        name, params.join(", "), ret_ann, async_kw)
                } else if self.spec.id == "csharp" {
                    let ret_ty = if f.ret.is_some() { ret.clone() } else { "void".into() };
                    format!("public static {} {}({}){} {{\n    throw new NotImplementedException(\"contract 级后端\");\n}}",
                        ret_ty, name, params.join(", "), throws)
                } else if self.spec.id == "vb" {
                    format!("Public Shared Sub {}({})\n    ' contract 级后端\nEnd Sub", name, params.join(", "))
                } else {
                    let async_kw = if f.is_async { "java.util.concurrent.CompletableFuture<".to_string() + &ret + "> " } else { format!("{} ", ret) };
                    format!("public static {}{}({}){} {{\n    throw new UnsupportedOperationException(\"contract 级后端\");\n}}",
                        async_kw, name, params.join(", "), throws)
                }
            }
            LangFamily::CFamily => {
                format!("auto {}({}) -> {} {{\n    // contract 级：函数体需 dhv-ts 或升级为 full/logic 后端\n}}",
                    name, params.join(", "), ret)
            }
            LangFamily::Script => {
                format!("def {}({})\n    # contract 级：函数体需 dhv-ts 或升级为 full 后端\nend",
                    name, params.join(", "))
            }
            LangFamily::Functional => {
                format!("{} :: {}", name, ret)
            }
        }
    }

    fn emit_const(&self, c: &ConstDef) -> String {
        let name = &c.name.name;
        let ty = self.ty_text(&c.ty);
        match self.family {
            LangFamily::OOClass => {
                match self.spec.id {
                    "java" => format!("public static final {} {} = null;", ty, name.to_uppercase()),
                    "csharp" => format!("internal const {} {} = null;", ty, name),
                    "kotlin" => format!("const val {}: {} = TODO()", name.to_uppercase(), ty),
                    "swift" => format!("let {}: {} = TODO()", name, ty),
                    _ => format!("public static final {} {} = /* ... */;", ty, name),
                }
            }
            LangFamily::CFamily => {
                format!("constexpr {} {} = /* ... */;", ty, name)
            }
            LangFamily::Script => {
                format!("# {} : {}", name, ty)
            }
            LangFamily::Functional => {
                format!("{} :: {}", name, ty)
            }
        }
    }

    fn emit_type_alias(&self, a: &TypeAliasDef) -> String {
        let name = &a.name.name;
        let ty = self.ty_text(&a.ty);
        match self.family {
            LangFamily::OOClass => {
                match self.spec.id {
                    "csharp" => format!("using {} = {};", name, ty),
                    "kotlin" | "swift" => format!("typealias {} = {}", name, ty),
                    _ => format!("// typealias {} = {}", name, ty),
                }
            }
            LangFamily::CFamily => {
                format!("using {} = {};", name, ty)
            }
            LangFamily::Script => {
                format!("# typealias {} = {}", name, ty)
            }
            LangFamily::Functional => {
                format!("type {} = {}", name, ty)
            }
        }
    }

    fn emit_impl(&self, i: &ImplDef) -> String {
        let target = self.ty_text(&i.self_ty);
        let trait_bound = i.trait_ty.as_ref()
            .map(|t| format!(" {}", self.ty_text(t)))
            .unwrap_or_default();
        let methods: Vec<String> = i.items.iter().filter_map(|it| match it {
            ImplItem::Fn(f) => Some(self.emit_fn(f)),
            _ => None,
        }).collect();
        let body = methods.iter().map(|m| self.indent(m, 1)).collect::<Vec<_>>().join("\n\n");
        match self.family {
            LangFamily::OOClass => {
                let vis = if self.spec.id == "csharp" { "internal " } else { "" };
                format!("{}class {}Impl{} {{\n{}\n}}", vis, target, trait_bound, body)
            }
            LangFamily::CFamily => {
                format!("struct {}{} {{\n{}\n}};", target, trait_bound, body)
            }
            LangFamily::Script => {
                format!("class {}{}\n{}\nend", target, trait_bound, body)
            }
            LangFamily::Functional => {
                format!("-- impl{} {}\n{}", trait_bound, target, body)
            }
        }
    }

    fn emit_trait(&self, t: &TraitDef) -> String {
        let name = &t.name.name;
        match self.family {
            LangFamily::OOClass => {
                match self.spec.id {
                    "java" => {
                        let methods = t.items.iter().filter_map(|ti| match ti {
                            TraitItem::FnSig(f) => {
                                let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or("Object".into());
                                let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
                                Some(format!("    {} {}({}) throws Exception;", ret, f.name.name, params.join(", ")))
                            }
                            TraitItem::Fn(f) => {
                                let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or("Object".into());
                                let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
                                Some(format!("    {} {}({}) throws Exception;", ret, f.name.name, params.join(", ")))
                            }
                            _ => None,
                        }).collect::<Vec<_>>().join("\n");
                        format!("public interface {} {{\n{}\n}}", name, methods)
                    }
                    "csharp" => {
                        let methods = t.items.iter().filter_map(|ti| match ti {
                            TraitItem::FnSig(f) => {
                                let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or("void".into());
                                let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
                                Some(format!("    {} {}({});", ret, Self::capitalize(&f.name.name), params.join(", ")))
                            }
                            TraitItem::Fn(f) => {
                                let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or("void".into());
                                let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
                                Some(format!("    {} {}({});", ret, Self::capitalize(&f.name.name), params.join(", ")))
                            }
                            _ => None,
                        }).collect::<Vec<_>>().join("\n");
                        format!("internal interface {} {{\n{}\n}}", name, methods)
                    }
                    "kotlin" => {
                        let methods = t.items.iter().filter_map(|ti| match ti {
                            TraitItem::FnSig(f) => {
                                let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or("Any".into());
                                let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
                                let async_kw = if f.is_async { "suspend " } else { "" };
                                let ret_ann = if f.ret.is_some() { format!(": {}", ret) } else { String::new() };
                                Some(format!("    {}fun {}({}){}", async_kw, f.name.name, params.join(", "), ret_ann))
                            }
                            TraitItem::Fn(f) => {
                                let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or("Any".into());
                                let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
                                let async_kw = if f.is_async { "suspend " } else { "" };
                                let ret_ann = if f.ret.is_some() { format!(": {}", ret) } else { String::new() };
                                Some(format!("    {}fun {}({}){}", async_kw, f.name.name, params.join(", "), ret_ann))
                            }
                            _ => None,
                        }).collect::<Vec<_>>().join("\n");
                        format!("interface {} {{\n{}\n}}", name, methods)
                    }
                    "swift" => {
                        let methods = t.items.iter().filter_map(|ti| match ti {
                            TraitItem::FnSig(f) => {
                                let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or("Any".into());
                                let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
                                let async_kw = if f.is_async { " async" } else { "" };
                                let ret_ann = if f.ret.is_some() { format!(" -> {}", ret) } else { String::new() };
                                Some(format!("    func {}({}){}{}", f.name.name, params.join(", "), ret_ann, async_kw))
                            }
                            TraitItem::Fn(f) => {
                                let ret = f.ret.as_ref().map(|t| self.ty_text(t)).unwrap_or("Any".into());
                                let params: Vec<String> = f.params.iter().filter_map(|p| self.param_text(p)).collect();
                                let async_kw = if f.is_async { " async" } else { "" };
                                let ret_ann = if f.ret.is_some() { format!(" -> {}", ret) } else { String::new() };
                                Some(format!("    func {}({}){}{}", f.name.name, params.join(", "), ret_ann, async_kw))
                            }
                            _ => None,
                        }).collect::<Vec<_>>().join("\n");
                        format!("protocol {} {{\n{}\n}}", name, methods)
                    }
                    _ => {
                        let body = t.items.iter().filter_map(|ti| match ti {
                            TraitItem::FnSig(f) => Some(self.indent(&self.fnsig_text(f), 1)),
                            TraitItem::Fn(f) => Some(self.indent(&self.fn_sig_text(f), 1)),
                            _ => None,
                        }).collect::<Vec<_>>().join("\n");
                        format!("public interface {} {{\n{}\n}}", name, body)
                    }
                }
            }
            LangFamily::CFamily => {
                let body = t.items.iter().filter_map(|ti| match ti {
                    TraitItem::FnSig(f) => Some(self.indent(&self.fnsig_text(f), 1)),
                    TraitItem::Fn(f) => Some(self.indent(&self.fn_sig_text(f), 1)),
                    _ => None,
                }).collect::<Vec<_>>().join("\n");
                format!("struct {}_trait {{\n{}\n}};", name, body)
            }
            LangFamily::Script => {
                let body = t.items.iter().filter_map(|ti| match ti {
                    TraitItem::FnSig(f) => Some(format!("    # {}", self.fnsig_text(f))),
                    TraitItem::Fn(f) => Some(format!("    # {}", self.fn_sig_text(f))),
                    _ => None,
                }).collect::<Vec<_>>().join("\n");
                format!("module {}\n{}\nend", name, body)
            }
            LangFamily::Functional => {
                let body = t.items.iter().filter_map(|ti| match ti {
                    TraitItem::FnSig(f) => Some(format!("    -- {}", self.fnsig_text(f))),
                    TraitItem::Fn(f) => Some(format!("    -- {}", self.fn_sig_text(f))),
                    _ => None,
                }).collect::<Vec<_>>().join("\n");
                format!("class {} where\n{}", name, body)
            }
        }
    }

    fn emit_graph(&self, g: &GraphDef) -> String {
        let name = &g.name.name;
        let stmt_count = g.body.len();
        match self.family {
            LangFamily::OOClass | LangFamily::CFamily => {
                format!("// AgentLoop topology: {} ({} graph statements) — 见 HSL 源码",
                    name, stmt_count)
            }
            LangFamily::Script => {
                format!("# AgentLoop topology: {} ({} stmts)", name, stmt_count)
            }
            LangFamily::Functional => {
                format!("-- AgentLoop topology: {} ({} stmts)", name, stmt_count)
            }
        }
    }

    fn emit_static_resource(&self, r: &StaticResourceDef) -> String {
        let name = &r.name.name;
        let len = r.content.len();
        format!("// static {} / block {}（{} 部分）", name, name, len)
    }

    fn item_kind_name(item: &Item) -> &'static str {
        match item {
            Item::Struct(_) => "struct",
            Item::Enum(_) => "enum",
            Item::Trait(_) => "trait",
            Item::Impl(_) => "impl",
            Item::Fn(_) => "fn",
            Item::Const(_) => "const",
            Item::TypeAlias(_) => "type alias",
            Item::Graph(_) => "graph",
            Item::StaticResource(_) => "static/block",
            Item::Import(_) => "import",
            Item::Export(_) => "export",
            Item::MacroRules(_) => "macro_rules",
            Item::MacroCall { .. } => "macro call",
        }
    }
}

impl CodegenBackend for ContractBackend {
    fn lang(&self) -> &'static str {
        self.spec.id
    }

    fn emit_item(&self, _ctx: &super::CodegenContext, item: &Item) -> Result<String, String> {
        let code = match item {
            Item::Struct(s) => self.emit_struct(s),
            Item::Enum(e) => self.emit_enum(e),
            Item::Trait(t) => self.emit_trait(t),
            Item::Fn(f) => self.emit_fn(f),
            Item::Impl(i) => self.emit_impl(i),
            Item::Const(c) => self.emit_const(c),
            Item::TypeAlias(a) => self.emit_type_alias(a),
            Item::Graph(g) => self.emit_graph(g),
            Item::StaticResource(r) => self.emit_static_resource(r),
            _ => return Err(format!(
                "contract 后端（{}）暂不支持 {:?} 项的投射",
                self.spec.id,
                Self::item_kind_name(item)
            )),
        };
        let mut out: Vec<String> = Vec::new();
        out.push(self.comment(&format!(
            "DHV v0.2 contract（{} · {} 语法族）",
            self.spec.name,
            match self.family {
                LangFamily::OOClass => "OOP class",
                LangFamily::CFamily => "C-family",
                LangFamily::Script => "script",
                LangFamily::Functional => "functional",
            }
        )));
        out.push(code);
        out.push(self.comment(
            "@dhv:hsl-mirror — 完整 HSL 源镜像见本文件外层 @dhv:source-map 围栏",
        ));
        Ok(out.join("\n"))
    }
}
