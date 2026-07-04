//! Binding generation (plan.txt Step 5b) — the op layer (`api.json`) projected
//! into committed per-language binding glue. The thesis (translation.md): every
//! op has a SHAPE (ctor | unary | stream | manual), and the idiom for each
//! shape is written ONCE per language as a genco template — AsyncTask→Promise
//! here (napi), `detach` for PyO3, GVL-plain for Magnus — so N ops × M
//! languages collapses to shapes × languages, and `@manual` stays the escape
//! hatch for the truly bespoke.
//!
//! straitjacket-allow-file:duplication — the per-language generators are
//! DELIBERATELY parallel: the (language × shape) template grid is the design
//! (see /translation.md); truly identical pieces (trait emission) are shared.
//!
//! The generated file defines the napi surface AND the core traits; the
//! consumer hand-writes ONE `core_impl` module implementing the traits over its
//! engine. Generated code references `crate::core_impl::{GitImpl, EntlImpl}` by
//! convention.

use genco::prelude::*;

use crate::api::{ApiDoc, ApiOp, ApiType, Shape};

/// snake_case for Rust idents (`repoPath` → `repo_path`).
fn snake(s: &str) -> String {
    crate::ir::snake(s)
}

/// The caller's optional extra banner line (e.g. a lint-suppression marker) as
/// a `//! …\n` doc line, or nothing — fluessig itself never bakes tool-specific
/// markers into its output.
fn note_line(note: Option<&str>) -> String {
    note.map(|n| format!("//! {n}\n")).unwrap_or_default()
}

/// `changes` → `Changes` (stream class names, task names).
fn pascal(s: &str) -> String {
    let sn = snake(s);
    sn.split('_').map(|p| {
        let mut c = p.chars();
        c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
    }).collect()
}

/// Enums whose variants carry wire values are projected as plain strings in the
/// bindings for now (napi enums can't carry arbitrary values cleanly).
fn is_string_enum(api: &ApiDoc, name: &str) -> bool {
    // the api layer doesn't carry enum defs — the catalog does; the bindgen
    // caller passes the set of value-carrying enums via this convention:
    matches!(name, "FileStatus" | "RefKind" | "PrState" | "IssueState" | "Mergeable")
}

