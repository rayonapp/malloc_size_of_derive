// Copyright 2016-2017 The Servo Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0>
// This file may not be copied, modified, or distributed
// except according to those terms.

//! A crate for deriving the MallocSizeOf trait.

extern crate quote;
#[macro_use]
extern crate syn;
#[macro_use]
extern crate synstructure;

extern crate proc_macro2;

#[cfg(not(test))]
decl_derive!([MallocSizeOf, attributes(ignore_malloc_size_of, with_malloc_size_of_func)] => malloc_size_of_derive);

fn malloc_size_of_derive(s: synstructure::Structure) -> proc_macro2::TokenStream {
    let match_body = s.each(|binding| {
        let ignore = binding
            .ast()
            .attrs
            .iter()
            .any(|attr| match attr.interpret_meta().unwrap() {
                syn::Meta::Word(ref ident) | syn::Meta::List(syn::MetaList { ref ident, .. })
                if ident == "ignore_malloc_size_of" =>
                    {
                        panic!(
                            "#[ignore_malloc_size_of] should have an explanation, \
                         e.g. #[ignore_malloc_size_of = \"because reasons\"]"
                        );
                    }
                syn::Meta::NameValue(syn::MetaNameValue { ref ident, .. })
                if ident == "ignore_malloc_size_of" =>
                    {
                        true
                    },
                _ => false,
            });
        let with_function : Option<syn::Path> = binding
            .ast()
            .attrs
            .iter()
            .filter_map(|attr| match attr.interpret_meta().unwrap() {
                syn::Meta::Word(ref ident) | syn::Meta::List(syn::MetaList { ref ident, .. })
                if ident == "with_malloc_size_of_func" =>
                    {
                        panic!(
                            "#[with_malloc_size_of_func] must have a function name as argument, \
                         e.g. #[with_malloc_size_of_func = \"util::measure_btreemap\"]"
                        );
                    }
                syn::Meta::NameValue(syn::MetaNameValue { ref ident, ref lit, .. })
                if ident == "with_malloc_size_of_func"  =>
                    {
                        if let syn::Lit::Str(ref lit) = lit {
                            // try to interpret the string as path
                            let as_path = lit.parse::<syn::Path>();
                            match as_path {
                                Ok(as_path) => Some(as_path),
                                Err(_err) => {
                                    panic!("The argument of #[with_malloc_size_of_func = \"...\"] must be the path to a function which is in scope.");
                                }
                            }
                        } else {
                            panic!(
                                "#[with_malloc_size_of_func] must have a function name as argument and this must be a string, \
                         e.g. #[with_malloc_size_of_func = \"util::measure_btreemap\"]"
                            );
                        }
                    },
                _ => None,
            })
            .next();
        if ignore {
            None
        } else if let Some(with_function) = with_function {
            Some(quote! {
                sum += #with_function(#binding, ops);
            })
        } else if let syn::Type::Array(..) = binding.ast().ty {
            Some(quote! {
                for item in #binding.iter() {
                    sum += ::malloc_size_of::MallocSizeOf::size_of(item, ops);
                }
            })
        } else {
            Some(quote! {
                sum += ::malloc_size_of::MallocSizeOf::size_of(#binding, ops);
            })
        }
    });

    let ast = s.ast();
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let mut where_clause = where_clause.unwrap_or(&parse_quote!(where)).clone();
    for param in ast.generics.type_params() {
        let ident = &param.ident;
        where_clause
            .predicates
            .push(parse_quote!(#ident: ::malloc_size_of::MallocSizeOf));
    }

    let tokens = quote! {
        impl #impl_generics ::malloc_size_of::MallocSizeOf for #name #ty_generics #where_clause {
            #[inline]
            #[allow(unused_variables, unused_mut, unreachable_code)]
            fn size_of(&self, ops: &mut ::malloc_size_of::MallocSizeOfOps) -> usize {
                let mut sum = 0;
                match *self {
                    #match_body
                }
                sum
            }
        }
    };

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! match_count {
        ($e: expr, $count: expr, $expanded: ident) => {
            let no_space = $expanded.replace(" ", "");
            assert_eq!(
                no_space.matches(&$e.replace(" ", "")).count(),
                $count,
                "counting occurrences of {:?} in {:?} (whitespace-insensitive)",
                $e,
                $expanded
            )
        };
    }

    #[test]
    fn test_struct() {
        let source = syn::parse_str(
            "struct Foo<T> { bar: Bar, baz: T, #[ignore_malloc_size_of = \"\"] z: Arc<T> }",
        )
        .unwrap();
        let source = synstructure::Structure::new(&source);

        let expanded = malloc_size_of_derive(source).to_string();

        match_count!("struct", 0, expanded);
        match_count!("ignore_malloc_size_of", 0, expanded);
        match_count!("impl<T> ::malloc_size_of::MallocSizeOf for Foo<T> where T: ::malloc_size_of::MallocSizeOf {", 1, expanded);
        match_count!(
            "sum += ::malloc_size_of::MallocSizeOf::size_of(",
            2,
            expanded
        );

        let source = syn::parse_str("struct Bar([Baz; 3]);").unwrap();
        let source = synstructure::Structure::new(&source);
        let expanded = malloc_size_of_derive(source).to_string();
        match_count!("for item in", 1, expanded);
    }

    #[should_panic(expected = "should have an explanation")]
    #[test]
    fn test_no_reason() {
        let input = syn::parse_str("struct A { #[ignore_malloc_size_of] b: C }").unwrap();
        malloc_size_of_derive(synstructure::Structure::new(&input));
    }

    #[test]
    fn test_with_function() {
        let source = syn::parse_str(
            "struct Foo { bar: Bar, #[with_malloc_size_of_func = \"col::anothermod::custom_func\"] baz: Baz,
        #[with_malloc_size_of_func = \"my_func\"] 
        baz2: Baz}",
        ).unwrap();

        let source = synstructure::Structure::new(&source);

        let expanded = malloc_size_of_derive(source).to_string();

        match_count!("struct", 0, expanded);
        match_count!("ignore_malloc_size_of", 0, expanded);
        match_count!("impl::malloc_size_of::MallocSizeOf for Foo {", 1, expanded);
        match_count!(
            "sum += ::malloc_size_of::MallocSizeOf::size_of(",
            1,
            expanded
        );
        match_count!("sum += col::anothermod::custom_func(", 1, expanded);
        match_count!("sum += my_func(", 1, expanded);
    }
}
