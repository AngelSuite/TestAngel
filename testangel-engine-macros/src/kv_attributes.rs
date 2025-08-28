use proc_macro_error2::emit_error;
use syn::{Attribute, Expr, Ident, MacroDelimiter, Meta, Token, parse::Parse};

pub fn parse_as_kv_attr(attr_name: &'static str, attr: &Attribute) -> Option<KVAttributeContent> {
    match &attr.meta {
        Meta::List(meta) => {
            match meta.delimiter {
                MacroDelimiter::Paren(_) => (),
                MacroDelimiter::Bracket(b) => {
                    emit_error!(
                        b.span.open(),
                        format!(r#"Usage: #[{attr_name}(name = "Demo")]"#)
                    );
                }
                MacroDelimiter::Brace(b) => {
                    emit_error!(
                        b.span.open(),
                        format!(r#"Usage: #[{attr_name}(name = "Demo")]"#)
                    );
                }
            }

            if let Ok(vars) = meta.parse_args() {
                return Some(vars);
            }
        }
        err => emit_error!(
            err,
            r#"You must specify an option with the engine attribute, e.g.:\n#[engine(name = "Demo")]"#
        ),
    }
    None
}

pub struct KVAttributeContent {
    pub vars: Vec<KeyValue>,
}

impl Parse for KVAttributeContent {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut vars = vec![];
        while !input.is_empty() {
            vars.push(input.parse()?);
            if input.is_empty() {
                break;
            }
            let _: Token![,] = input.parse()?;
        }

        Ok(Self { vars })
    }
}

pub struct KeyValue {
    pub key: Ident,
    pub value: Expr,
}

impl Parse for KeyValue {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let key = input.parse()?;
        let _: Token![=] = input.parse()?;
        let value = input.parse()?;

        Ok(Self { key, value })
    }
}
