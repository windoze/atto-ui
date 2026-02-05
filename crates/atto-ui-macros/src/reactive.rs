use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type, parse_macro_input};

pub fn derive_reactive_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return quote! {
                    compile_error!("Reactive only supports structs with named fields");
                }
                .into();
            }
        },
        _ => {
            return quote! {
                compile_error!("Reactive only supports structs");
            }
            .into();
        }
    };

    let mut reactive_fields: Vec<(&syn::Ident, Type)> = Vec::new();

    for field in fields {
        let Some(field_name) = field.ident.as_ref() else {
            continue;
        };
        let is_reactive = field
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("reactive"));
        if !is_reactive {
            continue;
        }

        let Some(inner_ty) = property_inner_type(&field.ty) else {
            let msg = format!(
                "field `{}` is marked #[reactive] but is not `Property<T>`",
                field_name
            );
            return quote! { compile_error!(#msg); }.into();
        };

        reactive_fields.push((field_name, inner_ty));
    }

    let getters = reactive_fields.iter().map(|(field_name, inner_ty)| {
        let fn_name = format_ident!("get_{field_name}");
        quote! {
            pub fn #fn_name(&self) -> #inner_ty {
                self.#field_name.get()
            }
        }
    });

    let setters = reactive_fields.iter().map(|(field_name, inner_ty)| {
        let fn_name = format_ident!("set_{field_name}");
        quote! {
            pub fn #fn_name(&self, value: #inner_ty) {
                self.#field_name.set(value);
            }
        }
    });

    let bindings = reactive_fields.iter().map(|(field_name, inner_ty)| {
        let fn_name = format_ident!("{field_name}_binding");
        quote! {
            pub fn #fn_name(&self) -> ::atto_ui::reactive::Binding<#inner_ty> {
                self.#field_name.binding()
            }
        }
    });

    let dirty_expr = if reactive_fields.is_empty() {
        quote! { false }
    } else {
        let checks = reactive_fields.iter().map(|(field_name, _)| {
            quote! { self.#field_name.is_dirty() }
        });
        quote! { #(#checks)||* }
    };

    let mark_clean_body = if reactive_fields.is_empty() {
        quote! {}
    } else {
        let calls = reactive_fields.iter().map(|(field_name, _)| {
            quote! { self.#field_name.mark_clean(); }
        });
        quote! { #(#calls)* }
    };

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            #(#getters)*
            #(#setters)*
            #(#bindings)*

            pub fn is_dirty(&self) -> bool {
                #dirty_expr
            }

            pub fn mark_clean(&self) {
                #mark_clean_body
            }
        }
    }
    .into()
}

fn property_inner_type(ty: &Type) -> Option<Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Property" {
        return None;
    }

    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let first = args.args.first()?;
    let GenericArgument::Type(inner) = first else {
        return None;
    };
    Some(inner.clone())
}
