use heck::{ToKebabCase, ToTitleCase, ToUpperCamelCase};
use proc_macro_error2::{abort, abort_call_site, emit_call_site_error, emit_error};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    Attribute, Block, Expr, Ident, Lit, Meta, Token, TypePath, braced, parenthesized, parse::Parse,
    parse_str, punctuated::Punctuated, token::Paren,
};

use crate::kv_attributes::parse_as_kv_attr;

#[derive(Debug)]
pub enum TestangelType {
    String,
    F64,
    I32,
    Bool,
}

impl TestangelType {
    pub fn value_fn(&self) -> Ident {
        match self {
            Self::String => Ident::new("value_string", Span::call_site()),
            Self::F64 => Ident::new("value_f64", Span::call_site()),
            Self::I32 => Ident::new("value_i32", Span::call_site()),
            Self::Bool => Ident::new("value_bool", Span::call_site()),
        }
    }

    pub fn type_ident(&self) -> Ident {
        match self {
            Self::String => Ident::new("String", Span::call_site()),
            Self::F64 => Ident::new("Decimal", Span::call_site()),
            Self::I32 => Ident::new("Integer", Span::call_site()),
            Self::Bool => Ident::new("Boolean", Span::call_site()),
        }
    }
}

impl Parse for TestangelType {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        match ident.to_string().as_str() {
            "String" => Ok(TestangelType::String),
            "i32" => Ok(TestangelType::I32),
            "f64" => Ok(TestangelType::F64),
            "bool" => Ok(TestangelType::Bool),
            x => abort!(ident, format!("{x} is not supported as a type")),
        }
    }
}

pub struct InstructionsImpl {
    pub self_ty: TypePath,
    pub items: Vec<InstructionFn>,
}

impl InstructionsImpl {
    pub fn to_tokens(&self, struct_ident: &Ident) -> TokenStream2 {
        if self
            .self_ty
            .path
            .get_ident()
            .is_none_or(|id| *id != *struct_ident)
        {
            abort!(
                self.self_ty.path,
                "The `impl` block needs to be for the respective `struct`."
            );
        }

        let engine_name = struct_ident.to_string();

        let instrucs = self
            .items
            .iter()
            .map(|item| {
                let instruc = item.to_tokens(&engine_name);
                quote!(#instruc)
            })
            .collect::<Vec<_>>();

        quote! {
            #(#instrucs)*
        }
    }
}

impl Parse for InstructionsImpl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let _: Token![impl] = input.parse()?;
        let self_ty = input.parse()?;
        let content;
        let _ = braced!(content in input);
        let mut items = vec![];
        while !content.is_empty() {
            items.push(content.parse()?);
        }
        Ok(Self { self_ty, items })
    }
}

pub struct InstructionFn {
    pub attrs: Vec<Attribute>,
    pub sig: InstructionSignature,
    pub block: Block,
}

