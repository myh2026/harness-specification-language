//! # Codegen — 多后端源到源转译（骨架）
//!
//! 遍历 AST，按 `project {}` 投射声明把逻辑项转译为目标语言文件。
//! 后端插件按语言注册；静态资源（block/static）的 yaml/markdown/json
//! 后端在 v0.1 已完整可用，rust/python/typescript 为骨架。

pub mod contract;
pub mod python;
pub mod rust_backend;
pub mod static_res;
pub mod typescript;

use std::collections::BTreeMap;

use crate::ast::*;
use crate::sourcemap::SourceRef;

/// 单个生成文件的模型
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    /// 物理路径（相对工程根）
    pub path: String,
    /// 目标语言
    pub lang: String,
    /// 文件内容（已注入 SourceMap 围栏）
    pub content: String,
    /// 是否为不可手改的内核/胶水代码
    pub is_generated_kernel: bool,
}

/// 编译尺度（影响 graph 的架构形态）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scale {
    #[default]
    Monolith,
    Microkernel,
}

impl Scale {
    pub fn from_mode(mode: &ScaleMode) -> Self {
        match mode {
            ScaleMode::Microkernel => Scale::Microkernel,
            _ => Scale::Monolith,
        }
    }
}

/// 后端能力 trait —— 每种目标语言一个实现
pub trait CodegenBackend {
    /// 语言标识（project 投射里的 lang）
    fn lang(&self) -> &'static str;
    /// 是否为静态资源后端（block 投射目标）
    fn is_static_backend(&self) -> bool { false }
    /// 转译一个逻辑项为文件内容
    fn emit_item(&self, ctx: &CodegenContext, item: &Item) -> Result<String, String> {
        let _ = (ctx, item);
        Err(format!("backend `{}` 尚未实现逻辑项转译（P3-P7 路线图）", self.lang()))
    }
    /// 转译静态资源块（默认：原文 + 插值求值）
    fn emit_static_resource(
        &self,
        ctx: &CodegenContext,
        res: &StaticResourceDef,
    ) -> Result<String, String> {
        let mut out = String::new();
        for part in &res.content {
            match part {
                RawContentPart::Text(t) => out.push_str(t),
                RawContentPart::Interpolation { expr, .. } => {
                    // N4/N5: 编译期插值 —— 字面量/const 直接求值，运行期引用生成占位符
                    out.push_str(&ctx.eval_interp(expr));
                }
            }
        }
        Ok(out)
    }
}

/// 带 const 表的插值求值：Path 命中顶层 const 表则编译期求值（N5），否则运行期占位符
impl CodegenContext {
    pub fn eval_interp(&self, expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Literal(_) => eval_interpolation(expr),
            ExprKind::Path(p) => {
                let name = p.last().name.clone();
                if let Some(v) = self.consts.get(&name) {
                    return v.clone();
                }
                // 运行期状态引用：生成 harness 模板占位符（N5）
                format!("{{{{hsl:{name}}}}}")
            }
            ExprKind::Field { base, field } => {
                let base_name = match &base.kind {
                    ExprKind::Path(p) => p.last().name.clone(),
                    _ => return eval_interpolation(expr),
                };
                if let Some(v) = self.consts.get(&base_name) {
                    return v.clone();
                }
                let field_s = match field {
                    FieldIndex::Named(id) => id.name.clone(),
                    FieldIndex::Index(i, _) => i.to_string(),
                };
                format!("{{{{hsl:{base_name}.{field_s}}}}}")
            }
            _ => eval_interpolation(expr),
        }
    }
}

/// 插值求值：常量折叠（N5：const 上下文求值；运行期引用 → harness 占位符）
pub fn eval_interp_public(expr: &Expr) -> String {
    eval_interpolation(expr)
}

fn eval_interpolation(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => match &lit.kind {
            LiteralKind::Int { value, .. } => value.to_string(),
            LiteralKind::Float { value, .. } => value.to_string(),
            LiteralKind::Str { value, .. } => value.clone(),
            LiteralKind::Bool(b) => b.to_string(),
            LiteralKind::Char(c) => c.to_string(),
        },
        ExprKind::Path(_) | ExprKind::Field { .. } => "{{hsl:runtime}}".to_string(),
        _ => "{{hsl:unsupported}}".to_string(),
    }
}

/// const 值提取（仅字面量；非字面量初始化返回占位符）
fn const_value(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(lit) => match &lit.kind {
            LiteralKind::Int { value, .. } => value.to_string(),
            LiteralKind::Float { value, .. } => value.to_string(),
            LiteralKind::Str { value, .. } => value.clone(),
            LiteralKind::Bool(b) => b.to_string(),
            LiteralKind::Char(c) => c.to_string(),
        },
        _ => "{{hsl:nonliteral}}".to_string(),
    }
}

