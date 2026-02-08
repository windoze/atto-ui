use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::parse::Parser;
use syn::{
    Data, DeriveInput, Expr, Fields, GenericArgument, Ident, ItemImpl, Meta, MetaList,
    MetaNameValue, PathArguments, Type, parse_macro_input,
};

#[derive(Clone)]
enum FieldKind {
    Binding { inner: Type },
    OptionBinding { inner: Type },
}

pub fn derive_component_properties_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let mut fields: Vec<&syn::Field> = Vec::new();
    match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => {
                fields.extend(named.named.iter());
            }
            Fields::Unit => {}
            Fields::Unnamed(_) => {
                return quote! {
                    compile_error!("ComponentProperties only supports structs with named fields");
                }
                .into();
            }
        },
        _ => {
            return quote! {
                compile_error!("ComponentProperties only supports structs");
            }
            .into();
        }
    };

    #[derive(Clone)]
    struct FieldInfo {
        ident: Ident,
        prop_name: String,
        kind: FieldKind,
        value_type: TokenStream2,
    }

    let mut binding_fields: Vec<FieldInfo> = Vec::new();
    let mut delegate_fields: Vec<(Ident, bool, Type)> = Vec::new();

    for field in fields {
        let Some(field_name) = field.ident.clone() else {
            continue;
        };

        let mut skip = false;
        let mut rename: Option<String> = None;
        let mut delegate = false;
        let mut include = false;

        for attr in &field.attrs {
            if !attr.path().is_ident("component") {
                continue;
            }

            match &attr.meta {
                Meta::Path(_) => {
                    // #[component] - no-op
                }
                Meta::NameValue(MetaNameValue { path, value, .. }) => {
                    if path.is_ident("rename")
                        && let Some(value) = expr_lit_str(value)
                    {
                        rename = Some(value);
                    }
                }
                Meta::List(MetaList { tokens, .. }) => {
                    let nested =
                        match syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated
                            .parse2(tokens.clone())
                        {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                    for meta in nested {
                        match meta {
                            Meta::Path(p) => {
                                if p.is_ident("skip") {
                                    skip = true;
                                } else if p.is_ident("delegate") {
                                    delegate = true;
                                } else if p.is_ident("include") {
                                    include = true;
                                }
                            }
                            Meta::NameValue(MetaNameValue { path, value, .. }) => {
                                if path.is_ident("rename")
                                    && let Some(value) = expr_lit_str(&value)
                                {
                                    rename = Some(value);
                                }
                            }
                            Meta::List(_) => {}
                        }
                    }
                }
            }
        }

        if delegate {
            let rwlock_like = is_rwlock_like(&field.ty);
            delegate_fields.push((field_name.clone(), rwlock_like, field.ty.clone()));
            if skip {
                continue;
            }
        }

        if skip {
            continue;
        }

        let Some(kind) = binding_kind(&field.ty) else {
            continue;
        };

        let inner_ty = match &kind {
            FieldKind::Binding { inner } => inner.clone(),
            FieldKind::OptionBinding { inner } => inner.clone(),
        };

        let value_type = match value_type_for(&inner_ty) {
            Some(v) => v,
            None => {
                if !include {
                    continue;
                }
                quote! { ::atto_ui::ValueType::Unknown }
            }
        };

        let prop_name = rename.unwrap_or_else(|| field_name.to_string());
        binding_fields.push(FieldInfo {
            ident: field_name,
            prop_name,
            kind,
            value_type,
        });
    }

    let prop_names = binding_fields.iter().map(|field| {
        let name = &field.prop_name;
        quote! { #name }
    });

    let prop_types = binding_fields.iter().map(|field| {
        let name = &field.prop_name;
        let ty = &field.value_type;
        quote! {
            ::atto_ui::PropertyMeta::new(#name, #ty)
        }
    });

    let get_match_arms = binding_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.prop_name;
        let inner = match &field.kind {
            FieldKind::Binding { inner } => inner,
            FieldKind::OptionBinding { inner } => inner,
        };

        match &field.kind {
            FieldKind::Binding { .. } => {
                quote! {
                    #name => {
                        let v: #inner = self.#ident.get();
                        return Some(::atto_ui::ComponentValueCodec::to_component_value(&v));
                    }
                }
            }
            FieldKind::OptionBinding { .. } => {
                quote! {
                    #name => {
                        if let Some(binding) = &self.#ident {
                            let v: #inner = binding.get();
                            return Some(::atto_ui::ComponentValueCodec::to_component_value(&v));
                        }
                        return None;
                    }
                }
            }
        }
    });

    let set_match_arms = binding_fields.iter().map(|field| {
        let ident = &field.ident;
        let name = &field.prop_name;
        let inner = match &field.kind {
            FieldKind::Binding { inner } => inner,
            FieldKind::OptionBinding { inner } => inner,
        };

        match &field.kind {
            FieldKind::Binding { .. } => {
                quote! {
                    #name => {
                        let v: #inner = ::atto_ui::ComponentValueCodec::from_component_value(value, name)?;
                        self.#ident.set(v);
                        return Ok(());
                    }
                }
            }
            FieldKind::OptionBinding { .. } => {
                quote! {
                    #name => {
                        let v: #inner = ::atto_ui::ComponentValueCodec::from_component_value(value, name)?;
                        if let Some(binding) = &self.#ident {
                            binding.set(v);
                        } else {
                            self.#ident = Some(::atto_ui::reactive::Binding::new(v));
                        }
                        return Ok(());
                    }
                }
            }
        }
    });

    let delegate_props = delegate_fields.iter().map(|(ident, rwlock_like, _ty)| {
        if *rwlock_like {
            quote! {
                {
                    let guard = self.#ident.read();
                    props.extend(guard.__component_property_names());
                }
            }
        } else {
            quote! {
                props.extend(self.#ident.__component_property_names());
            }
        }
    });

    let delegate_get = delegate_fields.iter().map(|(ident, rwlock_like, _ty)| {
        if *rwlock_like {
            quote! {
                {
                    let guard = self.#ident.read();
                    if let Some(v) = guard.__component_get_property(name) {
                        return Some(v);
                    }
                }
            }
        } else {
            quote! {
                if let Some(v) = self.#ident.__component_get_property(name) {
                    return Some(v);
                }
            }
        }
    });

    let delegate_set = delegate_fields.iter().map(|(ident, rwlock_like, _ty)| {
        if *rwlock_like {
            quote! {
                {
                    let mut guard = self.#ident.write();
                    if guard.__component_set_property(name, value.clone()).is_ok() {
                        return Ok(());
                    }
                }
            }
        } else {
            quote! {
                if self.#ident.__component_set_property(name, value.clone()).is_ok() {
                    return Ok(());
                }
            }
        }
    });

    let delegate_schema = delegate_fields.iter().map(|(_ident, _rwlock_like, ty)| {
        quote! {
            props.extend(<#ty as ::atto_ui::ComponentPropertySchema>::property_schema());
        }
    });

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            fn __component_property_names(&self) -> Vec<&'static str> {
                let mut props: Vec<&'static str> = Vec::new();
                #(props.push(#prop_names);)*
                #(#delegate_props)*
                props
            }

            fn __component_get_property(
                &self,
                name: &str,
            ) -> Option<::atto_ui::ComponentValue> {
                match name {
                    #(#get_match_arms)*
                    _ => {}
                }
                #(#delegate_get)*
                None
            }

            fn __component_set_property(
                &mut self,
                name: &str,
                value: ::atto_ui::ComponentValue,
            ) -> Result<(), ::atto_ui::ComponentError> {
                match name {
                    #(#set_match_arms)*
                    _ => {}
                }
                #(#delegate_set)*
                Err(::atto_ui::ComponentError::unsupported_property(name))
            }
        }

        impl #impl_generics ::atto_ui::ComponentPropertySchema for #name #ty_generics #where_clause {
            fn property_schema() -> Vec<::atto_ui::PropertyMeta> {
                let mut props = Vec::new();
                #(props.push(#prop_types);)*
                #(#delegate_schema)*
                props
            }
        }
    }
    .into()
}

