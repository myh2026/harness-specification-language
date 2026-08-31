// ============================================================================
// dhv/src/codegen/contract.rs — 通用契约后端（26 种 contract 语言）
// ----------------------------------------------------------------------------
// 为注册表中无专属后端的语言生成「类型契约」投射（与 dhv-ts backends/decls.ts
// 的契约模式同构）：struct/enum/trait/fn/graph 的目标语言注释契约 +
// @dhv:source-map 围栏语义（外层 wrap_editable 由 CodegenContext 注入）。
// 每语言按 LangSpec 的注释方言输出，保证目标文件语法合法。
// ============================================================================

use crate::ast::*;
use crate::langs::LangSpec;
use super::CodegenBackend;

pub struct ContractBackend {
    spec: LangSpec,
}

impl ContractBackend {
    pub fn new(spec: &LangSpec) -> Self {
        Self { spec: *spec }
    }

    fn comment(&self, text: &str) -> String {
        match self.spec.comment_close {
            Some(close) => format!("{} {} {}", self.spec.line_comment, text, close),
            None => format!("{} {}", self.spec.line_comment, text),
        }
    }

    fn ty_text(&self, ty: &Type) -> String {
        match &ty.kind {
            TypeKind::Path(p) => {
                let mut text = p
                    .path
                    .segments
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                if !p.generic_args.is_empty() {
                    let args: Vec<String> = p
                        .generic_args
                        .iter()
                        .map(|a| match a {
                            GenericArg::Type(t) => self.ty_text(t),
                            GenericArg::Const(_) => "_".to_string(),
                        })
                        .collect();
                    text = format!("{text}<{}>", args.join(", "));
                }
                text
            }
            TypeKind::Ref { inner, .. } => self.ty_text(inner),
            TypeKind::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(|t| self.ty_text(t)).collect();
                format!("({})", parts.join(", "))
            }
            TypeKind::Paren(inner) => self.ty_text(inner),
            _ => "Any".to_string(),
        }
    }

    fn param_text(&self, p: &Param) -> Option<String> {
        match &p.kind {
            ParamKind::Self_(_) => None, // 方法接收者：签名契约中省略
            ParamKind::Pattern(pat) => {
                let name = match &pat.kind {
                    PatternKind::Ident { name, .. } => name.name.clone(),
                    _ => "_".to_string(),
                };
                Some(format!("{}: {}", name, self.ty_text(&p.ty)))
            }
        }
    }

    fn item_signature(&self, item: &Item) -> Option<String> {
        Some(match item {
            Item::Struct(s) => {
                let fields = match &s.kind {
                    StructKind::Named(fields) => fields
                        .iter()
                        .map(|f| {
                            format!(
                                "{}: {}",
                                f.name.as_ref().map(|n| n.name.as_str()).unwrap_or("_"),
                                self.ty_text(&f.ty)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    StructKind::Tuple(fields) => fields
                        .iter()
                        .map(|f| self.ty_text(&f.ty))
                        .collect::<Vec<_>>()
                        .join(", "),
                    StructKind::Unit => String::new(),
                };
                format!("struct {} {{ {} }}", s.name.name, fields)
            }
            Item::Enum(e) => {
                let variants: Vec<String> = e
                    .variants
                    .iter()
                    .map(|v| v.name.name.clone())
                    .collect();
                format!("enum {} {{ {} }}", e.name.name, variants.join(" | "))
            }
            Item::Trait(t) => {
                let methods: Vec<String> = t
                    .items
                    .iter()
                    .filter_map(|ti| match ti {
                        TraitItem::FnSig(f) => Some(f.name.name.clone()),
                        TraitItem::Fn(f) => Some(f.name.name.clone()),
                        _ => None,
                    })
                    .collect();
                format!("trait {} {{ {} }}", t.name.name, methods.join("; "))
            }
            Item::Fn(f) => {
                let params: Vec<String> =
                    f.params.iter().filter_map(|p| self.param_text(p)).collect();
                let ret = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", self.ty_text(t)))
                    .unwrap_or_default();
                format!(
                    "{}fn {}({}){}",
                    if f.is_async { "async " } else { "" },
                    f.name.name,
                    params.join(", "),
                    ret
                )
            }
            Item::Graph(g) => format!("graph {}（AgentLoop 拓扑）", g.name.name),
            _ => return None,
        })
    }
}

impl CodegenBackend for ContractBackend {
    fn lang(&self) -> &'static str {
        self.spec.id
    }

    fn emit_item(&self, _ctx: &super::CodegenContext, item: &Item) -> Result<String, String> {
        let Some(sig) = self.item_signature(item) else {
            return Err(format!(
                "contract 后端（{}）暂不支持 {:?} 项的契约投射",
                self.spec.id,
                std::mem::discriminant(item)
            ));
        };
        let mut out: Vec<String> = Vec::new();
        out.push(self.comment(&format!(
            "DHV v0.2 contract 投射（{} 后端 · {} 能力级）",
            self.spec.name,
            match self.spec.capability {
                crate::langs::Capability::Full => "full",
                crate::langs::Capability::Logic => "logic",
                crate::langs::Capability::Contract => "contract",
                crate::langs::Capability::Raw => "raw",
            }
        )));
        out.push(self.comment(&format!("HSL 契约：{sig}")));
        out.push(self.comment(
            "@dhv:hsl-mirror — 完整 HSL 源镜像见本文件外层 @dhv:source-map 围栏（编辑后 dhv sync 回写）",
        ));
        out.push(self.comment(&format!(
            "运行请使用 dhv-ts 解释器或将本目标升级为 full/logic 后端（{} 属 contract 级）",
            self.spec.id
        )));
        Ok(out.join("\n"))
    }
}