/// Codegen 上下文
pub struct CodegenContext {
    pub source_file: String,
    pub scale: Scale,
    /// 已注册后端
    pub backends: BTreeMap<String, Box<dyn CodegenBackend>>,
    /// 顶层 const 表（名称 → 字面量值）—— N5 编译期插值求值
    pub consts: BTreeMap<String, String>,
}

impl CodegenContext {
    pub fn new(source_file: impl Into<String>, scale: Scale, ast: &SourceFile) -> Self {
        let mut backends: BTreeMap<String, Box<dyn CodegenBackend>> = BTreeMap::new();
        backends.insert("rust".into(), Box::new(rust_backend::RustBackend));
        backends.insert("python".into(), Box::new(python::PythonBackend));
        backends.insert("typescript".into(), Box::new(typescript::TypeScriptBackend));
        backends.insert("yaml".into(), Box::new(static_res::YamlBackend));
        backends.insert("markdown".into(), Box::new(static_res::MarkdownBackend));
        backends.insert("json".into(), Box::new(static_res::JsonBackend));
        // 38 后端注册表（BNF v1.4 §5.2）：其余语言经通用契约后端投射
        // （类型/签名真实翻译 + @dhv:source-map 围栏内嵌 HSL 原文 + 未实现标记）
        for spec in crate::langs::LANGS {
            if backends.contains_key(spec.id) {
                continue;
            }
            backends.insert(spec.id.into(), Box::new(contract::ContractBackend::new(spec)));
        }
        let mut ctx = CodegenContext {
            source_file: source_file.into(),
            scale,
            backends,
            consts: BTreeMap::new(),
        };
        ctx.collect_consts(ast);
        ctx
    }

    /// 收集顶层 const（含 export const）供编译期插值求值
    fn collect_consts(&mut self, file: &SourceFile) {
        for top in &file.items {
            let TopLevel::Item(item) = top else { continue };
            let eff = match item {
                Item::Export(e) => &e.item,
                other => other,
            };
            if let Item::Const(c) = eff {
                self.consts.insert(c.name.name.clone(), const_value(&c.value));
            }
        }
    }

    /// 按 project 投射声明驱动整个生成
    pub fn emit(&self, file: &SourceFile) -> Result<Vec<GeneratedFile>, Vec<String>> {
        let mut errors = Vec::new();
        let mut outputs = Vec::new();

        let project = file.items.iter().find_map(|t| match t {
            TopLevel::Project(p) => Some(p),
            _ => None,
        });
        let Some(project) = project else {
            return Ok(outputs); // 无投射声明：纯类型库文件
        };

        for proj in &project.projections {
            let target_name = proj.target.last().name.clone();
            // 找到目标项
            let Some(top) = file.items.iter().find(|t| {
                matches!(t, TopLevel::Item(item) if item.name().map(|n| n.name == target_name).unwrap_or(false))
            }) else {
                errors.push(format!("投射目标 `{target_name}` 未找到"));
                continue;
            };
            let TopLevel::Item(item) = top else { continue };

            let Some(backend) = self.backends.get(&proj.lang.name) else {
                errors.push(format!(
                    "语言 `{}` 未注册（已注册: {}）",
                    proj.lang.name,
                    self.backends.keys().cloned().collect::<Vec<_>>().join(", ")
                ));
                continue;
            };

            // export 只影响可见性（模块导出语义），不改变投射内容 —— 解包后投射内部项
            // （v0.2.10 修复：此前 Item::Export 直接落入后端 _ 分支报"暂不支持 export"）
            let eff_item = match item {
                Item::Export(exported) => &exported.item,
                other => other,
            };
            let result = match eff_item {
                Item::StaticResource(res) => backend.emit_static_resource(self, res),
                _ => backend.emit_item(self, eff_item),
            };
            match result {
                Ok(content) => {
                    let wrapped = crate::sourcemap::wrap_editable(
                        &SourceRef::new(&self.source_file, 1, target_name.clone()),
                        &content,
                        &proj.lang.name,
                    );
                    outputs.push(GeneratedFile {
                        path: proj.path.clone(),
                        lang: proj.lang.name.clone(),
                        content: wrapped,
                        is_generated_kernel: false,
                    });
                }
                Err(e) => errors.push(e),
            }
        }
        if errors.is_empty() {
            Ok(outputs)
        } else {
            Err(errors)
        }
    }
}