pub fn component_properties_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut item_impl = parse_macro_input!(item as ItemImpl);

    let is_component = item_impl
        .trait_
        .as_ref()
        .map(|(_, path, _)| {
            path.segments
                .last()
                .map(|s| s.ident == "Component")
                .unwrap_or(false)
        })
        .unwrap_or(false);

    if !is_component {
        return item_impl.into_token_stream().into();
    }

    let mut has_props = false;
    let mut has_get = false;
    let mut has_set = false;

    for item in &item_impl.items {
        if let syn::ImplItem::Fn(func) = item {
            let name = func.sig.ident.to_string();
            match name.as_str() {
                "property_names" => has_props = true,
                "get_property" => has_get = true,
                "set_property" => has_set = true,
                _ => {}
            }
        }
    }

    let mut extra_items = Vec::new();

    if !has_props {
        extra_items.push(syn::parse_quote! {
            fn property_names(&self) -> Vec<&'static str> {
                self.__component_property_names()
            }
        });
    }

    if !has_get {
        extra_items.push(syn::parse_quote! {
            fn get_property(
                &self,
                name: &str,
            ) -> Option<::atto_ui::ComponentValue> {
                self.__component_get_property(name)
            }
        });
    }

    if !has_set {
        extra_items.push(syn::parse_quote! {
            fn set_property(
                &mut self,
                name: &str,
                value: ::atto_ui::ComponentValue,
            ) -> Result<(), ::atto_ui::ComponentError> {
                self.__component_set_property(name, value)
            }
        });
    }

    item_impl.items.extend(extra_items);

    item_impl.into_token_stream().into()
}

