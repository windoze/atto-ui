use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Expr, Ident, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

enum ViewNode {
    Call(ViewCall),
    Container(ViewContainer),
}

struct ViewCall {
    view_type: Ident,
    args: Vec<Expr>,
    modifiers: Vec<ViewModifier>,
}

struct ViewContainer {
    container: Ident,
    children: Vec<ViewNode>,
    modifiers: Vec<ViewModifier>,
}

struct ViewModifier {
    name: Ident,
    args: Vec<Expr>,
}

impl Parse for ViewNode {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;

        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);

            let mut children = Vec::new();
            while !content.is_empty() {
                children.push(content.parse::<ViewNode>()?);
            }

            let modifiers = parse_modifiers(input)?;

            Ok(ViewNode::Container(ViewContainer {
                container: ident,
                children,
                modifiers,
            }))
        } else if input.peek(syn::token::Paren) {
            let args_content;
            syn::parenthesized!(args_content in input);

            let mut args = Vec::new();
            while !args_content.is_empty() {
                args.push(args_content.parse()?);
                if args_content.peek(Token![,]) {
                    args_content.parse::<Token![,]>()?;
                }
            }

            let modifiers = parse_modifiers(input)?;

            Ok(ViewNode::Call(ViewCall {
                view_type: ident,
                args,
                modifiers,
            }))
        } else {
            Err(input.error("expected `{ ... }` or `( ... )` after view identifier"))
        }
    }
}

pub fn view_builder_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ViewNode);
    let expr = expand_node(&input);
    TokenStream::from(quote! { { #expr } })
}

fn expand_container(container: &ViewContainer) -> TokenStream2 {
    let name = &container.container;
    let mut out = quote! { ::atto_ui::declarative::#name::new() };

    for child in &container.children {
        let child_expr = expand_node(child);
        out = quote! { #out.child(#child_expr) };
    }

    apply_modifiers(out, &container.modifiers)
}

fn expand_node(node: &ViewNode) -> TokenStream2 {
    match node {
        ViewNode::Call(call) => {
            let name = &call.view_type;
            let args = &call.args;
            let base = quote! { ::atto_ui::declarative::#name::new(#(#args),*) };
            apply_modifiers(base, &call.modifiers)
        }
        ViewNode::Container(container) => expand_container(container),
    }
}

fn apply_modifiers(mut expr: TokenStream2, modifiers: &[ViewModifier]) -> TokenStream2 {
    for m in modifiers {
        let name = &m.name;
        let args = &m.args;
        expr = quote! { #expr.#name(#(#args),*) };
    }
    expr
}

fn parse_modifiers(input: ParseStream) -> syn::Result<Vec<ViewModifier>> {
    let mut modifiers = Vec::new();
    while input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let name: Ident = input.parse()?;

        let args_content;
        syn::parenthesized!(args_content in input);

        let mut args = Vec::new();
        while !args_content.is_empty() {
            args.push(args_content.parse()?);
            if args_content.peek(Token![,]) {
                args_content.parse::<Token![,]>()?;
            }
        }

        modifiers.push(ViewModifier { name, args });
    }

    Ok(modifiers)
}
