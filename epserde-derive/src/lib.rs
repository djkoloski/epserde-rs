/*
 * SPDX-FileCopyrightText: 2023 Inria
 * SPDX-FileCopyrightText: 2023 Sebastiano Vigna
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

//!
//! Derive procedural macros for the [`epserde`](https://crates.io/crates/epserde) crate.
//!

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Data, DeriveInput};

struct CommonDeriveInput {
    name: syn::Ident,
    generics: proc_macro2::TokenStream,
    generics_names: proc_macro2::TokenStream,
    generics_names_raw: Vec<String>,
    consts_names_raw: Vec<String>,
    where_clause: proc_macro2::TokenStream,
}

impl CommonDeriveInput {
    fn new(
        input: DeriveInput,
        traits_to_add: Vec<syn::Path>,
        lifetimes_to_add: Vec<syn::Lifetime>,
    ) -> Self {
        let name = input.ident;
        let mut generics = quote!();
        let mut generics_names_raw = vec![];
        let mut consts_names_raw = vec![];
        let mut generics_names = quote!();
        if !input.generics.params.is_empty() {
            input.generics.params.iter().for_each(|x| {
                match x {
                    syn::GenericParam::Type(t) => {
                        generics_names.extend(t.ident.to_token_stream());
                        generics_names_raw.push(t.ident.to_string());
                    }
                    syn::GenericParam::Lifetime(l) => {
                        generics_names.extend(l.lifetime.to_token_stream());
                    }
                    syn::GenericParam::Const(c) => {
                        generics_names.extend(c.ident.to_token_stream());
                        consts_names_raw.push(c.ident.to_string());
                    }
                };
                generics_names.extend(quote!(,))
            });
            input.generics.params.into_iter().for_each(|x| match x {
                syn::GenericParam::Type(t) => {
                    let mut t = t;
                    for trait_to_add in traits_to_add.iter() {
                        t.bounds.push(syn::TypeParamBound::Trait(syn::TraitBound {
                            paren_token: None,
                            modifier: syn::TraitBoundModifier::None,
                            lifetimes: None,
                            path: trait_to_add.clone(),
                        }));
                    }
                    for lifetime_to_add in lifetimes_to_add.iter() {
                        t.bounds
                            .push(syn::TypeParamBound::Lifetime(lifetime_to_add.clone()));
                    }
                    generics.extend(quote!(#t,));
                }
                x => {
                    generics.extend(x.to_token_stream());
                    generics.extend(quote!(,))
                }
            });
        }

        let where_clause = input
            .generics
            .where_clause
            .map(|x| x.to_token_stream())
            .unwrap_or(quote!(where));

        Self {
            name,
            generics,
            generics_names,
            where_clause,
            generics_names_raw,
            consts_names_raw,
        }
    }
}

fn check_attrs(input: &DeriveInput) -> (bool, bool, bool) {
    let is_repr_c = input.attrs.iter().any(|x| {
        x.meta.path().is_ident("repr") && x.meta.require_list().unwrap().tokens.to_string() == "C"
    });
    let is_zero_copy = input
        .attrs
        .iter()
        .any(|x| x.meta.path().is_ident("zero_copy"));
    let is_full_copy = input
        .attrs
        .iter()
        .any(|x| x.meta.path().is_ident("full_copy"));
    if is_zero_copy && !is_repr_c {
        panic!(
            "Type {} is declared as zero copy, but it is not repr(C)",
            input.ident
        );
    }
    if is_zero_copy && is_full_copy {
        panic!(
            "Type {} is declared as both zero copy and full copy",
            input.ident
        );
    }

    (is_repr_c, is_zero_copy, is_full_copy)
}

#[proc_macro_derive(Serialize, attributes(zero_copy, full_copy))]
pub fn epserde_serialize_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let (is_repr_c, is_zero_copy, is_full_copy) = check_attrs(&input);
    let CommonDeriveInput {
        name,
        generics,
        generics_names,
        where_clause,
        generics_names_raw,
        ..
    } = CommonDeriveInput::new(
        input.clone(),
        vec![syn::parse_quote!(epserde::ser::SerializeInner)],
        vec![],
    );
    // We have to play with this to get type parameters working

    let out = match input.data {
        Data::Struct(s) => {
            let mut fields = vec![];
            let mut fields_names = vec![];
            let mut non_generic_fields = vec![];
            let mut non_generic_types = vec![];
            let mut generic_fields = vec![];
            let mut generic_types = vec![];

            s.fields.iter().for_each(|field| {
                let ty = &field.ty;
                let field_name = field.ident.clone().unwrap();
                if generics_names_raw.contains(&ty.to_token_stream().to_string()) {
                    generic_fields.push(field_name.clone());
                    generic_types.push(ty);
                } else {
                    non_generic_fields.push(field_name.clone());
                    non_generic_types.push(ty);
                }
                fields.push(ty);
                fields_names.push(field_name);
            });

            if is_zero_copy {
                quote! {
                    #[automatically_derived]
                    impl<#generics> epserde::ser::SerializeInner for #name<#generics_names> #where_clause {
                        const IS_ZERO_COPY: bool = #is_repr_c #(
                            && <#fields>::IS_ZERO_COPY
                        )*;

                        const ZERO_COPY_MISMATCH: bool = false;

                        #[inline(always)]
                        fn _serialize_inner<F: epserde::ser::FieldWrite>(&self, mut backend: F) -> epserde::ser::Result<F> {
                            if ! Self::IS_ZERO_COPY {
                                panic!("Cannot serialize non zero-copy type {} declared as zero copy", core::any::type_name::<Self>());
                            }
                            backend.add_padding_to_align(core::mem::align_of::<Self>())?;
                            #(
                                backend= backend.add_field(stringify!(#fields_names), &self.#fields_names)?;
                            )*
                            Ok(backend)
                        }
                    }
                }
            } else {
                quote! {
                    #[automatically_derived]
                    impl<#generics> epserde::ser::SerializeInner for #name<#generics_names> #where_clause {
                        const IS_ZERO_COPY: bool = #is_repr_c #(
                            && <#fields>::IS_ZERO_COPY
                        )*;

                        const ZERO_COPY_MISMATCH: bool = ! #is_full_copy #(&& <#fields>::IS_ZERO_COPY)*;

                        #[inline(always)]
                        fn _serialize_inner<F: epserde::ser::FieldWrite>(&self, mut backend: F) -> epserde::ser::Result<F> {
                            if Self::ZERO_COPY_MISMATCH {
                                eprintln!("Type {} is zero copy, but it has not declared as such; use the #full_copy attribute to silence this warning", core::any::type_name::<Self>());
                            }
                            #(
                                backend= backend.add_field(stringify!(#fields_names), &self.#fields_names)?;
                            )*
                            Ok(backend)
                        }
                    }
                }
            }
        }
        _ => todo!(),
    };
    out.into()
}

#[proc_macro_derive(Deserialize, attributes(zero_copy, full_copy))]
pub fn epserde_deserialize_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let (_, is_zero_copy, _) = check_attrs(&input);
    let CommonDeriveInput {
        name,
        generics_names_raw,
        generics,
        generics_names,
        where_clause,
        ..
    } = CommonDeriveInput::new(
        input.clone(),
        vec![syn::parse_quote!(epserde::des::DeserializeInner)],
        vec![],
    );
    let out = match input.data {
        Data::Struct(s) => {
            let fields = s
                .fields
                .iter()
                .map(|field| field.ident.to_owned().unwrap())
                .collect::<Vec<_>>();

            let fields_types = s
                .fields
                .iter()
                .map(|field| field.ty.to_owned())
                .collect::<Vec<_>>();

            let mut generic_types = vec![];
            let mut methods: Vec<proc_macro2::TokenStream> = vec![];

            s.fields.iter().for_each(|field| {
                let ty = &field.ty;
                if generics_names_raw.contains(&ty.to_token_stream().to_string()) {
                    generic_types.push(ty);
                    methods.push(syn::parse_quote!(_deserialize_eps_copy_inner));
                } else {
                    methods.push(syn::parse_quote!(_deserialize_full_copy_inner));
                }
            });

            if is_zero_copy {
                quote! {
                    #[automatically_derived]
                    impl<#generics> epserde::des::DeserializeInner for #name<#generics_names> #where_clause
                    #(
                        #generic_types: epserde::des::DeserializeInner,
                    )*{
                        fn _deserialize_full_copy_inner(
                            mut backend: epserde::des::Cursor,
                        ) -> core::result::Result<(Self, epserde::des::Cursor), epserde::des::DeserializeError> {
                            use epserde::des::DeserializeInner;
                            backend = Self::pad_align_and_check(backend)?;
                            #(
                                let (#fields, backend) = <#fields_types>::_deserialize_full_copy_inner(backend)?;
                            )*
                            Ok((#name{
                                #(#fields),*
                            }, backend))
                        }

                        type DeserType<'a> = &'a #name<#(
                            <#generic_types as epserde::des::DeserializeInner>::DeserType<'a>
                        ,)*>;

                        fn _deserialize_eps_copy_inner(
                            backend: epserde::des::Cursor,
                        ) -> core::result::Result<(Self::DeserType<'_>, epserde::des::Cursor), epserde::des::DeserializeError>
                        {
                            let mut backend = backend;
                            let bytes = core::mem::size_of::<Self>();
                            backend = Self::pad_align_and_check(backend)?;
                            let (pre, data, after) = unsafe { backend.data[..bytes].align_to::<Self>() };
                            debug_assert!(pre.is_empty());
                            debug_assert!(after.is_empty());
                            Ok((&data[0], backend.skip(bytes)))
                        }
                    }
                }
            } else {
                quote! {
                    #[automatically_derived]
                    impl<#generics> epserde::des::DeserializeInner for #name<#generics_names> #where_clause
                    #(
                        #generic_types: epserde::des::DeserializeInner,
                    )*{
                        fn _deserialize_full_copy_inner(
                            backend: epserde::des::Cursor,
                        ) -> core::result::Result<(Self, epserde::des::Cursor), epserde::des::DeserializeError> {
                            use epserde::des::DeserializeInner;
                            #(
                                let (#fields, backend) = <#fields_types>::_deserialize_full_copy_inner(backend)?;
                            )*
                            Ok((#name{
                                #(#fields),*
                            }, backend))
                        }

                        type DeserType<'a> = #name<#(
                            <#generic_types as epserde::des::DeserializeInner>::DeserType<'a>
                        ,)*>;

                        fn _deserialize_eps_copy_inner(
                            backend: epserde::des::Cursor,
                        ) -> core::result::Result<(Self::DeserType<'_>, epserde::des::Cursor), epserde::des::DeserializeError>
                        {
                            use epserde::des::DeserializeInner;
                            #(
                                let (#fields, backend) = <#fields_types>::#methods(backend)?;
                            )*
                            Ok((#name{
                                #(#fields),*
                            }, backend))
                        }
                    }
                }
            }
        }
        _ => todo!(),
    };
    out.into()
}

#[proc_macro_derive(TypeHash)]
pub fn epserde_type_hash(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let (_, is_zero_copy, _) = check_attrs(&input);
    let CommonDeriveInput {
        name,
        generics,
        generics_names,
        where_clause,
        generics_names_raw,
        consts_names_raw,
    } = CommonDeriveInput::new(
        input.clone(),
        vec![syn::parse_quote!(epserde::TypeHash)],
        vec![],
    );

    let out = match input.data {
        Data::Struct(s) => {
            let fields_names = s
                .fields
                .iter()
                .map(|field| field.ident.to_owned().unwrap().to_string())
                .collect::<Vec<_>>();

            let fields_types = s
                .fields
                .iter()
                .map(|field| field.ty.to_owned())
                .collect::<Vec<_>>();

            let type_name: proc_macro2::TokenStream = if generics.is_empty() {
                format!("\"{}\".into()", name)
            } else {
                let mut res = "format!(\"".to_string();
                res += &name.to_string();
                res += "<";
                for _ in 0..generics_names_raw.len() + consts_names_raw.len() {
                    res += "{}, ";
                }
                res.pop();
                res.pop();
                res += ">\",";

                for gn in generics_names_raw.iter() {
                    res += &format!("{}::type_name()", gn);
                    res += ",";
                }
                res.pop();
                res += ")";
                res
            }
            .parse()
            .unwrap();

            let name_literal = format!("\"{}\"", type_name);

            let repr = input
                .attrs
                .iter()
                .filter(|x| x.meta.path().is_ident("repr"))
                .map(|x| x.meta.require_list().unwrap().tokens.to_string())
                .collect::<Vec<_>>();

            quote! {
                #[automatically_derived]
                impl<#generics> epserde::TypeHash for #name<#generics_names> #where_clause{
                    #[inline(always)]
                    fn type_hash(hasher: &mut impl core::hash::Hasher) {
                        use core::hash::Hash;
                        #is_zero_copy.hash(hasher);
                        #(
                            #repr.hash(hasher);
                        )*
                        #name_literal.hash(hasher);
                        #(
                            #fields_names.hash(hasher);
                        )*
                        #(
                            <#fields_types as epserde::TypeHash>::type_hash(hasher);
                        )*
                    }
                }
            }
        }
        _ => todo!(),
    };
    out.into()
}