/// An [`ApiType`] as (rust type, ts type) strings.
fn ty(api: &ApiDoc, t: &ApiType) -> (String, String) {
    match t {
        ApiType::Scalar(s) => match s.as_str() {
            "string" => ("String".into(), "string".into()),
            "boolean" => ("bool".into(), "boolean".into()),
            "int32" => ("i32".into(), "number".into()),
            "int64" => ("i64".into(), "number".into()),
            "float64" => ("f64".into(), "number".into()),
            "Json" => ("String".into(), "string".into()), // JSON text payload
            "void" => ("()".into(), "void".into()),
            _ => ("String".into(), "string".into()),
        },
        ApiType::Model { model } => (model.clone(), model.clone()),
        ApiType::Enum { r#enum } => {
            if is_string_enum(api, r#enum) {
                ("String".into(), "string".into())
            } else {
                (r#enum.clone(), r#enum.clone())
            }
        }
        ApiType::List { list } => {
            let (r, t) = ty(api, list);
            (format!("Vec<{r}>"), format!("{t}[]"))
        }
        ApiType::Nullable { nullable } => {
            let (r, t) = ty(api, nullable);
            (format!("Option<{r}>"), format!("{t} | null"))
        }
    }
}

fn param_sig(api: &ApiDoc, op: &ApiOp) -> Vec<(String, String)> {
    op.params
        .iter()
        .map(|p| {
            let (r, _) = ty(api, &p.ty);
            let r = if p.optional == Some(true) { format!("Option<{r}>") } else { r };
            (snake(&p.name), r)
        })
        .collect()
}

/// The `<Interface>Core` traits — identical across every language's generated
/// file (each binding implements them once via `entl_core::binding_core_impls!`).
fn emit_core_traits(t: &mut rust::Tokens, api: &ApiDoc) {
    for i in &api.interfaces {
        let trait_name = format!("{}Core", i.name);
        let mut methods: Vec<rust::Tokens> = Vec::new();
        for op in &i.ops {
            if op.shape == Shape::Manual {
                continue;
            }
            let name = snake(&op.name);
            let params: Vec<String> =
                param_sig(api, op).iter().map(|(n, r)| format!("{n}: {r}")).collect();
            let ps = params.join(", ");
            let (ret, _) = ty(api, &op.returns);
            let sig = match op.shape {
                Shape::Ctor => format!("fn {name}({ps}) -> anyhow::Result<Self>"),
                Shape::Stream => {
                    format!("fn {name}(&self, {ps}) -> anyhow::Result<Box<dyn PollStream<{ret}>>>")
                }
                _ if i.ops.iter().any(|o| o.shape == Shape::Ctor) => {
                    format!("fn {name}(&self, {ps}) -> anyhow::Result<{ret}>")
                }
                _ => format!("fn {name}({ps}) -> anyhow::Result<{ret}>"),
            };
            methods.push(quote!($sig;));
        }
        quote_in! { *t =>
            $['\r']
            $(format!("/// The `{}` contract — implement over the engine in `crate::core_impl`.", i.name))
            pub trait $(&trait_name): Sized + Send + Sync + $("'static") {
                $(for m in &methods join ($['\r']) => $m)
            }
            $['\n']
        };
    }
}

/// Generate the napi (Node) binding: DTO structs, enums, core traits, per-op
/// AsyncTasks, stream classes, free functions, and the handle class.
pub fn node_binding(api: &ApiDoc, enums: &[(String, Vec<String>)], banner_note: Option<&str>) -> String {
    let mut t: rust::Tokens = quote! {
        $("// The fixed prelude — generated code uses fully-qualified paths elsewhere.")
        use std::sync::Arc;
        use std::time::Duration;
        use napi::bindgen_prelude::{AsyncTask, Result};
        use napi::{Env, Task};
        use napi_derive::napi;

        fn err(e: impl std::fmt::Display) -> napi::Error {
            napi::Error::from_reason(e.to_string())
        }

        $("/// One poll result from a core stream (the sync primitive every stream shape dresses).")
        pub enum Poll<T> {
            Item(T),
            Idle,
            Closed,
        }

        $("/// The one sync primitive: a blocking, timeout-bounded poll.")
        pub trait PollStream<T>: Send + Sync {
            fn poll(&self, timeout: Duration) -> Poll<T>;
        }
    };
    t.line();

    // ── enums (name-only variants → napi enums; wire-valued → strings) ──
    for (name, variants) in enums {
        if is_string_enum(api, name) {
            continue;
        }
        let vs: Vec<String> = variants.iter().map(|v| pascal(v)).collect();
        // napi 3 no longer auto-derives Clone/Copy on #[napi] enums; option
        // structs that carry one derive Clone, so the enum must too.
        quote_in! { t =>
            $['\n']
            #[napi]
            #[derive(Clone, Copy)]
            pub enum $name {
                $(for v in &vs join ($['\r']) => $v,)
            }
        };
    }
    t.line();

    // ── DTO structs ──
    for m in &api.models {
        let fields: Vec<rust::Tokens> = m
            .fields
            .iter()
            .map(|f| {
                let (r, _) = ty(api, &f.ty);
                let r = if f.nullable { format!("Option<{r}>") } else { r };
                let n = snake(&f.name);
                quote!(pub $n: $r,)
            })
            .collect();
        if let Some(doc) = &m.doc {
            for line in doc.lines() {
                quote_in! { t => $['\r']$(format!("/// {line}")) };
            }
        }
        quote_in! { t =>
            $['\r']
            #[napi(object)]
            #[derive(Clone)]
            pub struct $(&m.name) {
                $(for f in &fields join ($['\r']) => $f)
            }
            $['\n']
        };
    }

    emit_core_traits(&mut t, api);

    // ── per-interface surface ──
    for i in &api.interfaces {
        let has_ctor = i.ops.iter().any(|o| o.shape == Shape::Ctor);
        let trait_name = format!("{}Core", i.name);
        let impl_path = format!("crate::core_impl::{}Impl", i.name);

        // stream classes + next-tasks
        for op in i.ops.iter().filter(|o| o.shape == Shape::Stream) {
            let class = pascal(&op.name);
            let (item, ts_item) = ty(api, &op.returns);
            quote_in! { t =>
                $['\r']
                $(format!("/// Poll-based stream from `{}.{}` — call `next()` until it resolves null.", i.name, op.name))
                #[napi]
                pub struct $(&class) {
                    stream: Arc<dyn PollStream<$(&item)>>,
                }
                pub struct Next$(&class)Task {
                    stream: Arc<dyn PollStream<$(&item)>>,
                }
                impl Task for Next$(&class)Task {
                    type Output = Option<$(&item)>;
                    type JsValue = Option<$(&item)>;
                    fn compute(&mut self) -> Result<Self::Output> {
                        loop {
                            match self.stream.poll(Duration::from_millis(500)) {
                                Poll::Item(v) => return Ok(Some(v)),
                                Poll::Idle => continue,
                                Poll::Closed => return Ok(None),
                            }
                        }
                    }
                    fn resolve(&mut self, _env: Env, o: Self::Output) -> Result<Self::JsValue> {
                        Ok(o)
                    }
                }
                #[napi]
                impl $(&class) {
                    #[napi(ts_return_type = $(quoted(format!("Promise<{ts_item} | null>"))))]
                    pub fn next(&self) -> AsyncTask<Next$(&class)Task> {
                        AsyncTask::new(Next$(&class)Task { stream: self.stream.clone() })
                    }
                }
                $['\n']
            };
        }

        // unary op tasks
        for op in i.ops.iter().filter(|o| o.shape == Shape::Unary) {
            let task = format!("{}Task", pascal(&op.name));
            let name = snake(&op.name);
            let (ret, _) = ty(api, &op.returns);
            let fields: Vec<String> =
                param_sig(api, op).iter().map(|(n, r)| format!("{n}: {r},")).collect();
            let args = param_sig(api, op)
                .iter()
                .map(|(n, _)| format!("self.{n}.clone()"))
                .collect::<Vec<_>>()
                .join(", ");
            let call = if has_ctor {
                format!("self.core.{name}({args})")
            } else {
                format!("<{impl_path} as {trait_name}>::{name}({args})")
            };
            let core_field = if has_ctor { format!("core: Arc<{impl_path}>,") } else { String::new() };
            quote_in! { t =>
                $['\r']
                pub struct $(&task) {
                    $core_field
                    $(for f in &fields join ($['\r']) => $f)
                }
                impl Task for $(&task) {
                    type Output = $(&ret);
                    type JsValue = $(&ret);
                    fn compute(&mut self) -> Result<Self::Output> {
                        $call.map_err(err)
                    }
                    fn resolve(&mut self, _env: Env, o: Self::Output) -> Result<Self::JsValue> {
                        Ok(o)
                    }
                }
                $['\n']
            };
        }

        if has_ctor {
            // the handle class
            let mut methods: rust::Tokens = quote!();
            for op in &i.ops {
                let name = snake(&op.name);
                if op.shape != Shape::Manual {
                    if let Some(doc) = &op.doc {
                        for line in doc.lines() {
                            quote_in! { methods => $['\r']$(format!("/// {line}")) };
                        }
                    }
                }
                let params: Vec<String> =
                    param_sig(api, op).iter().map(|(n, r)| format!("{n}: {r}")).collect();
                let ps = params.join(", ");
                let names =
                    param_sig(api, op).iter().map(|(n, _)| n.clone()).collect::<Vec<_>>().join(", ");
                match op.shape {
                    Shape::Ctor => quote_in! { methods =>
                        $['\r']
                        #[napi(constructor)]
                        pub fn new($(&ps)) -> Result<Self> {
                            Ok(Self { core: Arc::new(<$(&impl_path) as $(&trait_name)>::$(&name)($(&names)).map_err(err)?) })
                        }
                    },
                    Shape::Unary => {
                        let task = format!("{}Task", pascal(&op.name));
                        let (_, ts_ret) = ty(api, &op.returns);
                        quote_in! { methods =>
                            $['\r']
                            #[napi(ts_return_type = $(quoted(format!("Promise<{ts_ret}>"))))]
                            pub fn $(&name)(&self, $(&ps)) -> AsyncTask<$(&task)> {
                                AsyncTask::new($(&task) { core: self.core.clone(), $(&names) })
                            }
                        }
                    }
                    Shape::Stream => {
                        let class = pascal(&op.name);
                        quote_in! { methods =>
                            $['\r']
                            #[napi]
                            pub fn $(&name)(&self, $(&ps)) -> Result<$(&class)> {
                                Ok($(&class) { stream: Arc::from(self.core.$(&name)($(&names)).map_err(err)?) })
                            }
                        }
                    }
                    Shape::Manual => quote_in! { methods =>
                        $['\r']
                        $(format!("// @manual: {} — hand-written in lib.rs.", op.name))
                    },
                }
            }
            if let Some(doc) = &i.doc {
                for line in doc.lines() {
                    quote_in! { t => $['\r']$(format!("/// {line}")) };
                }
            }
            quote_in! { t =>
                $['\r']
                #[napi]
                pub struct $(&i.name) {
                    $("// pub(crate): the @manual ops in lib.rs extend this class and need the core")
                    pub(crate) core: Arc<$(&impl_path)>,
                }

                #[napi]
                impl $(&i.name) {
                    $methods
                }
                $['\n']
            };
        } else {
            // stateless interface → free functions
            for op in &i.ops {
                let name = snake(&op.name);
                if op.shape == Shape::Manual {
                    quote_in! { t => $['\r']$(format!("// @manual: {}.{} — hand-written in lib.rs.", i.name, op.name)) };
                    continue;
                }
                let task = format!("{}Task", pascal(&op.name));
                let (_, ts_ret) = ty(api, &op.returns);
                let params: Vec<String> =
                    param_sig(api, op).iter().map(|(n, r)| format!("{n}: {r}")).collect();
                let ps = params.join(", ");
                let names =
                    param_sig(api, op).iter().map(|(n, _)| n.clone()).collect::<Vec<_>>().join(", ");
                if let Some(doc) = &op.doc {
                    for line in doc.lines() {
                        quote_in! { t => $['\r']$(format!("/// {line}")) };
                    }
                }
                quote_in! { t =>
                    $['\r']
                    #[napi(ts_return_type = $(quoted(format!("Promise<{ts_ret}>"))))]
                    pub fn $(&name)($(&ps)) -> AsyncTask<$(&task)> {
                        AsyncTask::new($(&task) { $(&names) })
                    }
                    $['\n']
                };
            }
        }
    }

    let body = t.to_file_string().expect("rust renders");
    format!(
        "//! GENERATED by fluessig bindgen from crates/fluessig/entl.tsp (api layer). Do not edit.\n//! Regenerate: `bun run gen` in crates/entl-node. Hand-written halves: crate::core_impl\n//! (the trait impls over entl-core) and the @manual ops in lib.rs.\n{}#![allow(clippy::all)]\n\n{body}",
        note_line(banner_note)
    )
}

// ═════════════════════════════════════════════════════════════════════════════
// Python (PyO3)
// ═════════════════════════════════════════════════════════════════════════════

/// One parameter of a flattened Python signature: model-typed op params are
/// expanded into their fields as keyword arguments (the pythonic idiom the
/// hand-written binding used), then reassembled into the options struct before
/// the trait call.
struct PyParam {
    name: String,
    rust_ty: String,
    /// `= None` in the signature (optional field / optional param).
    defaulted: bool,
    /// `Some((model, all_optional))` when this param came from flattening.
    group: Option<String>,
}

fn py_reserved(name: &str) -> String {
    match name {
        "from" | "import" | "class" | "def" | "return" | "pass" | "global" | "lambda" | "None"
        | "True" | "False" => format!("{name}_"),
        _ => name.to_string(),
    }
}

/// Flatten an op's params for Python: scalars pass through; model-typed params
/// expand to their fields (optional → `= None` keywords).
fn py_flatten(api: &ApiDoc, op: &ApiOp) -> Vec<PyParam> {
    let mut out = Vec::new();
    // NB: callers re-sort nothing — required params must precede defaulted ones,
    // which holds because required model fields precede optional ones in the
    // catalog and required op params precede model bags in the op surface.
    for p in &op.params {
        let model_name = match &p.ty {
            ApiType::Model { model } => Some(model.clone()),
            _ => None,
        };
        if let Some(model) = model_name {
            let m = api.models.iter().find(|m| m.name == model).expect("model in api.json");
            for f in &m.fields {
                let (r, _) = ty(api, &f.ty);
                out.push(PyParam {
                    name: py_reserved(&snake(&f.name)),
                    rust_ty: if f.nullable { format!("Option<{r}>") } else { r },
                    defaulted: f.nullable,
                    group: Some(model.clone()),
                });
            }
        } else {
            let (r, _) = ty(api, &p.ty);
            let optional = p.optional == Some(true);
            out.push(PyParam {
                name: py_reserved(&snake(&p.name)),
                rust_ty: if optional { format!("Option<{r}>") } else { r },
                defaulted: optional,
                group: None,
            });
        }
    }
    out
}

/// The `#[pyo3(signature = …)]` attribute + fn params + the body prelude that
/// reassembles flattened groups, + the argument list for the trait call.
fn py_op_pieces(api: &ApiDoc, op: &ApiOp) -> (String, String, String, String) {
    let flat = py_flatten(api, op);
    let signature = flat
        .iter()
        .map(|p| if p.defaulted { format!("{}=None", p.name) } else { p.name.clone() })
        .collect::<Vec<_>>()
        .join(", ");
    let fn_params =
        flat.iter().map(|p| format!("{}: {}", p.name, p.rust_ty)).collect::<Vec<_>>().join(", ");
    // group reassembly, in first-appearance order
    let mut prelude = String::new();
    let mut seen = Vec::new();
    for p in &flat {
        if let Some(g) = &p.group {
            if !seen.contains(g) {
                seen.push(g.clone());
            }
        }
    }
    for g in &seen {
        let fields: Vec<String> = flat
            .iter()
            .filter(|p| p.group.as_deref() == Some(g))
            .map(|p| {
                let m = api.models.iter().find(|m| &m.name == g).unwrap();
                let orig = m
                    .fields
                    .iter()
                    .find(|f| py_reserved(&snake(&f.name)) == p.name)
                    .map(|f| snake(&f.name))
                    .unwrap_or_else(|| p.name.clone());
                if orig == p.name { orig } else { format!("{orig}: {}", p.name) }
            })
            .collect();
        prelude.push_str(&format!(
            "let {}_arg = {g} {{ {} }};\n",
            snake(g),
            fields.join(", ")
        ));
    }
    // the trait-call argument list, in the op's original param order
    let args = op
        .params
        .iter()
        .map(|p| match &p.ty {
            ApiType::Model { model } => {
                if p.optional == Some(true) {
                    format!("Some({}_arg)", snake(model))
                } else {
                    format!("{}_arg", snake(model))
                }
            }
            _ => py_reserved(&snake(&p.name)),
        })
        .collect::<Vec<_>>()
        .join(", ");
    (signature, fn_params, prelude, args)
}

/// Generate the PyO3 (Python) binding: pyclass DTOs + enums, the core traits,
/// `#[pyfunction]`s with the GIL released, kwargs-flattened methods, iterator
/// stream classes, and a `register()` for the `#[pymodule]`.
pub fn python_binding(api: &ApiDoc, enums: &[(String, Vec<String>)], banner_note: Option<&str>) -> String {
    let mut t: rust::Tokens = quote! {
        use std::sync::Arc;
        use std::time::Duration;
        use pyo3::exceptions::PyRuntimeError;
        use pyo3::prelude::*;

        fn err(e: impl std::fmt::Display) -> PyErr {
            PyRuntimeError::new_err(e.to_string())
        }

        $("/// One poll result from a core stream (the sync primitive every stream shape dresses).")
        pub enum Poll<T> {
            Item(T),
            Idle,
            Closed,
        }

        $("/// The one sync primitive: a blocking, timeout-bounded poll.")
        pub trait PollStream<T>: Send + Sync {
            fn poll(&self, timeout: Duration) -> Poll<T>;
        }
    };
    t.line();

    let mut class_names: Vec<String> = Vec::new();
    let mut fn_names: Vec<String> = Vec::new();

    // ── enums ──
    for (name, variants) in enums {
        if is_string_enum(api, name) {
            continue;
        }
        class_names.push(name.clone());
        let vs: Vec<String> = variants.iter().map(|v| pascal(v)).collect();
        quote_in! { t =>
            $['\n']
            #[pyclass(eq, eq_int)]
            #[derive(Clone, Copy, PartialEq)]
            pub enum $name {
                $(for v in &vs join ($['\r']) => $v,)
            }
        };
    }
    t.line();

    // ── DTO structs (constructible from Python; fields readable via get_all) ──
    for m in &api.models {
        class_names.push(m.name.clone());
        let fields: Vec<rust::Tokens> = m
            .fields
            .iter()
            .map(|f| {
                let (r, _) = ty(api, &f.ty);
                let r = if f.nullable { format!("Option<{r}>") } else { r };
                let n = py_reserved(&snake(&f.name));
                quote!(pub $n: $r,)
            })
            .collect();
        // ctor param order: required fields first, then `=None` optionals (python rule)
        let ctor_fields: Vec<&crate::api::ApiField> = m
            .fields
            .iter()
            .filter(|f| !f.nullable)
            .chain(m.fields.iter().filter(|f| f.nullable))
            .collect();
        let sig = ctor_fields
            .iter()
            .map(|f| {
                let n = py_reserved(&snake(&f.name));
                if f.nullable { format!("{n}=None") } else { n }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let params = ctor_fields
            .iter()
            .map(|f| {
                let (r, _) = ty(api, &f.ty);
                let r = if f.nullable { format!("Option<{r}>") } else { r };
                format!("{}: {}", py_reserved(&snake(&f.name)), r)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let names = ctor_fields
            .iter()
            .map(|f| py_reserved(&snake(&f.name)))
            .collect::<Vec<_>>()
            .join(", ");
        if let Some(doc) = &m.doc {
            for line in doc.lines() {
                quote_in! { t => $['\r']$(format!("/// {line}")) };
            }
        }
        quote_in! { t =>
            $['\r']
            #[pyclass(get_all)]
            #[derive(Clone)]
            pub struct $(&m.name) {
                $(for f in &fields join ($['\r']) => $f)
            }
            #[pymethods]
            impl $(&m.name) {
                #[new]
                #[pyo3(signature = ($(&sig)))]
                fn new($(&params)) -> Self {
                    Self { $(&names) }
                }
            }
            $['\n']
        };
    }

    emit_core_traits(&mut t, api);

    // ── per-interface surface ──
    for i in &api.interfaces {
        let has_ctor = i.ops.iter().any(|o| o.shape == Shape::Ctor);
        let trait_name = format!("{}Core", i.name);
        let impl_path = format!("crate::core_impl::{}Impl", i.name);

        // stream classes: python iterators (GIL released while polling)
        for op in i.ops.iter().filter(|o| o.shape == Shape::Stream) {
            let class = pascal(&op.name);
            class_names.push(class.clone());
            let (item, _) = ty(api, &op.returns);
            quote_in! { t =>
                $['\r']
                $(format!("/// Poll-based stream from `{}.{}`, dressed as a Python iterator.", i.name, op.name))
                #[pyclass]
                pub struct $(&class) {
                    stream: Box<dyn PollStream<$(&item)>>,
                }
                #[pymethods]
                impl $(&class) {
                    fn __iter__(slf: PyRef<$("'_"), Self>) -> PyRef<$("'_"), Self> {
                        slf
                    }
                    fn __next__(&self, py: Python<$("'_")>) -> Option<$(&item)> {
                        py.detach(|| loop {
                            match self.stream.poll(Duration::from_millis(500)) {
                                Poll::Item(v) => return Some(v),
                                Poll::Idle => continue,
                                Poll::Closed => return None, $("// None => StopIteration")
                            }
                        })
                    }
                }
                $['\n']
            };
        }

        if has_ctor {
            let mut methods: rust::Tokens = quote!();
            for op in &i.ops {
                let name = snake(&op.name);
                if op.shape != Shape::Manual {
                    if let Some(doc) = &op.doc {
                        for line in doc.lines() {
                            quote_in! { methods => $['\r']$(format!("/// {line}")) };
                        }
                    }
                }
                let (signature, fn_params, prelude, args) = py_op_pieces(api, op);
                let (ret, _) = ty(api, &op.returns);
                match op.shape {
                    Shape::Ctor => quote_in! { methods =>
                        $['\r']
                        #[new]
                        fn new($(&fn_params)) -> PyResult<Self> {
                            Ok(Self { core: Arc::new(<$(&impl_path) as $(&trait_name)>::$(&name)($(&args)).map_err(err)?) })
                        }
                    },
                    Shape::Unary => quote_in! { methods =>
                        $['\r']
                        #[pyo3(signature = ($(&signature)))]
                        fn $(&name)(&self, py: Python<$("'_")>, $(&fn_params)) -> PyResult<$(&ret)> {
                            $prelude
                            let core = self.core.clone();
                            py.detach(move || core.$(&name)($(&args))).map_err(err)
                        }
                    },
                    Shape::Stream => {
                        let class = pascal(&op.name);
                        quote_in! { methods =>
                            $['\r']
                            #[pyo3(signature = ($(&signature)))]
                            fn $(&name)(&self, $(&fn_params)) -> PyResult<$(&class)> {
                                $prelude
                                Ok($(&class) { stream: self.core.$(&name)($(&args)).map_err(err)? })
                            }
                        }
                    }
                    Shape::Manual => quote_in! { methods =>
                        $['\r']
                        $(format!("// @manual: {} — hand-written in lib.rs if this binding offers it.", op.name))
                    },
                }
            }
            class_names.push(i.name.clone());
            if let Some(doc) = &i.doc {
                for line in doc.lines() {
                    quote_in! { t => $['\r']$(format!("/// {line}")) };
                }
            }
            quote_in! { t =>
                $['\r']
                #[pyclass]
                pub struct $(&i.name) {
                    $("// pub(crate): @manual ops in lib.rs extend this class and need the core")
                    pub(crate) core: Arc<$(&impl_path)>,
                }

                #[pymethods]
                impl $(&i.name) {
                    $methods
                }
                $['\n']
            };
        } else {
            for op in &i.ops {
                let name = snake(&op.name);
                if op.shape == Shape::Manual {
                    quote_in! { t => $['\r']$(format!("// @manual: {}.{} — hand-written in lib.rs if offered.", i.name, op.name)) };
                    continue;
                }
                fn_names.push(name.clone());
                let (signature, fn_params, prelude, args) = py_op_pieces(api, op);
                let (ret, _) = ty(api, &op.returns);
                if let Some(doc) = &op.doc {
                    for line in doc.lines() {
                        quote_in! { t => $['\r']$(format!("/// {line}")) };
                    }
                }
                quote_in! { t =>
                    $['\r']
                    #[pyfunction]
                    #[pyo3(signature = ($(&signature)))]
                    fn $(&name)(py: Python<$("'_")>, $(&fn_params)) -> PyResult<$(&ret)> {
                        $prelude
                        py.detach(move || <$(&impl_path) as $(&trait_name)>::$(&name)($(&args))).map_err(err)
                    }
                    $['\n']
                };
            }
        }
    }

    // ── module registration ──
    let adds: Vec<String> = class_names
        .iter()
        .map(|c| format!("m.add_class::<{c}>()?;"))
        .chain(fn_names.iter().map(|f| format!("m.add_function(wrap_pyfunction!({f}, m)?)?;")))
        .collect();
    quote_in! { t =>
        $['\r']
        $("/// Register every generated class + function on the `#[pymodule]`.")
        pub(crate) fn register(m: &Bound<$("'_"), PyModule>) -> PyResult<()> {
            $(for a in &adds join ($['\r']) => $a)
            Ok(())
        }
    };

    let body = t.to_file_string().expect("rust renders");
    format!(
        "//! GENERATED by fluessig bindgen from crates/fluessig/entl.tsp (api layer). Do not edit.\n//! Regenerate: `bun run gen` in crates/entl-node. Hand-written half: crate::core_impl.\n{}#![allow(clippy::all)]\n\n{body}",
        note_line(banner_note)
    )
}

// ═════════════════════════════════════════════════════════════════════════════
// Ruby (Magnus)
// ═════════════════════════════════════════════════════════════════════════════

/// The models an op surface RETURNS (directly, in lists, or nullable) — these
/// get Ruby classes with getters; input bags are flattened away instead.
fn output_models(api: &ApiDoc) -> Vec<String> {
    fn walk(t: &ApiType, out: &mut Vec<String>) {
        match t {
            ApiType::Model { model } => out.push(model.clone()),
            ApiType::List { list } => walk(list, out),
            ApiType::Nullable { nullable } => walk(nullable, out),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for i in &api.interfaces {
        for op in &i.ops {
            walk(&op.returns, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Ruby flattening: like Python's, plus — enum fields arrive as Strings (parsed
/// in the prelude) and input-model-typed fields (e.g. `rename: TableRename[]`)
/// are not exposed (passed as None; a kwargs follow-up).
struct RbParam {
    name: String,
    rust_ty: String,
    optional: bool,
    /// build-struct group (model name), when flattened.
    group: Option<String>,
    /// original field name inside the group.
    field: Option<String>,
    /// enum name to parse from String in the prelude.
    parse_enum: Option<String>,
}

fn rb_flatten(api: &ApiDoc, op: &ApiOp) -> (Vec<RbParam>, Vec<(String, String, Vec<String>)>) {
    // returns (params, groups: (model, var, skipped-fields))
    let mut params = Vec::new();
    let mut groups = Vec::new();
    for p in &op.params {
        let model_name = match &p.ty {
            ApiType::Model { model } => Some(model.clone()),
            _ => None,
        };
        if let Some(model) = model_name {
            let m = api.models.iter().find(|m| m.name == model).expect("model in api.json");
            let mut skipped = Vec::new();
            for f in &m.fields {
                let is_input_model = match &f.ty {
                    ApiType::Model { .. } => true,
                    ApiType::List { list } => matches!(**list, ApiType::Model { .. }),
                    _ => false,
                };
                if is_input_model {
                    skipped.push(snake(&f.name));
                    continue;
                }
                let (enum_name, base_ty) = match &f.ty {
                    ApiType::Enum { r#enum } if !is_string_enum(api, r#enum) => {
                        (Some(r#enum.clone()), "String".to_string())
                    }
                    other => (None, ty(api, other).0),
                };
                params.push(RbParam {
                    name: snake(&f.name),
                    rust_ty: if f.nullable { format!("Option<{base_ty}>") } else { base_ty },
                    optional: f.nullable,
                    group: Some(model.clone()),
                    field: Some(snake(&f.name)),
                    parse_enum: enum_name,
                });
            }
            groups.push((model.clone(), format!("{}_arg", snake(&model)), skipped));
        } else {
            let (r, _) = ty(api, &p.ty);
            let optional = p.optional == Some(true);
            params.push(RbParam {
                name: snake(&p.name),
                rust_ty: if optional { format!("Option<{r}>") } else { r },
                optional,
                group: None,
                field: None,
                parse_enum: None,
            });
        }
    }
    (params, groups)
}

/// List returns cross into Ruby as RArray (magnus has no blanket Vec<Wrapped> impl).
fn rb_is_list_return(op: &ApiOp) -> bool {
    matches!(op.returns, ApiType::List { .. })
}

struct RbPieces {
    fn_params: String,
    arity: i64,
    prelude: String,
    args: String,
    /// scan_args destructuring lines, when the op has optional params (variadic).
    scan: Option<String>,
}

fn rb_op_pieces(api: &ApiDoc, op: &ApiOp) -> RbPieces {
    let (flat, groups) = rb_flatten(api, op);
    let has_optional = flat.iter().any(|p| p.optional);
    let fn_params = if has_optional {
        "args: &[magnus::Value]".to_string()
    } else {
        flat.iter().map(|p| format!("{}: {}", p.name, p.rust_ty)).collect::<Vec<_>>().join(", ")
    };
    let arity: i64 = if has_optional { -1 } else { flat.len() as i64 };
    let scan = if has_optional {
        let req: Vec<&RbParam> = flat.iter().filter(|p| !p.optional).collect();
        let opt: Vec<&RbParam> = flat.iter().filter(|p| p.optional).collect();
        let req_tys = req.iter().map(|p| p.rust_ty.clone()).collect::<Vec<_>>().join(", ");
        let opt_tys = opt.iter().map(|p| p.rust_ty.clone()).collect::<Vec<_>>().join(", ");
        let req_names = req.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ");
        let opt_names = opt.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ");
        let req_tuple = if req.is_empty() { "()".to_string() } else { format!("({req_tys},)") };
        let mut out = format!(
            "let a = magnus::scan_args::scan_args::<{req_tuple}, ({opt_tys},), (), (), (), ()>(args)?;
"
        );
        if !req.is_empty() {
            out.push_str(&format!("let ({req_names},) = a.required;
"));
        }
        out.push_str(&format!("let ({opt_names},) = a.optional;
"));
        Some(out)
    } else {
        None
    };
    let mut prelude = String::new();
    for p in flat.iter().filter(|p| p.parse_enum.is_some()) {
        let e = p.parse_enum.as_ref().unwrap();
        prelude.push_str(&format!("let {n} = {e}::parse(&{n}).map_err(rberr)?;\n", n = p.name));
    }
    for (model, var, skipped) in &groups {
        let mut fields: Vec<String> = flat
            .iter()
            .filter(|p| p.group.as_deref() == Some(model))
            .map(|p| p.field.clone().unwrap())
            .collect();
        fields.extend(skipped.iter().map(|f| format!("{f}: None")));
        prelude.push_str(&format!("let {var} = {model} {{ {} }};\n", fields.join(", ")));
    }
    let args = op
        .params
        .iter()
        .map(|p| match &p.ty {
            ApiType::Model { model } => {
                let var = format!("{}_arg", snake(model));
                if p.optional == Some(true) { format!("Some({var})") } else { var }
            }
            _ => snake(&p.name),
        })
        .collect::<Vec<_>>()
        .join(", ");
    RbPieces { fn_params, arity, prelude, args, scan }
}

/// Generate the Magnus (Ruby) binding: plain-Rust DTOs + enums (with parse),
/// wrapped output classes with getters, GVL-plain methods with trailing
/// optionals, `.next`-nil streams, and a `register()` for `#[magnus::init]`.
pub fn ruby_binding(api: &ApiDoc, enums: &[(String, Vec<String>)], banner_note: Option<&str>) -> String {
    let outputs = output_models(api);
    let mut t: rust::Tokens = quote! {
        use std::sync::Arc;
        use std::time::Duration;
        use magnus::{function, method, prelude::*, Error, Ruby};

        fn rberr(e: impl std::fmt::Display) -> Error {
            let ruby = magnus::Ruby::get().expect("entl called outside the Ruby GVL");
            Error::new(ruby.exception_runtime_error(), e.to_string())
        }

        $("/// One poll result from a core stream (the sync primitive every stream shape dresses).")
        pub enum Poll<T> {
            Item(T),
            Idle,
            Closed,
        }

        $("/// The one sync primitive: a blocking, timeout-bounded poll.")
        pub trait PollStream<T>: Send + Sync {
            fn poll(&self, timeout: Duration) -> Poll<T>;
        }
    };
    t.line();

    // ── enums: plain Rust + parse-from-string (Ruby passes lowercase names) ──
    for (name, variants) in enums {
        if is_string_enum(api, name) {
            continue;
        }
        let vs: Vec<String> = variants.iter().map(|v| pascal(v)).collect();
        let arms: Vec<String> = variants
            .iter()
            .map(|v| format!("{:?} => Ok(Self::{}),", v.to_lowercase(), pascal(v)))
            .collect();
        let expect = variants.iter().map(|v| v.to_lowercase()).collect::<Vec<_>>().join(" | ");
        quote_in! { t =>
            $['\n']
            #[derive(Clone, Copy, PartialEq)]
            pub enum $name {
                $(for v in &vs join ($['\r']) => $v,)
            }
            impl $name {
                pub fn parse(s: &str) -> anyhow::Result<Self> {
                    match s.to_ascii_lowercase().as_str() {
                        $(for a in &arms join ($['\r']) => $a)
                        other => Err(anyhow::anyhow!($(quoted(format!("unknown {name}: {{other}} (expected {expect})")))))
                    }
                }
            }
        };
    }
    t.line();

    // ── DTO structs: plain Rust; output models get wrapped Ruby classes + getters ──
    for m in &api.models {
        let is_output = outputs.contains(&m.name);
        let fields: Vec<rust::Tokens> = m
            .fields
            .iter()
            .map(|f| {
                let (r, _) = ty(api, &f.ty);
                let r = if f.nullable { format!("Option<{r}>") } else { r };
                let n = snake(&f.name);
                quote!(pub $n: $r,)
            })
            .collect();
        if let Some(doc) = &m.doc {
            for line in doc.lines() {
                quote_in! { t => $['\r']$(format!("/// {line}")) };
            }
        }
        if is_output {
            let getters: Vec<rust::Tokens> = m
                .fields
                .iter()
                .map(|f| {
                    let (r, _) = ty(api, &f.ty);
                    let r = if f.nullable { format!("Option<{r}>") } else { r };
                    let n = snake(&f.name);
                    quote! {
                        fn get_$(&n)(&self) -> $r {
                            self.$(&n).clone()
                        }
                    }
                })
                .collect();
            quote_in! { t =>
                $['\r']
                #[magnus::wrap(class = $(quoted(format!("Entl::{}", m.name))), free_immediately, size)]
                #[derive(Clone)]
                pub struct $(&m.name) {
                    $(for f in &fields join ($['\r']) => $f)
                }
                impl $(&m.name) {
                    $(for g in &getters join ($['\r']) => $g)
                }
                $['\n']
            };
        } else {
            quote_in! { t =>
                $['\r']
                #[derive(Clone)]
                pub struct $(&m.name) {
                    $(for f in &fields join ($['\r']) => $f)
                }
                $['\n']
            };
        }
    }

    emit_core_traits(&mut t, api);

    // ── the surface ──
    let mut registrations: Vec<String> = Vec::new();
    for m in &api.models {
        if outputs.contains(&m.name) {
            registrations.push(format!(
                "let c = class.define_class({:?}, ruby.class_object())?;",
                m.name
            ));
            for f in &m.fields {
                let n = snake(&f.name);
                registrations.push(format!(
                    "c.define_method({n:?}, method!({}::get_{n}, 0))?;",
                    m.name
                ));
            }
        }
    }

    for i in &api.interfaces {
        let has_ctor = i.ops.iter().any(|o| o.shape == Shape::Ctor);
        let trait_name = format!("{}Core", i.name);
        let impl_path = format!("crate::core_impl::{}Impl", i.name);

        for op in i.ops.iter().filter(|o| o.shape == Shape::Stream) {
            let class = pascal(&op.name);
            let (item, _) = ty(api, &op.returns);
            registrations.push(format!(
                "let s = class.define_class({class:?}, ruby.class_object())?;"
            ));
            registrations.push(format!("s.define_method(\"next\", method!({class}::next, 0))?;"));
            quote_in! { t =>
                $['\r']
                $(format!("/// Poll-based stream from `{}.{}` — `.next` returns the next item or nil.", i.name, op.name))
                #[magnus::wrap(class = $(quoted(format!("Entl::{class}"))), free_immediately, size)]
                pub struct $(&class) {
                    stream: Box<dyn PollStream<$(&item)>>,
                }
                impl $(&class) {
                    fn next(&self) -> Option<$(&item)> {
                        loop {
                            match self.stream.poll(Duration::from_millis(500)) {
                                Poll::Item(v) => return Some(v),
                                Poll::Idle => continue,
                                Poll::Closed => return None, $("// nil ends iteration")
                            }
                        }
                    }
                }
                $['\n']
            };
        }

        if has_ctor {
            let mut methods: rust::Tokens = quote!();
            for op in &i.ops {
                let name = snake(&op.name);
                if op.shape != Shape::Manual {
                    if let Some(doc) = &op.doc {
                        for line in doc.lines() {
                            quote_in! { methods => $['\r']$(format!("/// {line}")) };
                        }
                    }
                }
                let p = rb_op_pieces(api, op);
                let (fn_params, arity) = (p.fn_params, p.arity);
                let prelude = format!("{}{}", p.scan.unwrap_or_default(), p.prelude);
                let args = p.args;
                let (ret, _) = ty(api, &op.returns);
                match op.shape {
                    Shape::Ctor => {
                        registrations.push(format!(
                            "class.define_singleton_method(\"new\", function!({}::new, {arity}))?;",
                            i.name
                        ));
                        quote_in! { methods =>
                            $['\r']
                            fn new($(&fn_params)) -> Result<Self, Error> {
                                $prelude
                                Ok(Self { core: Arc::new(<$(&impl_path) as $(&trait_name)>::$(&name)($(&args)).map_err(rberr)?) })
                            }
                        }
                    }
                    Shape::Unary => {
                        registrations.push(format!(
                            "class.define_method({name:?}, method!({}::{name}, {arity}))?;",
                            i.name
                        ));
                        if rb_is_list_return(op) {
                            quote_in! { methods =>
                                $['\r']
                                fn $(&name)(&self, $(&fn_params)) -> Result<magnus::RArray, Error> {
                                    $prelude
                                    let out = self.core.$(&name)($(&args)).map_err(rberr)?;
                                    let ruby = Ruby::get().map_err(|e| rberr(e))?;
                                    let ary = ruby.ary_new();
                                    for v in out {
                                        ary.push(v)?;
                                    }
                                    Ok(ary)
                                }
                            }
                        } else {
                            quote_in! { methods =>
                                $['\r']
                                fn $(&name)(&self, $(&fn_params)) -> Result<$(&ret), Error> {
                                    $prelude
                                    self.core.$(&name)($(&args)).map_err(rberr)
                                }
                            }
                        }
                    }
                    Shape::Stream => {
                        let class = pascal(&op.name);
                        registrations.push(format!(
                            "class.define_method({name:?}, method!({}::{name}, {arity}))?;",
                            i.name
                        ));
                        quote_in! { methods =>
                            $['\r']
                            fn $(&name)(&self, $(&fn_params)) -> Result<$(&class), Error> {
                                $prelude
                                Ok($(&class) { stream: self.core.$(&name)($(&args)).map_err(rberr)? })
                            }
                        }
                    }
                    Shape::Manual => quote_in! { methods =>
                        $['\r']
                        $(format!("// @manual: {} — hand-written in lib.rs if this binding offers it.", op.name))
                    },
                }
            }
            if let Some(doc) = &i.doc {
                for line in doc.lines() {
                    quote_in! { t => $['\r']$(format!("/// {line}")) };
                }
            }
            quote_in! { t =>
                $['\r']
                #[magnus::wrap(class = $(quoted(i.name.as_str())), free_immediately, size)]
                pub struct $(&i.name) {
                    core: Arc<$(&impl_path)>,
                }

                impl $(&i.name) {
                    $methods
                }
                $['\n']
            };
        } else {
            // stateless interface → singleton methods on the Entl class
            for op in &i.ops {
                let name = snake(&op.name);
                if op.shape == Shape::Manual {
                    continue;
                }
                let p = rb_op_pieces(api, op);
                let (fn_params, arity) = (p.fn_params, p.arity);
                let prelude = format!("{}{}", p.scan.unwrap_or_default(), p.prelude);
                let args = p.args;
                let (ret, _) = ty(api, &op.returns);
                registrations.push(format!(
                    "class.define_singleton_method({name:?}, function!({name}, {arity}))?;"
                ));
                if let Some(doc) = &op.doc {
                    for line in doc.lines() {
                        quote_in! { t => $['\r']$(format!("/// {line}")) };
                    }
                }
                if rb_is_list_return(op) {
                    quote_in! { t =>
                        $['\r']
                        fn $(&name)($(&fn_params)) -> Result<magnus::RArray, Error> {
                            $prelude
                            let out = <$(&impl_path) as $(&trait_name)>::$(&name)($(&args)).map_err(rberr)?;
                            let ruby = Ruby::get().map_err(|e| rberr(e))?;
                            let ary = ruby.ary_new();
                            for v in out {
                                ary.push(v)?;
                            }
                            Ok(ary)
                        }
                        $['\n']
                    };
                } else {
                    quote_in! { t =>
                        $['\r']
                        fn $(&name)($(&fn_params)) -> Result<$(&ret), Error> {
                            $prelude
                            <$(&impl_path) as $(&trait_name)>::$(&name)($(&args)).map_err(rberr)
                        }
                        $['\n']
                    };
                }
            }
        }
    }

    quote_in! { t =>
        $['\r']
        $("/// Register the Entl class + every generated method (called from #[magnus::init]).")
        pub fn register(ruby: &Ruby) -> Result<(), Error> {
            let class = ruby.define_class("Entl", ruby.class_object())?;
            $(for r in &registrations join ($['\r']) => $r)
            Ok(())
        }
    };

    let body = t.to_file_string().expect("rust renders");
    format!(
        "//! GENERATED by fluessig bindgen from crates/fluessig/entl.tsp (api layer). Do not edit.\n//! Regenerate: `bun run gen` in crates/entl-node. Hand-written half: crate::core_impl.\n//! Ruby surface notes: options bags are flattened to trailing OPTIONAL POSITIONAL args\n//! (kwargs are a follow-up); enum params are lowercase strings; input-model fields\n//! (e.g. sink's `rename`) are not exposed yet.\n{}#![allow(clippy::all)]\n\n{body}",
        note_line(banner_note)
    )
}