impl InstructionFn {
    #[allow(clippy::too_many_lines)]
    pub fn to_tokens(&self, engine_name: &str) -> TokenStream2 {
        let ident = &self.sig.ident;

        let mut id = parse_str(&format!(
            r#""{}-{}""#,
            engine_name.to_kebab_case(),
            ident.to_string().to_kebab_case()
        ))
        .unwrap();
        let mut lua_name =
            parse_str(&format!(r#""{}""#, ident.to_string().to_upper_camel_case())).unwrap();
        let mut friendly_name =
            parse_str(&format!(r#""{}""#, ident.to_string().to_title_case())).unwrap();
        let mut description = String::new();
        let mut flags = parse_str("InstructionFlags::NONE").unwrap();

        // Validate struct attributes
        for attr in &self.attrs {
            if attr.path().is_ident("instruction") {
                if let Some(vars) = parse_as_kv_attr("instruction", attr) {
                    for var in vars.vars {
                        match var.key.to_string().as_str() {
                            "id" => id = var.value,
                            "name" => friendly_name = var.value,
                            "lua_name" => lua_name = var.value,
                            "flags" => flags = var.value,
                            _ => emit_error!(
                                var.key.span(),
                                "Invalid key, expecting 'id', 'name', 'lua_name' or 'flags'."
                            ),
                        }
                    }
                }
            } else if attr.path().is_ident("doc")
                && let Meta::NameValue(name_val) = &attr.meta
                    && let Expr::Lit(lit) = &name_val.value
                        && let Lit::Str(s) = &lit.lit {
                            let doc_line = s.value();
                            let doc_line = doc_line.trim();
                            if doc_line.is_empty() {
                                description.push('\n');
                            } else {
                                if description
                                    .chars()
                                    .last()
                                    .is_some_and(|c| !c.is_whitespace())
                                {
                                    description.push(' ');
                                }

                                description.push_str(doc_line);
                            }
                        }
        }

        let mut param_expansions = vec![];
        let mut param_registrations = vec![];
        for i in &self.sig.inputs {
            let InstructionParameter {
                id,
                friendly_name,
                ident,
                ty,
            } = i;
            let value_fn = ty.value_fn();
            let ty_ident = ty.type_ident();

            param_expansions.push(quote!(let #ident = _params[#id].#value_fn();));
            param_registrations.push(quote!(i = i.with_parameter(#id, #friendly_name, ::testangel_engine::ParameterKind::#ty_ident);));
        }
        let body = &self.block;

        let mut outputs = vec![];
        let mut output_registrations = vec![];
        let mut output_transformers = vec![];
        for (i, o) in self.sig.output.iter().enumerate() {
            let ident = Ident::new(&format!("o{i}"), Span::call_site());
            outputs.push(ident.clone());

            let id = &o.id;
            let friendly_name = &o.name;
            let ty_ident = o.ty.type_ident();
            output_registrations.push(quote! {
                i = i.with_output(#id, #friendly_name, ::testangel_engine::ParameterKind::#ty_ident);
            });

            output_transformers.push(quote! {
                _output.insert(
                    #id.to_string(),
                    ::testangel_engine::ParameterValue::#ty_ident(#ident),
                );
            });
        }

        let output_let = if self.sig.output.is_empty() {
            quote!(#body)
        } else if self.sig.results_in_tuple {
            quote!(let (#(#outputs,)*) = #body;)
        } else {
            let output = outputs.first().unwrap();
            quote!(let #output = #body;)
        };

        quote! {
            let mut i = ::testangel_engine::Instruction::new(
                #id, #lua_name, #friendly_name, #description, #flags,
            );
            #(#param_registrations)*
            #(#output_registrations)*
            engine = engine.with_instruction(i, |state, _params, dry_run, _output, evidence| {
                #(#param_expansions)*
                #output_let
                #(#output_transformers)*
                Ok(())
            });
        }
    }
}

impl Parse for InstructionFn {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let sig = input.parse()?;
        let block = input.parse()?;
        Ok(Self { attrs, sig, block })
    }
}

#[derive(Debug)]
pub struct InstructionSignature {
    pub ident: Ident,
    pub inputs: Punctuated<InstructionParameter, Token![,]>,
    pub results_in_tuple: bool,
    pub output: Punctuated<InstructionReturn, Token![,]>,
}

impl Parse for InstructionSignature {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let _: Token![fn] = input.parse()?;
        let ident = input.parse()?;
        let content;
        let _ = parenthesized!(content in input);
        let inputs = content.parse_terminated(InstructionParameter::parse, Token![,])?;

        let mut results_in_tuple = true;
        let output = if input.peek(Token![->]) {
            let _: Token![->] = input.parse()?;
            if input.peek(Paren) {
                let content;
                let _ = parenthesized!(content in input);
                content.parse_terminated(InstructionReturn::parse, Token![,])?
            } else {
                let mut p = Punctuated::new();
                p.push(input.parse()?);
                results_in_tuple = false;
                p
            }
        } else {
            Punctuated::new()
        };
        Ok(Self {
            ident,
            inputs,
            results_in_tuple,
            output,
        })
    }
}

#[derive(Debug)]
pub struct InstructionParameter {
    id: Expr,
    friendly_name: Expr,
    ident: Ident,
    ty: TestangelType,
}

impl Parse for InstructionParameter {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        if attrs.len() > 1 {
            emit_call_site_error!("There must be zero or one attribute on parameters");
        }

        let mut id = None;
        let mut friendly_name = None;

        for attr in attrs {
            if attr.path().is_ident("arg")
                && let Some(vars) = parse_as_kv_attr("arg", &attr) {
                    for var in vars.vars {
                        match var.key.to_string().as_str() {
                            "id" => id = Some(var.value),
                            "name" => friendly_name = Some(var.value),
                            _ => emit_error!(
                                var.key.span(),
                                "Invalid key, expecting 'id' or 'name'."
                            ),
                        }
                    }
                }
        }

        let ident: Ident = input.parse()?;
        let _: Token![:] = input.parse()?;
        let ty = input.parse()?;

        if ident.to_string().as_str() == "state" {
            emit_error!(
                ident,
                "Using a parameter with the name `state` will make it impossible to change the engine's state variable."
            );
        } else if ident.to_string().as_str() == "evidence" {
            emit_error!(
                ident,
                "Using a parameter with the name `evidence` will make it impossible to change the engine's evidence variable."
            );
        }

        Ok(Self {
            id: id.unwrap_or(
                parse_str(&format!(r#""{}""#, ident.to_string().to_kebab_case())).unwrap(),
            ),
            friendly_name: friendly_name.unwrap_or(
                parse_str(&format!(r#""{}""#, ident.to_string().to_title_case())).unwrap(),
            ),
            ident,
            ty,
        })
    }
}

#[derive(Debug)]
pub struct InstructionReturn {
    id: Expr,
    name: Expr,
    ty: TestangelType,
}

impl Parse for InstructionReturn {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        if attrs.len() > 1 {
            abort_call_site!("Only zero or one attribute is supported on return types");
        }

        let mut id = None;
        let mut friendly_name = None;

        if let Some(attr) = attrs.first()
            && attr.path().is_ident("output")
                && let Some(vars) = parse_as_kv_attr("output", attr) {
                    for var in vars.vars {
                        match var.key.to_string().as_str() {
                            "id" => id = Some(var.value),
                            "name" => friendly_name = Some(var.value),
                            _ => emit_error!(
                                var.key.span(),
                                "Invalid key, expecting 'id' or 'name'."
                            ),
                        }
                    }
                }

        if id.is_none() {
            abort_call_site!("Return types require an id");
        }
        if friendly_name.is_none() {
            abort_call_site!("Return types require a name");
        }

        let ty = input.parse()?;
        Ok(Self {
            id: id.unwrap(),
            name: friendly_name.unwrap(),
            ty,
        })
    }
}