fn binding_kind(ty: &Type) -> Option<FieldKind> {
    if let Some(inner) = binding_inner_type(ty) {
        return Some(FieldKind::Binding { inner });
    }
    if let Some(inner) = option_binding_inner_type(ty) {
        return Some(FieldKind::OptionBinding { inner });
    }
    None
}

fn binding_inner_type(ty: &Type) -> Option<Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Binding" {
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

fn option_binding_inner_type(ty: &Type) -> Option<Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let first = args.args.first()?;
    let GenericArgument::Type(inner) = first else {
        return None;
    };
    binding_inner_type(inner)
}

fn value_type_for(ty: &Type) -> Option<TokenStream2> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last().map(|s| s.ident.to_string());
    let name = seg?;

    match name.as_str() {
        "String" => Some(quote! { ::atto_ui::ValueType::String }),
        "bool" => Some(quote! { ::atto_ui::ValueType::Bool }),
        "f64" | "f32" => Some(quote! { ::atto_ui::ValueType::F64 }),
        "i64" => Some(quote! { ::atto_ui::ValueType::I64 }),
        "u64" | "usize" | "u16" | "u32" => Some(quote! { ::atto_ui::ValueType::U64 }),
        "Rect" => Some(quote! { ::atto_ui::ValueType::Rect }),
        "EdgeInsets" => Some(quote! { ::atto_ui::ValueType::Map }),
        "DividerOrientation" => Some(quote! { ::atto_ui::ValueType::String }),
        "SplitterOrientation" => Some(quote! { ::atto_ui::ValueType::String }),
        "TabHeaderPosition" => Some(quote! { ::atto_ui::ValueType::String }),
        "WindowMinSizeMode" => Some(quote! { ::atto_ui::ValueType::String }),
        "Vec" => vec_value_type(tp),
        _ => None,
    }
}

fn vec_value_type(tp: &syn::TypePath) -> Option<TokenStream2> {
    let seg = tp.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let first = match args.args.first() {
        Some(GenericArgument::Type(ty)) => ty,
        _ => return None,
    };
    let Type::Path(inner_tp) = first else {
        return None;
    };
    let inner_seg = inner_tp.path.segments.last().map(|s| s.ident.to_string());
    if inner_seg.as_deref() == Some("String") {
        return Some(quote! { ::atto_ui::ValueType::StringList });
    }
    if inner_seg.as_deref() == Some("Vec") {
        // Vec<Vec<String>>
        return vec_value_type(inner_tp).map(|_| quote! { ::atto_ui::ValueType::Table });
    }
    None
}

fn is_rwlock_like(ty: &Type) -> bool {
    let Type::Path(tp) = ty else { return false };
    let seg = match tp.path.segments.last() {
        Some(seg) => seg,
        None => return false,
    };
    if seg.ident == "RwLock" {
        return true;
    }
    if seg.ident != "Arc" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return false;
    };
    let inner_tp = match args.args.first() {
        Some(GenericArgument::Type(Type::Path(inner_tp))) => inner_tp,
        _ => return false,
    };
    let inner_seg = inner_tp.path.segments.last();
    inner_seg.map(|s| s.ident == "RwLock").unwrap_or(false)
}

fn expr_lit_str(expr: &Expr) -> Option<String> {
    let Expr::Lit(expr_lit) = expr else {
        return None;
    };
    let syn::Lit::Str(s) = &expr_lit.lit else {
        return None;
    };
    Some(s.value())
}
